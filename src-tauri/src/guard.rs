// Which folders the page may touch.
//
// The webview gets no blanket file access. At startup the Hub's own folders and every
// registered instance are added to the scope the fs plugin checks, and every command that
// writes, deletes or launches something checks its paths against that same scope. A path
// the page cannot reach through the fs plugin therefore cannot be reached through a
// command either. Files and folders picked in a dialog are added by the dialog plugin.

use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_fs::FsExt;

/// The instance list the frontend keeps in the app config folder.
const INSTANCE_REGISTRY: &str = "instances.json";

/// Folders of the Hub root the page works in. The root itself is not exposed: on Windows
/// it is the install folder, which holds the Hub's own executable.
const ROOT_FOLDERS: [&str; 3] = ["Songs", "tmp", "OpenTaiko"];

/// Leftover temporary folders older than this are removed at startup (see `sweep_stale_tmp`).
const STALE_TMP: Duration = Duration::from_secs(6 * 60 * 60);

fn allow_dir<R: Runtime>(app: &AppHandle<R>, dir: &Path) {
    let scope = app.fs_scope();
    if let Err(e) = scope.allow_directory(dir, true) {
        log::warn!("Could not allow {}: {}", dir.display(), e);
    }
    // Checked paths are canonicalized first, which on Windows also normalizes the letter
    // case, so the canonical spelling is allowed as well.
    if let Ok(real) = std::fs::canonicalize(dir) {
        if real != dir {
            let _ = scope.allow_directory(&real, true);
        }
    }
}

/// Spellings of the Hub root: as the frontend builds it, and as the disk has it.
fn root_spellings() -> Vec<PathBuf> {
    let Some(root) = crate::hub_root_dir() else { return Vec::new() };
    let mut out = vec![root.clone()];
    if let Ok(real) = std::fs::canonicalize(&root) {
        if real != root {
            out.push(real);
        }
    }
    out
}

fn registered_instances(registry: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(registry) else { return Vec::new() };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { return Vec::new() };
    json.get("instances")
        .and_then(|list| list.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|inst| inst.get("path")?.as_str())
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .collect()
        })
        .unwrap_or_default()
}

/// Called once from `setup`, before the page can make a request.
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    for root in root_spellings() {
        for folder in ROOT_FOLDERS {
            allow_dir(app, &root.join(folder));
        }
    }
    if let Ok(config) = app.path().app_config_dir() {
        allow_dir(app, &config);
        for instance in registered_instances(&config.join(INSTANCE_REGISTRY)) {
            allow_dir(app, &instance);
        }
    }
}

/// Canonicalizes the deepest part of `path` that exists and appends the rest, so a path
/// about to be created is compared the same way as one that exists.
fn resolve_existing(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut rest = Vec::new();
    while !existing.exists() {
        match (existing.file_name().map(|n| n.to_os_string()), existing.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                existing = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
    let mut out = std::fs::canonicalize(&existing).unwrap_or(existing);
    for name in rest.into_iter().rev() {
        out.push(name);
    }
    out
}

/// Absolute, and no "..": checked lexically before anything touches the disk.
fn is_plain_absolute(path: &Path) -> bool {
    path.is_absolute() && !path.components().any(|c| matches!(c, Component::ParentDir))
}

/// Refuses a path the page could not reach through the fs plugin either.
pub fn checked<R: Runtime>(app: &AppHandle<R>, raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    if !is_plain_absolute(&path) {
        return Err(format!("Refusing an unexpected path: {}", raw));
    }
    if app.fs_scope().is_allowed(resolve_existing(&path)) {
        Ok(path)
    } else {
        Err(format!("{} is outside the folders the Hub manages", path.display()))
    }
}

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts.iter().map(|p| p.len()).eq([8, 4, 4, 4, 12])
        && parts.iter().all(|p| is_hex(p))
}

/// Names the Hub gives its folders in `tmp`: a UUID, its first 8 digits (experimental
/// builds), or `indev-<uuid>` from older versions.
fn is_hub_tmp_name(name: &str) -> bool {
    (name.len() == 8 && is_hex(name)) || is_uuid(name) || name.strip_prefix("indev-").is_some_and(is_uuid)
}

/// Experimental builds fall back to `<system temp>/otk-<8 hex>` when the Hub folder is
/// too deep for the Windows path limit.
fn is_build_dir_name(name: &str) -> bool {
    name.strip_prefix("otk-").is_some_and(|id| id.len() == 8 && is_hex(id))
}

/// Allows one experimental build folder in the system temp folder. Only a direct child
/// of the temp folder named like the ones the Hub creates is accepted.
#[tauri::command]
pub fn allow_build_dir<R: Runtime>(app: AppHandle<R>, path: String) -> Result<(), String> {
    let path = PathBuf::from(&path);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let parent_ok = match (path.parent().map(std::fs::canonicalize), std::fs::canonicalize(std::env::temp_dir())) {
        (Some(Ok(parent)), Ok(temp)) => parent == temp,
        _ => false,
    };
    if !is_plain_absolute(&path) || !is_build_dir_name(name) || !parent_ok {
        return Err(format!("Refusing an unexpected build folder: {}", path.display()));
    }
    allow_dir(&app, &path);
    Ok(())
}

/// Creates the folder of a new instance in the Hub root and allows it. The name is
/// generated by the frontend and must look like one (`OpenTaiko-<uuid>`).
#[tauri::command]
pub fn create_instance_dir<R: Runtime>(app: AppHandle<R>, folder_name: String) -> Result<String, String> {
    if !folder_name.strip_prefix("OpenTaiko-").is_some_and(is_uuid) {
        return Err(format!("Refusing an unexpected instance name: {}", folder_name));
    }
    let root = crate::hub_root_dir().ok_or("The Hub folder could not be resolved")?;
    let dir = root.join(&folder_name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create {}: {}", dir.display(), e))?;
    allow_dir(&app, &dir);
    Ok(dir.to_string_lossy().into_owned())
}

fn is_stale(path: &Path, now: SystemTime) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(|age| age >= STALE_TMP)
}

fn remove_stale_in(dir: &Path, matches: fn(&str) -> bool, now: SystemTime) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let path = entry.path();
        if !name.to_str().is_some_and(matches) || !path.is_dir() || !is_stale(&path, now) {
            continue;
        }
        match std::fs::remove_dir_all(&path) {
            Ok(()) => removed += 1,
            Err(e) => log::warn!("Could not remove leftover temporary folder {}: {}", path.display(), e),
        }
    }
    removed
}

/// A download or build stopped by a crash, a forced quit or a power cut never reaches the
/// cleanup that removes its folder, and one such folder can hold a whole game build.
/// Anything older than the longest operation the Hub runs is one of those. The age check
/// leaves alone a folder a second Hub window may still be writing to, and the name check
/// matters on Linux and macOS, where `tmp` sits in ~/Documents and may be the user's own.
pub fn sweep_stale_tmp() {
    let now = SystemTime::now();
    let mut removed = 0;
    if let Some(root) = crate::hub_root_dir() {
        removed += remove_stale_in(&root.join("tmp"), is_hub_tmp_name, now);
    }
    removed += remove_stale_in(&std::env::temp_dir(), is_build_dir_name, now);
    if removed > 0 {
        log::info!("Removed {} leftover temporary folder(s)", removed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_folders_named_by_the_hub_count_as_temporary() {
        assert!(is_hub_tmp_name("1a2b3c4d"));
        assert!(is_hub_tmp_name("0f8fad5b-d9cb-469f-a165-70867728950e"));
        assert!(is_hub_tmp_name("indev-0f8fad5b-d9cb-469f-a165-70867728950e"));
        assert!(!is_hub_tmp_name("my stuff"));
        assert!(!is_hub_tmp_name("1a2b3c4"));
        assert!(!is_hub_tmp_name("1a2b3c4z"));
        assert!(!is_hub_tmp_name("0f8fad5b-d9cb-469f-a165-70867728950"));
        assert!(!is_hub_tmp_name("indev-"));

        assert!(is_build_dir_name("otk-1a2b3c4d"));
        assert!(!is_build_dir_name("otk-1a2b3c4d5"));
        assert!(!is_build_dir_name("1a2b3c4d"));
    }

    #[test]
    fn a_path_with_a_parent_step_is_never_plain() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(is_plain_absolute(&tmp.path().join("Songs")));
        assert!(!is_plain_absolute(&tmp.path().join("Songs").join("..").join("x")));
        assert!(!is_plain_absolute(Path::new("Songs")));
    }

    #[test]
    fn a_missing_path_resolves_through_its_existing_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let real = std::fs::canonicalize(tmp.path()).unwrap();
        let resolved = resolve_existing(&tmp.path().join("a").join("b.txt"));
        assert_eq!(resolved, real.join("a").join("b.txt"));
    }

    #[test]
    fn only_old_folders_with_hub_names_are_swept() {
        let tmp = tempfile::tempdir().unwrap();
        for name in ["1a2b3c4d", "mine"] {
            std::fs::create_dir(tmp.path().join(name)).unwrap();
        }
        // Nothing is old enough yet
        assert_eq!(remove_stale_in(tmp.path(), is_hub_tmp_name, SystemTime::now()), 0);
        // Seen from seven hours later, only the Hub-named folder goes
        let later = SystemTime::now() + Duration::from_secs(7 * 60 * 60);
        assert_eq!(remove_stale_in(tmp.path(), is_hub_tmp_name, later), 1);
        assert!(!tmp.path().join("1a2b3c4d").exists());
        assert!(tmp.path().join("mine").exists());
    }

    #[test]
    fn instance_paths_are_read_from_the_registry() {
        let tmp = tempfile::tempdir().unwrap();
        let registry = tmp.path().join(INSTANCE_REGISTRY);
        let abs = tmp.path().join("Game");
        let json = serde_json::json!({
            "version": 1,
            "instances": [ { "path": abs }, { "path": "relative/one" }, { "name": "no path" } ]
        });
        std::fs::write(&registry, json.to_string()).unwrap();
        assert_eq!(registered_instances(&registry), vec![abs]);
        assert!(registered_instances(&tmp.path().join("absent.json")).is_empty());
    }
}
