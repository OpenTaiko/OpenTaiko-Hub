// Native filesystem operations: fast merge-moves for installing builds (rename first,
// copy+delete fallback for cross-device moves) and the one-shot migration of instance
// song libraries into the shared global Songs folder.
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), String> {
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

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MigrateSummary {
    pub moved: usize,
    pub skipped: usize,
}

/// A song folder is a directory that directly contains at least one .tja file.
/// Song folders are moved as a whole, so nested content stays intact.
fn collect_song_dirs(dir: &Path, out: &mut Vec<PathBuf>) {
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
    if has_tja {
        out.push(dir.to_path_buf());
        return;
    }
    for sub in subdirs {
        collect_song_dirs(&sub, out);
    }
}

/// Moves every song folder found under `src_songs` to the same relative location under
/// `dest_songs`, skipping songs that already exist there. Genre metadata (box.def and
/// default.png) of ancestor folders is copied over when the destination lacks it.
#[tauri::command]
pub async fn migrate_songs(src_songs: String, dest_songs: String) -> Result<MigrateSummary, String> {
    tokio::task::spawn_blocking(move || migrate_songs_sync(&src_songs, &dest_songs))
        .await
        .map_err(|e| format!("Task failed: {e}"))?
}

pub fn migrate_songs_sync(src_songs: &str, dest_songs: &str) -> Result<MigrateSummary, String> {
    {
        let src = PathBuf::from(src_songs);
        let dest = PathBuf::from(dest_songs);
        if !src.is_dir() {
            return Ok(MigrateSummary { moved: 0, skipped: 0 });
        }
        std::fs::create_dir_all(&dest).map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;

        let mut song_dirs = Vec::new();
        collect_song_dirs(&src, &mut song_dirs);

        let mut moved = 0;
        let mut skipped = 0;
        let mut moved_rels: Vec<PathBuf> = Vec::new();

        for dir in song_dirs {
            let rel = match dir.strip_prefix(&src) {
                Ok(rel) => rel.to_path_buf(),
                Err(_) => continue,
            };
            let target = dest.join(&rel);
            if target.exists() {
                skipped += 1;
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
            }
            if std::fs::rename(&dir, &target).is_err() {
                copy_dir_recursive(&dir, &target)?;
                std::fs::remove_dir_all(&dir)
                    .map_err(|e| format!("Failed to remove {}: {}", dir.display(), e))?;
            }
            moved += 1;
            moved_rels.push(rel);
        }

        // Bring over genre metadata for every ancestor of a moved song
        let mut seen: HashSet<PathBuf> = HashSet::new();
        for rel in &moved_rels {
            let mut current = rel.parent();
            while let Some(ancestor) = current {
                if ancestor.as_os_str().is_empty() {
                    break;
                }
                if seen.insert(ancestor.to_path_buf()) {
                    for file in ["box.def", "default.png"] {
                        let src_file = src.join(ancestor).join(file);
                        let dest_file = dest.join(ancestor).join(file);
                        if src_file.is_file() && !dest_file.exists() {
                            let _ = std::fs::copy(&src_file, &dest_file);
                        }
                    }
                }
                current = ancestor.parent();
            }
        }

        Ok(MigrateSummary { moved, skipped })
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

    #[test]
    fn migrate_moves_songs_and_genre_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("instance_songs");
        let dest = tmp.path().join("global_songs");

        // Genre folder with metadata containing two songs, one already in dest
        write(&src.join("01 Pop/box.def"), "#TITLE:Pop");
        write(&src.join("01 Pop/default.png"), "png");
        write(&src.join("01 Pop/Song A/song.tja"), "TITLE:A");
        write(&src.join("01 Pop/Song A/uniqueID.json"), "{\"id\":\"a\"}");
        write(&src.join("01 Pop/Song B/song.tja"), "TITLE:B");
        write(&dest.join("01 Pop/Song B/song.tja"), "TITLE:B-existing");
        // Scaffolding folder without songs must stay untouched
        write(&src.join("X1 Favorite/box.def"), "#TITLE:Fav");

        let summary = migrate_songs_sync(src.to_str().unwrap(), dest.to_str().unwrap()).unwrap();

        assert_eq!(summary.moved, 1);
        assert_eq!(summary.skipped, 1);
        assert_eq!(fs::read_to_string(dest.join("01 Pop/Song A/song.tja")).unwrap(), "TITLE:A");
        assert_eq!(fs::read_to_string(dest.join("01 Pop/box.def")).unwrap(), "#TITLE:Pop");
        assert!(dest.join("01 Pop/default.png").is_file());
        // Skipped song keeps the destination version and stays in the source
        assert_eq!(fs::read_to_string(dest.join("01 Pop/Song B/song.tja")).unwrap(), "TITLE:B-existing");
        assert!(src.join("01 Pop/Song B/song.tja").is_file());
        // Songless scaffolding is not migrated
        assert!(!dest.join("X1 Favorite").exists());
    }
}
