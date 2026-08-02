// Edits the game's Config.ini to link an instance to the shared global Songs folder.
// The game reads and writes Config.ini as Shift_JIS (OpenTaiko.sEncType), so all
// I/O here must go through that encoding — writing UTF-8 from JS would corrupt
// non-ASCII paths.
use encoding_rs::SHIFT_JIS;
use std::path::PathBuf;

fn normalize_for_compare(path: &str) -> String {
    let normalized = path.trim().replace('\\', "/");
    let normalized = normalized.trim_end_matches('/').to_string();
    if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    }
}

/// Ensures `[System] TJAPath` in `<instance_dir>/Config.ini` contains the global songs
/// path (appended to whatever entries already exist; the file is created with the
/// default local `Songs` entry plus the global path when missing).
/// With `include_local` false (experimental builds), the default local `Songs` entry is
/// removed instead, so the game only reads the shared library; custom paths are kept.
/// Returns the resulting TJAPath value.
#[tauri::command]
pub fn ensure_config_tjapath(
    instance_dir: String,
    global_songs_path: String,
    include_local: bool,
) -> Result<String, String> {
    let dir = PathBuf::from(&instance_dir);
    if !dir.is_dir() {
        return Err(format!("Instance folder not found: {}", instance_dir));
    }

    let sep = std::path::MAIN_SEPARATOR;
    let global_entry = format!("{}{}", global_songs_path.trim_end_matches(['/', '\\']), sep);

    // Refuse paths the game itself could never read back from a Shift_JIS file
    let (_, _, had_errors) = SHIFT_JIS.encode(&global_entry);
    if had_errors {
        return Err(format!(
            "The songs path contains characters that cannot be stored in Config.ini (Shift_JIS): {}",
            global_songs_path
        ));
    }

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

    let global_norm = normalize_for_compare(&global_entry);
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
            if !entries.iter().any(|entry| normalize_for_compare(entry) == global_norm) {
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

        let value = ensure_config_tjapath(dir.clone(), "D:/Global/Songs".into(), true).unwrap();
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

        let first = ensure_config_tjapath(dir.clone(), "D:/Global/Songs".into(), true).unwrap();
        let second = ensure_config_tjapath(dir.clone(), "D:/Global/Songs".into(), true).unwrap();
        assert_eq!(first, second, "second call must not change the value");
        assert_eq!(first.matches("D:/Global/Songs").count(), 1, "no duplicate entries");

        let bytes = std::fs::read(tmp.path().join("Config.ini")).unwrap();
        let (text, _, _) = SHIFT_JIS.decode(&bytes);
        assert!(text.contains("; 譜面フォルダ"), "comments must survive the rewrite");
        assert!(text.contains("E:\\custom\\"), "existing custom paths must survive");
        assert!(text.contains("FullScreen=0"), "other keys must survive");
    }

    #[test]
    fn rejects_paths_not_representable_in_shift_jis() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap().to_string();
        // é is not representable in Shift_JIS
        assert!(ensure_config_tjapath(dir, "D:/Généreux/Songs".into(), true).is_err());
    }

    #[test]
    fn experimental_mode_drops_local_songs_but_keeps_custom_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap().to_string();
        let sep = std::path::MAIN_SEPARATOR;

        // Fresh file: only the global path
        let value = ensure_config_tjapath(dir.clone(), "D:/Global/Songs".into(), false).unwrap();
        assert_eq!(value, format!("D:/Global/Songs{sep}"));

        // Existing file with the bundled Songs entry and a custom path
        let original = "[System]\r\nTJAPath=Songs\\;E:\\custom\\\r\n";
        let (encoded, _, _) = SHIFT_JIS.encode(original);
        std::fs::write(tmp.path().join("Config.ini"), &encoded).unwrap();

        let value = ensure_config_tjapath(dir.clone(), "D:/Global/Songs".into(), false).unwrap();
        assert!(!value.split(';').any(|e| e == "Songs\\" || e == "Songs/"), "local Songs must be removed");
        assert!(value.contains("E:\\custom\\"), "custom paths must be kept");
        assert!(value.contains("D:/Global/Songs"), "global path must be present");

        // Idempotent
        let again = ensure_config_tjapath(dir, "D:/Global/Songs".into(), false).unwrap();
        assert_eq!(value, again);
    }
}
