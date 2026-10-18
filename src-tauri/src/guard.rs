// Which folders the page may touch.
//
// The webview gets no blanket file access. At startup the Hub's own folders and its
// instances are added to the scope the fs plugin checks, and every command that
// writes, deletes or launches something checks its paths against that same scope. A path
// the page cannot reach through the fs plugin therefore cannot be reached through a
// command either. Files and folders picked in a dialog are added by the dialog plugin.
//
// Which instance folders are allowed is decided here, not by the instance list the page
// keeps: the page writes that list, so trusting it let page script grant itself any
// folder, the whole drive included, from the next start on.

use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_fs::FsExt;

/// The instance list the frontend keeps in the app config folder.
const INSTANCE_REGISTRY: &str = "instances.json";

/// Instance folders the Hub created, or the user picked in the Hub's own folder dialog,
/// kept in the app local data folder in the same shape as the instance list. The page
/// can neither write nor delete it (see `init`).
pub const INSTANCE_ROOTS: &str = "instance-roots.json";

/// The instance folders allowed this session: those in INSTANCE_ROOTS that the page's
/// list still names. Only commands here add to it.
#[derive(Default)]
pub struct InstanceRoots(Mutex<Vec<PathBuf>>);

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

fn roots_file<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_local_data_dir().ok().map(|dir| dir.join(INSTANCE_ROOTS))
}

fn write_roots(file: &Path, roots: &[PathBuf]) -> Result<(), String> {
    let json = serde_json::json!({
        "version": 1,
        "instances": roots.iter().map(|p| serde_json::json!({ "path": p })).collect::<Vec<_>>()
    });
    crate::writes::write_atomic(file, json.to_string().as_bytes())
}

/// The instance folders the Hub may use, of those `registered` names. The first time,
/// before the Hub kept a list of its own, the page's list is taken as it is.
fn load_roots(file: &Path, registered: &[PathBuf]) -> Vec<PathBuf> {
    if !file.exists() {
        if let Err(e) = write_roots(file, registered) {
            log::warn!("Could not record the instance folders: {}", e);
        }
        return registered.to_vec();
    }
    // Detaching an instance only edits the page's list, so a folder it no longer names is
    // not allowed this session. Nothing is removed from the file: the page's list cannot
    // be relied on, for instance after a crash while it was being written.
    registered_instances(file)
        .into_iter()
        .filter(|root| registered.iter().any(|r| same_path(r, root)))
        .collect()
}

/// Called once from `setup`, before the page can make a request.
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    for root in root_spellings() {
        for folder in ROOT_FOLDERS {
            allow_dir(app, &root.join(folder));
        }
    }
    let mut roots = Vec::new();
    if let Ok(config) = app.path().app_config_dir() {
        allow_dir(app, &config);
        if let Some(file) = roots_file(app) {
            // On macOS the app local data folder is the config folder the page may use
            let scope = app.fs_scope();
            for path in [file.clone(), crate::writes::temp_path(&file)] {
                if let Err(e) = scope.forbid_file(&path) {
                    log::warn!("Could not protect {}: {}", path.display(), e);
                }
            }
            roots = load_roots(&file, &registered_instances(&config.join(INSTANCE_REGISTRY)));
        }
    }
    for root in &roots {
        allow_dir(app, root);
    }
    app.manage(InstanceRoots(Mutex::new(roots)));
}

/// Adds an instance folder to the ones the Hub may use, now and on later starts.
fn remember_root<R: Runtime>(app: &AppHandle<R>, dir: &Path) -> Result<(), String> {
    let file = roots_file(app).ok_or("The Hub's data folder could not be resolved")?;
    let mut saved = registered_instances(&file);
    if !saved.iter().any(|p| same_path(p, dir)) {
        saved.push(dir.to_path_buf());
        write_roots(&file, &saved)?;
    }
    allow_dir(app, dir);
    let state = app.state::<InstanceRoots>();
    let mut roots = state.0.lock().unwrap_or_else(|e| e.into_inner());
    if !roots.iter().any(|p| same_path(p, dir)) {
        roots.push(dir.to_path_buf());
    }
    Ok(())
}

/// The folder of one of the Hub's instances: one it created or the user attached, or the
/// game folder of Hub 0.1 in the Hub root. Anything else, a temporary folder in
/// particular, is refused.
pub fn instance_dir<R: Runtime>(app: &AppHandle<R>, raw: &str) -> Result<PathBuf, String> {
    let dir = checked(app, raw)?;
    let legacy = root_spellings().into_iter().any(|root| same_path(&root.join("OpenTaiko"), &dir));
    let attached = app
        .state::<InstanceRoots>()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .any(|root| same_path(root, &dir));
    if legacy || attached {
        Ok(dir)
    } else {
        Err(format!("{} is not one of the Hub's instances", dir.display()))
    }
}

/// The source folder of an experimental build: `s` inside a build folder the Hub named,
/// in its own tmp folder or in the system temp folder (see `allow_build_dir`), holding
/// the game's project.
pub fn build_source_dir<R: Runtime>(app: &AppHandle<R>, raw: &str) -> Result<PathBuf, String> {
    let src = checked(app, raw)?;
    let build = src.parent().filter(|_| src.file_name().is_some_and(|n| n == "s"));
    let name = build.and_then(|b| b.file_name()).and_then(|n| n.to_str()).unwrap_or("");
    let container = build.and_then(Path::parent);
    let in_hub_tmp = name.len() == 8
        && is_hex(name)
        && container.is_some_and(|c| root_spellings().iter().any(|root| same_path(&root.join("tmp"), c)));
    let in_system_temp = is_build_dir_name(name) && container.is_some_and(|c| same_path(&std::env::temp_dir(), c));
    if !(in_hub_tmp || in_system_temp) || !src.join("OpenTaiko").join("OpenTaiko.csproj").is_file() {
        return Err(format!("Refusing to build in {}", src.display()));
    }
    Ok(src)
}

/// Spelled the way the disk has it, compared component by component, and without regard
/// to case on Windows.
fn path_key(path: &Path) -> Vec<String> {
    resolve_existing(path)
        .components()
        .map(|c| {
            let part = c.as_os_str().to_string_lossy();
            if cfg!(windows) { part.to_lowercase() } else { part.into_owned() }
        })
        .collect()
}

fn same_path(a: &Path, b: &Path) -> bool {
    path_key(a) == path_key(b)
}

/// True when `dir` is `other` or one of the folders holding it.
fn holds(dir: &Path, other: &Path) -> bool {
    path_key(other).starts_with(&path_key(dir))
}

/// A folder picked as an instance is opened to the page all the way down, so it may not be
/// one that holds the user's home folder, the Hub's own folder or its settings.
fn is_acceptable_instance_dir<R: Runtime>(app: &AppHandle<R>, dir: &Path) -> bool {
    let protected = [
        dirs::home_dir(),
        crate::hub_root_dir(),
        app.path().app_config_dir().ok(),
        app.path().app_local_data_dir().ok(),
    ];
    dir.is_absolute() && dir.is_dir() && !protected.iter().flatten().any(|p| holds(dir, p))
}

/// Asks the user for an existing instance folder, in a dialog the Hub shows itself, and
/// allows it. `None` when the dialog was closed without a choice. The page cannot name
/// the folder: it would then be able to open any folder to itself.
#[tauri::command]
pub async fn attach_instance_dir<R: Runtime>(
    app: AppHandle<R>,
    window: tauri::Window<R>,
    title: String,
) -> Result<Option<String>, String> {
    let dialog_app = app.clone();
    let picked = crate::library::blocking(move || {
        Ok(dialog_app.dialog().file().set_title(title).set_parent(&window).blocking_pick_folder())
    })
    .await?;
    let Some(picked) = picked else { return Ok(None) };
    let dir = picked.into_path().map_err(|e| format!("Unusable folder: {}", e))?;
    if !is_acceptable_instance_dir(&app, &dir) {
        return Err(format!("{} cannot be used as an instance folder", dir.display()));
    }
    remember_root(&app, &dir)?;
    Ok(Some(dir.to_string_lossy().into_owned()))
}

/// Canonicalizes the deepest part of `path` that exists and appends the rest, so a path
/// about to be created is compared the same way as one that exists.
pub(crate) fn resolve_existing(path: &Path) -> PathBuf {
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
    remember_root(&app, &dir)?;
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

    #[test]
    fn the_page_list_never_adds_an_instance_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("data").join(INSTANCE_ROOTS);
        let (a, b, c) = (tmp.path().join("A"), tmp.path().join("B"), tmp.path().join("C"));

        // The first start takes the page's list as it is, and records it
        assert_eq!(load_roots(&file, &[a.clone(), b.clone()]), vec![a.clone(), b.clone()]);
        assert_eq!(registered_instances(&file), vec![a.clone(), b.clone()]);

        // Later, a folder only the page's list names is not allowed, and one the page's
        // list dropped (a detached instance) is not allowed this session but kept on file
        assert_eq!(load_roots(&file, &[a.clone(), c.clone(), tmp.path().to_path_buf()]), vec![a.clone()]);
        assert_eq!(registered_instances(&file), vec![a, b]);
    }

    #[test]
    fn a_folder_holding_a_protected_one_is_told_apart() {
        let tmp = tempfile::tempdir().unwrap();
        let inner = tmp.path().join("Hub").join("Songs");
        std::fs::create_dir_all(&inner).unwrap();
        assert!(holds(tmp.path(), &inner));
        assert!(holds(&inner, &inner));
        assert!(!holds(&inner, tmp.path()));
        assert!(!holds(&tmp.path().join("Hu"), &inner));
        assert!(same_path(&inner, &tmp.path().join("Hub").join("Songs").join(".")));
    }
}
