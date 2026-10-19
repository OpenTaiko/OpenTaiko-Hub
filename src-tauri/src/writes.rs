// Files the page has written. The page has no permission to write files itself: whatever
// it could put in an instance or a build folder, the game or the compiler could end up
// running. Each command here decides where it writes, and what.

use std::fs;
use std::path::{Path, PathBuf};

use percent_encoding::percent_decode_str;
use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::DialogExt;

use crate::library::blocking;

/// The Hub's own files in the app config folder.
const SETTINGS_FILES: [&str; 4] = [INSTANCE_LIST, "theme.json", "thememode.json", "language.json"];

const SAVES_DB: &str = "Saves.db3";
/// Copies of Saves.db3 taken before each change, in the instance folder.
const BACKUP_DIR: &str = "SavesBackup";
const KEEP_BACKUPS: usize = 20;

/// Where `write_atomic` writes before renaming.
pub(crate) fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// Writes next to `path` and renames over it, so a crash or a full disk mid-write leaves
/// the previous content intact instead of a truncated file.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create {}: {}", parent.display(), e))?;
    }
    let tmp = temp_path(path);
    let written = fs::write(&tmp, bytes).and_then(|()| fs::rename(&tmp, path));
    written.map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Could not write {}: {}", path.display(), e)
    })
}

/// Writes one of the Hub's settings files (the instance list, theme, mode, language).
#[tauri::command]
pub async fn write_settings_file<R: Runtime>(app: AppHandle<R>, name: String, contents: String) -> Result<(), String> {
    if !SETTINGS_FILES.contains(&name.as_str()) {
        return Err(format!("Refusing to write {}", name));
    }
    serde_json::from_str::<serde_json::Value>(&contents).map_err(|e| format!("{} is not valid JSON: {}", name, e))?;
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    blocking(move || {
        let path = dir.join(&name);
        if name == INSTANCE_LIST {
            keep_backup(&path);
        }
        write_atomic(&path, contents.as_bytes())
    })
    .await
}

const INSTANCE_LIST: &str = "instances.json";

/// Where the previous instance list is kept.
fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

/// Keeps the current instance list beside it before it is replaced, for the page to fall
/// back on when the list cannot be read: without one, an unreadable list was taken for no
/// list at all and overwritten, and every instance had to be added again. A list that is
/// not valid JSON is not kept, so it can never replace a good copy.
fn keep_backup(path: &Path) {
    let Ok(text) = fs::read_to_string(path) else { return };
    if serde_json::from_str::<serde_json::Value>(&text).is_ok() {
        if let Err(e) = write_atomic(&backup_path(path), text.as_bytes()) {
            log::warn!("Could not keep a copy of {}: {}", path.display(), e);
        }
    }
}

/// version.json in an instance folder, which Hub versions before 0.2 read.
#[tauri::command]
pub async fn write_version_json<R: Runtime>(
    app: AppHandle<R>,
    instance_dir: String,
    version: Option<String>,
    build: String,
) -> Result<(), String> {
    let dir = crate::guard::instance_dir(&app, &instance_dir)?;
    let text = serde_json::json!({ "version": version, "build": build }).to_string();
    blocking(move || write_atomic(&dir.join("version.json"), text.as_bytes())).await
}

/// `Saves-YYYYMMDD-HHMMSS-mmm.db3`, the name the page gives a backup.
fn is_backup_name(name: &str) -> bool {
    let Some(stamp) = name.strip_prefix("Saves-").and_then(|n| n.strip_suffix(".db3")) else { return false };
    let parts: Vec<&str> = stamp.split('-').collect();
    parts.len() == 3
        && parts.iter().map(|p| p.len()).eq([8, 6, 3])
        && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()))
}

/// Copies the current database aside, keeps the newest KEEP_BACKUPS copies, then replaces
/// the database. If the copy cannot be made, nothing is written.
fn replace_saves(instance: &Path, backup_name: &str, bytes: &[u8]) -> Result<(), String> {
    let db = instance.join(SAVES_DB);
    let backups = instance.join(BACKUP_DIR);
    fs::create_dir_all(&backups).map_err(|e| format!("Could not create {}: {}", backups.display(), e))?;
    fs::copy(&db, backups.join(backup_name)).map_err(|e| format!("Could not back up {}: {}", db.display(), e))?;

    // Names sort by date, so the oldest come first
    let mut names: Vec<String> = fs::read_dir(&backups)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| is_backup_name(n))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    for name in names.iter().take(names.len().saturating_sub(KEEP_BACKUPS)) {
        if let Err(e) = fs::remove_file(backups.join(name)) {
            log::warn!("Could not remove old save backup {}: {}", name, e);
        }
    }

    write_atomic(&db, bytes)
}

/// Replaces an instance's Saves.db3. The bytes are the request body, and the instance
/// folder and backup name come as headers (`instance` percent-encoded).
#[tauri::command]
pub async fn write_saves<R: Runtime>(app: AppHandle<R>, request: Request<'_>) -> Result<(), String> {
    let header = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|v| percent_decode_str(v).decode_utf8_lossy().into_owned())
    };
    let instance = header("instance").ok_or("No instance given")?;
    let backup = header("backup").filter(|n| is_backup_name(n)).ok_or("No valid backup name given")?;
    // Raw over the IPC protocol; a list of numbers where the webview falls back to messages
    let bytes = match request.body() {
        InvokeBody::Raw(bytes) => bytes.clone(),
        InvokeBody::Json(value) => serde_json::from_value::<Vec<u8>>(value.clone())
            .map_err(|_| "The save database must be sent as bytes".to_string())?,
    };
    let dir = crate::guard::instance_dir(&app, &instance)?;
    // The game would write its own copy back on exit, undoing this one
    if crate::game_is_running(&app, &dir) {
        return Err("OpenTaiko is running in this instance; close it before changing its saves".into());
    }
    blocking(move || replace_saves(&dir, &backup, &bytes)).await
}

/// Asks where to save an exported save, in a dialog the Hub shows itself, and writes it
/// there. Returns the chosen path, or `None` when the dialog was closed without one.
#[tauri::command]
pub async fn export_save<R: Runtime>(
    app: AppHandle<R>,
    window: tauri::Window<R>,
    file_name: String,
    filter_name: String,
    contents: String,
) -> Result<Option<String>, String> {
    let picked = blocking(move || {
        Ok(app
            .dialog()
            .file()
            .set_file_name(file_name)
            .add_filter(filter_name, &["optksave"])
            .set_parent(&window)
            .blocking_save_file())
    })
    .await?;
    let Some(picked) = picked else { return Ok(None) };
    let path = picked.into_path().map_err(|e| format!("Unusable file: {}", e))?;
    let shown = path.to_string_lossy().into_owned();
    blocking(move || fs::write(&path, contents).map_err(|e| format!("Could not write {}: {}", path.display(), e))).await?;
    Ok(Some(shown))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_atomic_write_replaces_and_leaves_no_temporary_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("deep").join("a.json");
        write_atomic(&file, b"one").unwrap();
        write_atomic(&file, b"two").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"two");
        assert!(!temp_path(&file).exists());
    }

    #[test]
    fn only_a_readable_instance_list_is_kept_as_the_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let list = tmp.path().join(INSTANCE_LIST);
        fs::write(&list, r#"{"instances":[]}"#).unwrap();
        keep_backup(&list);
        assert_eq!(fs::read_to_string(backup_path(&list)).unwrap(), r#"{"instances":[]}"#);
        // A broken list leaves the good copy alone
        fs::write(&list, r#"{"instances":["#).unwrap();
        keep_backup(&list);
        assert_eq!(fs::read_to_string(backup_path(&list)).unwrap(), r#"{"instances":[]}"#);
    }

    #[test]
    fn only_backup_names_the_hub_gives_are_accepted() {
        assert!(is_backup_name("Saves-20261019-142501-007.db3"));
        assert!(!is_backup_name("Saves-20261019-142501.db3"));
        assert!(!is_backup_name("Saves-2026101x-142501-007.db3"));
        assert!(!is_backup_name("../Saves-20261019-142501-007.db3"));
        assert!(!is_backup_name("Saves.db3"));
    }

    #[test]
    fn saves_are_backed_up_and_old_backups_pruned() {
        let tmp = tempfile::tempdir().unwrap();
        let backups = tmp.path().join(BACKUP_DIR);
        fs::create_dir_all(&backups).unwrap();
        for i in 0..KEEP_BACKUPS {
            fs::write(backups.join(format!("Saves-20250101-000000-{:03}.db3", i)), b"old").unwrap();
        }
        fs::write(backups.join("mine.txt"), b"kept").unwrap();
        fs::write(tmp.path().join(SAVES_DB), b"current").unwrap();

        replace_saves(tmp.path(), "Saves-20261019-142501-007.db3", b"new").unwrap();

        assert_eq!(fs::read(tmp.path().join(SAVES_DB)).unwrap(), b"new");
        assert_eq!(fs::read(backups.join("Saves-20261019-142501-007.db3")).unwrap(), b"current");
        // The oldest went, the newest stayed, and other files are left alone
        assert!(!backups.join("Saves-20250101-000000-000.db3").exists());
        assert!(backups.join("Saves-20250101-000000-001.db3").exists());
        assert!(backups.join("mine.txt").exists());
    }

    #[test]
    fn nothing_is_written_when_no_backup_can_be_made() {
        let tmp = tempfile::tempdir().unwrap();
        // No Saves.db3 to copy
        assert!(replace_saves(tmp.path(), "Saves-20261019-142501-007.db3", b"new").is_err());
        assert!(!tmp.path().join(SAVES_DB).exists());
    }
}
