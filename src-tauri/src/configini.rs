// Edits the game's Config.ini to link an instance to the shared global Songs folder.
// The game reads and writes Config.ini as Shift_JIS (OpenTaiko.sEncType), so all
// I/O here must go through that encoding — writing UTF-8 from JS would corrupt
// non-ASCII paths.
use encoding_rs::SHIFT_JIS;
use std::path::{Component, Path, PathBuf};

fn normalize_for_compare(path: &str) -> String {
    let normalized = path.trim().replace('\\', "/");
    let normalized = normalized.trim_end_matches('/').to_string();
    if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    }
}

fn fits_shift_jis(text: &str) -> bool {
    !SHIFT_JIS.encode(text).2
}

/// Resolves `..` and `.` from the text alone, without touching the disk.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

fn same_component(a: &Component, b: &Component) -> bool {
    let (a, b) = (a.as_os_str().to_string_lossy(), b.as_os_str().to_string_lossy());
    if cfg!(windows) { a.eq_ignore_ascii_case(&b) } else { a == b }
}

/// `to` written relative to `from_dir`, or None when they share no root (two
/// Windows drives) or either is not absolute.
fn relative_path(from_dir: &Path, to: &Path) -> Option<PathBuf> {
    if !from_dir.is_absolute() || !to.is_absolute() {
        return None;
    }
    let from_norm = lexical_normalize(from_dir);
    let to_norm = lexical_normalize(to);
    let from: Vec<Component> = from_norm.components().collect();
    let to: Vec<Component> = to_norm.components().collect();
    // Both must start from the same root: the same drive on Windows, "/" elsewhere
    let root_len = |parts: &[Component]| parts.iter().take_while(|p| matches!(p, Component::Prefix(_) | Component::RootDir)).count();
    let root = root_len(&from);
    if root == 0 || root_len(&to) != root || !from[..root].iter().zip(&to[..root]).all(|(a, b)| same_component(a, b)) {
        return None;
    }
    let common = from.iter().zip(&to).take_while(|(a, b)| same_component(a, b)).count();
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for part in &to[common..] {
        out.push(part);
    }
    Some(out)
}

/// What a TJAPath entry points to, for telling whether two entries are the same folder
/// however they are written: relative entries are resolved against the game's folder,
/// the way the game itself reads them.
fn entry_key(instance_dir: &Path, entry: &str) -> String {
    let path = Path::new(entry.trim());
    let resolved = if path.is_absolute() { lexical_normalize(path) } else { lexical_normalize(&instance_dir.join(path)) };
    normalize_for_compare(&resolved.to_string_lossy())
}

/// How `songs` is written in the TJAPath of the game in `instance_dir`, or None when it
/// cannot be. The game reads Config.ini as Shift_JIS, which cannot store every character:
/// a home folder such as ~/문서 or /home/rené cannot be written there. The game also
/// accepts a path relative to its own folder, which for an instance next to the library
/// is just ../Songs/, so that is used when the full path does not fit.
fn songs_entry(instance_dir: &Path, songs: &str) -> Option<String> {
    let sep = std::path::MAIN_SEPARATOR;
    let trimmed = songs.trim_end_matches(['/', '\\']);
    let absolute = format!("{}{}", trimmed, sep);
    if fits_shift_jis(&absolute) {
        return Some(absolute);
    }
    let relative = format!("{}{}", relative_path(instance_dir, Path::new(trimmed))?.to_string_lossy(), sep);
    fits_shift_jis(&relative).then_some(relative)
}

/// Replaces the TJAPath entry naming `old` with `new`, however the entry was written.
/// Used when the library folder moves (relocate.rs). Returns whether anything changed;
/// an instance without a Config.ini, or one not naming `old`, is left alone.
#[cfg_attr(any(debug_assertions, target_os = "windows"), allow(dead_code))]
pub(crate) fn repoint_songs_entry(instance_dir: &Path, old: &Path, new: &Path) -> Result<bool, String> {
    let ini_path = instance_dir.join("Config.ini");
    if !ini_path.is_file() {
        return Ok(false);
    }
    let bytes = std::fs::read(&ini_path).map_err(|e| format!("Failed to read Config.ini: {}", e))?;
    let (text, _, _) = SHIFT_JIS.decode(&bytes);
    let mut lines: Vec<String> = text.replace("\r\n", "\n").split('\n').map(String::from).collect();
    let old_key = entry_key(instance_dir, &old.to_string_lossy());
    let Some(line) = lines.iter_mut().find(|line| line.trim_start().starts_with("TJAPath=")) else { return Ok(false) };
    let entries: Vec<&str> = line
        .trim_start()
        .trim_start_matches("TJAPath=")
        .split(';')
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .collect();
    let changed = entries.iter().any(|entry| entry_key(instance_dir, entry) == old_key);
    if changed {
        // Only now: an instance that does not name the old folder is no reason to fail
        let replacement = songs_entry(instance_dir, &new.to_string_lossy())
            .ok_or_else(|| format!("{} cannot be stored in Config.ini (Shift_JIS)", new.display()))?;
        let rewritten: Vec<String> = entries
            .iter()
            .map(|entry| if entry_key(instance_dir, entry) == old_key { replacement.clone() } else { entry.to_string() })
            .collect();
        *line = format!("TJAPath={}", rewritten.join(";"));
    }
    if changed {
        let text = lines.join("\r\n");
        let (encoded, _, _) = SHIFT_JIS.encode(&text);
        std::fs::write(&ini_path, &encoded).map_err(|e| format!("Failed to write Config.ini: {}", e))?;
    }
    Ok(changed)
}

/// Ensures `[System] TJAPath` in `<instance_dir>/Config.ini` contains the global songs
/// path (appended to whatever entries already exist; the file is created with the
/// default local `Songs` entry plus the global path when missing).
/// With `include_local` false (experimental builds), the default local `Songs` entry is
/// removed instead, so the game only reads the shared library; custom paths are kept.
/// Returns the resulting TJAPath value.
///
/// The library is the Hub's own Songs folder, decided here: a path the page supplied was
/// written into the file as it came, a line break and a network share included.
#[tauri::command]
pub async fn ensure_config_tjapath(app: tauri::AppHandle, instance_dir: String, include_local: bool) -> Result<String, String> {
    let dir = crate::guard::instance_dir(&app, &instance_dir)?;
    let songs = crate::hub_root_dir().ok_or("The Hub folder could not be resolved")?.join("Songs");
    // A sync command runs on the main thread, which also drives the window
    crate::library::blocking(move || {
        write_tjapath(dir.to_string_lossy().into_owned(), songs.to_string_lossy().into_owned(), include_local)
    })
    .await
}

/// A TJAPath entry is one of a `;`-separated list on one line.
fn fits_one_entry(path: &str) -> bool {
    !path.chars().any(|c| c == ';' || c.is_control())
}

fn write_tjapath(
    instance_dir: String,
    global_songs_path: String,
    include_local: bool,
) -> Result<String, String> {
    let dir = PathBuf::from(&instance_dir);
    if !dir.is_dir() {
        return Err(format!("Instance folder not found: {}", instance_dir));
    }
    if !fits_one_entry(&global_songs_path) {
        return Err(format!("The songs path cannot be written into Config.ini: {}", global_songs_path));
    }

    let sep = std::path::MAIN_SEPARATOR;
    let global_entry = songs_entry(&dir, &global_songs_path).ok_or_else(|| {
        format!(
            "The songs path contains characters that cannot be stored in Config.ini (Shift_JIS): {}",
            global_songs_path
        )
    })?;

    let ini_path = dir.join("Config.ini");
    let default_local = format!("Songs{}", sep);

    let mut lines: Vec<String> = if ini_path.is_file() {
        let bytes = std::fs::read(&ini_path).map_err(|e| format!("Failed to read Config.ini: {}", e))?;
        let (text, _, _) = SHIFT_JIS.decode(&bytes);
        text.replace("\r\n", "\n")
            .split('\n')
            .map(|s| s.to_string())
            .collect()
    } else {
        vec!["[System]".to_string()]
    };

    let global_key = entry_key(&dir, &global_entry);
    let result_value: String;
    let mut tja_line_index: Option<usize> = None;

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(';') {
            continue;
        }
        if trimmed.starts_with("TJAPath=") {
            tja_line_index = Some(index);
            break;
        }
    }

    let local_norm = normalize_for_compare(&default_local);

    match tja_line_index {
        Some(index) => {
            let value = lines[index].trim_start().trim_start_matches("TJAPath=").to_string();
            let mut entries: Vec<String> = value
                .split(';')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            let mut changed = false;
            if !include_local {
                let before = entries.len();
                entries.retain(|entry| normalize_for_compare(entry) != local_norm);
                changed = entries.len() != before;
            }
            if !entries.iter().any(|entry| entry_key(&dir, entry) == global_key) {
                entries.push(global_entry.clone());
                changed = true;
            }
            if !changed {
                // Nothing to do; leave the user's file untouched
                return Ok(entries.join(";"));
            }
            result_value = entries.join(";");
            lines[index] = format!("TJAPath={}", result_value);
        }
        None => {
            result_value = if include_local {
                format!("{};{}", default_local, global_entry)
            } else {
                global_entry.clone()
            };
            let new_line = format!("TJAPath={}", result_value);
            let system_index = lines.iter().position(|line| line.trim() == "[System]");
            match system_index {
                Some(index) => lines.insert(index + 1, new_line),
                None => {
                    lines.push("[System]".to_string());
                    lines.push(new_line);
                }
            }
        }
    }

    let text = lines.join("\r\n");
    let (encoded, _, _) = SHIFT_JIS.encode(&text);
    std::fs::write(&ini_path, &encoded).map_err(|e| format!("Failed to write Config.ini: {}", e))?;

    Ok(result_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_config_with_local_and_global_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap().to_string();
        let sep = std::path::MAIN_SEPARATOR;

        let value = write_tjapath(dir.clone(), "D:/Global/Songs".into(), true).unwrap();
        assert_eq!(value, format!("Songs{sep};D:/Global/Songs{sep}"));

        let bytes = std::fs::read(tmp.path().join("Config.ini")).unwrap();
        let (text, _, _) = SHIFT_JIS.decode(&bytes);
        assert!(text.contains("[System]"));
        assert!(text.contains(&format!("TJAPath=Songs{sep};D:/Global/Songs{sep}")));
    }

    #[test]
    fn is_idempotent_and_preserves_existing_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap().to_string();

        // Existing Shift_JIS Config.ini with a comment, another key, and a custom path
        let original = "[System]\r\n; 譜面フォルダ\r\nTJAPath=Songs\\;E:\\custom\\\r\nFullScreen=0\r\n";
        let (encoded, _, _) = SHIFT_JIS.encode(original);
        std::fs::write(tmp.path().join("Config.ini"), &encoded).unwrap();

        let first = write_tjapath(dir.clone(), "D:/Global/Songs".into(), true).unwrap();
        let second = write_tjapath(dir.clone(), "D:/Global/Songs".into(), true).unwrap();
        assert_eq!(first, second, "second call must not change the value");
        assert_eq!(first.matches("D:/Global/Songs").count(), 1, "no duplicate entries");

        let bytes = std::fs::read(tmp.path().join("Config.ini")).unwrap();
        let (text, _, _) = SHIFT_JIS.decode(&bytes);
        assert!(text.contains("; 譜面フォルダ"), "comments must survive the rewrite");
        assert!(text.contains("E:\\custom\\"), "existing custom paths must survive");
        assert!(text.contains("FullScreen=0"), "other keys must survive");
    }

    #[test]
    fn a_path_that_would_add_an_entry_or_a_line_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap().to_string();
        for songs in ["D:/Songs\r\nSkinPath=\\\\host\\share", "D:/a;\\\\host\\share", "D:/a\tb"] {
            assert!(write_tjapath(dir.clone(), songs.into(), true).is_err(), "{songs:?}");
        }
        assert!(!tmp.path().join("Config.ini").exists());
    }

    #[test]
    fn rejects_paths_not_representable_in_shift_jis() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap().to_string();
        // é is not representable in Shift_JIS
        assert!(write_tjapath(dir, "D:/Généreux/Songs".into(), true).is_err());
    }

    #[test]
    fn experimental_mode_drops_local_songs_but_keeps_custom_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap().to_string();
        let sep = std::path::MAIN_SEPARATOR;

        // Fresh file: only the global path
        let value = write_tjapath(dir.clone(), "D:/Global/Songs".into(), false).unwrap();
        assert_eq!(value, format!("D:/Global/Songs{sep}"));

        // Existing file with the bundled Songs entry and a custom path
        let original = "[System]\r\nTJAPath=Songs\\;E:\\custom\\\r\n";
        let (encoded, _, _) = SHIFT_JIS.encode(original);
        std::fs::write(tmp.path().join("Config.ini"), &encoded).unwrap();

        let value = write_tjapath(dir.clone(), "D:/Global/Songs".into(), false).unwrap();
        assert!(!value.split(';').any(|e| e == "Songs\\" || e == "Songs/"), "local Songs must be removed");
        assert!(value.contains("E:\\custom\\"), "custom paths must be kept");
        assert!(value.contains("D:/Global/Songs"), "global path must be present");

        // Idempotent
        let again = write_tjapath(dir, "D:/Global/Songs".into(), false).unwrap();
        assert_eq!(value, again);
    }

    // Korean is not in Shift_JIS: ~/문서 (the Documents folder of a Korean Linux
    // install) could not be written, and the game found no songs
    #[test]
    fn a_path_shift_jis_cannot_store_is_written_relative_to_the_game() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("문서");
        let instance = root.join("OpenTaiko-1");
        std::fs::create_dir_all(&instance).unwrap();
        let sep = std::path::MAIN_SEPARATOR;

        let songs = root.join("Songs").to_string_lossy().into_owned();
        let value = write_tjapath(instance.to_string_lossy().into_owned(), songs.clone(), false).unwrap();
        assert_eq!(value, format!("..{sep}Songs{sep}"));

        // Written again with the same folder, it is recognised and not added twice
        let again = write_tjapath(instance.to_string_lossy().into_owned(), songs, false).unwrap();
        assert_eq!(again, value);
    }

    #[test]
    fn an_entry_for_the_same_folder_written_differently_is_not_added_twice() {
        let tmp = tempfile::tempdir().unwrap();
        let instance = tmp.path().join("OpenTaiko-1");
        std::fs::create_dir_all(&instance).unwrap();
        let sep = std::path::MAIN_SEPARATOR;
        let ini = format!("[System]\r\nTJAPath=..{sep}Songs{sep}\r\n");
        std::fs::write(instance.join("Config.ini"), ini.as_bytes()).unwrap();

        let songs = tmp.path().join("Songs").to_string_lossy().into_owned();
        let value = write_tjapath(instance.to_string_lossy().into_owned(), songs, false).unwrap();
        assert_eq!(value, format!("..{sep}Songs{sep}"));
    }

    #[test]
    fn a_relative_path_needs_a_shared_root() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("x").join("game");
        let b = tmp.path().join("Songs");
        let rel = relative_path(&a, &b).unwrap();
        assert_eq!(rel, Path::new("..").join("..").join("Songs"));
        assert!(relative_path(&a, Path::new("Songs")).is_none());
        #[cfg(windows)]
        assert!(relative_path(Path::new("C:\\Games\\x"), Path::new("D:\\Songs")).is_none());
    }
}
