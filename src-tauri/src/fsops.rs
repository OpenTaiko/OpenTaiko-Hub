// Native filesystem operations: fast merge-moves for installing builds (rename first,
// copy+delete fallback for cross-device moves). Song-library migration lives in
// migrate.rs and reuses the shared helpers exposed here.
use std::path::{Path, PathBuf};

pub(crate) fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;
    for entry in std::fs::read_dir(src)
        .map_err(|e| format!("Failed to read {}: {}", src.display(), e))?
        .flatten()
    {
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|e| format!("Failed to stat {}: {}", src_path.display(), e))?;
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else if file_type.is_file() {
            std::fs::copy(&src_path, &dest_path)
                .map_err(|e| format!("Failed to copy {}: {}", src_path.display(), e))?;
        }
    }
    Ok(())
}

fn merge_move(src: &Path, dest: &Path) -> Result<(), String> {
    if src.is_dir() {
        if dest.is_file() {
            std::fs::remove_file(dest).map_err(|e| format!("Failed to remove {}: {}", dest.display(), e))?;
        }
        if !dest.exists() && std::fs::rename(src, dest).is_ok() {
            return Ok(());
        }
        std::fs::create_dir_all(dest).map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;
        for entry in std::fs::read_dir(src)
            .map_err(|e| format!("Failed to read {}: {}", src.display(), e))?
            .flatten()
        {
            merge_move(&entry.path(), &dest.join(entry.file_name()))?;
        }
        let _ = std::fs::remove_dir(src);
        Ok(())
    } else {
        if dest.exists() {
            std::fs::remove_file(dest).map_err(|e| format!("Failed to replace {}: {}", dest.display(), e))?;
        }
        if std::fs::rename(src, dest).is_ok() {
            return Ok(());
        }
        std::fs::copy(src, dest).map_err(|e| format!("Failed to copy {}: {}", src.display(), e))?;
        std::fs::remove_file(src).map_err(|e| format!("Failed to remove {}: {}", src.display(), e))?;
        Ok(())
    }
}

/// Moves the CONTENT of `src` into `dest` (merging into existing folders, replacing
/// existing files), then removes the emptied `src`.
#[tauri::command]
pub async fn merge_move_dir(src: String, dest: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || merge_move_dir_sync(&src, &dest))
        .await
        .map_err(|e| format!("Task failed: {e}"))?
}

/// A song folder is a directory that directly contains at least one .tja file.
/// Song folders are moved as a whole, so nested content stays intact.
///
/// The starting folder itself is never reported as a song: a loose .tja dropped into a
/// Songs folder would otherwise make the whole library look like a single song.
pub(crate) fn collect_song_dirs(base: &Path, out: &mut Vec<PathBuf>) {
    collect_song_dirs_from(base, base, out);
}

fn collect_song_dirs_from(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    let mut subdirs = Vec::new();
    let mut has_tja = false;
    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            subdirs.push(entry.path());
        } else if file_type.is_file()
            && entry.file_name().to_string_lossy().to_lowercase().ends_with(".tja")
        {
            has_tja = true;
        }
    }
    if has_tja && dir != base {
        out.push(dir.to_path_buf());
        return;
    }
    for sub in subdirs {
        collect_song_dirs_from(&sub, base, out);
    }
}

pub fn merge_move_dir_sync(src: &str, dest: &str) -> Result<(), String> {
    let src_path = PathBuf::from(src);
    let dest_path = PathBuf::from(dest);
    if !src_path.is_dir() {
        return Err(format!("Source folder not found: {}", src));
    }
    std::fs::create_dir_all(&dest_path)
        .map_err(|e| format!("Failed to create {}: {}", dest_path.display(), e))?;
    for entry in std::fs::read_dir(&src_path)
        .map_err(|e| format!("Failed to read {}: {}", src_path.display(), e))?
        .flatten()
    {
        merge_move(&entry.path(), &dest_path.join(entry.file_name()))?;
    }
    let _ = std::fs::remove_dir(&src_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn merge_move_merges_and_replaces() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        write(&src.join("a.txt"), "new-a");
        write(&src.join("sub/b.txt"), "new-b");
        write(&dest.join("a.txt"), "old-a");
        write(&dest.join("sub/keep.txt"), "keep");

        merge_move_dir_sync(src.to_str().unwrap(), dest.to_str().unwrap()).unwrap();

        assert_eq!(fs::read_to_string(dest.join("a.txt")).unwrap(), "new-a");
        assert_eq!(fs::read_to_string(dest.join("sub/b.txt")).unwrap(), "new-b");
        assert_eq!(fs::read_to_string(dest.join("sub/keep.txt")).unwrap(), "keep");
        assert!(!src.exists(), "source should be gone after the move");
    }
}
