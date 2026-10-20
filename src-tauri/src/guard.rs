// Which folders the page may touch.
//
// The webview gets no blanket file access. At startup the Hub's own folders and its
// instances are added to the scope the fs plugin checks, and every command that
// writes, deletes or launches something checks its paths against that same scope. A path
// the page cannot reach through the fs plugin therefore cannot be reached through a
// command either. Files the user picks are added by `pick_files`, which shows the dialog.
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
    // case, so the canonical spelling is allowed as well. Worked out through the deepest
    // folder that exists: on a first start the Hub's folders do not exist yet, and under
    // a Documents folder that is a symbolic link, only the spelling through the link was
    // allowed, so every download into a folder created later was refused.
    let real = resolve_existing(dir);
    if real != dir {
        let _ = scope.allow_directory(&real, true);
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

/// The instance folders a list names, or `None` when it is missing or cannot be read.
fn read_instances(registry: &Path) -> Option<Vec<PathBuf>> {
    let text = std::fs::read_to_string(registry).ok()?;
    let json = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    Some(
        json.get("instances")
            .and_then(|list| list.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|inst| inst.get("path")?.as_str())
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute())
                    .collect()
            })
            .unwrap_or_default(),
    )
}

fn registered_instances(registry: &Path) -> Vec<PathBuf> {
    read_instances(registry).unwrap_or_default()
}

/// The page's instance list, or the copy writes.rs keeps of it when it cannot be read: the
/// page falls back on that copy too.
fn page_instances(config: &Path) -> Option<Vec<PathBuf>> {
    read_instances(&config.join(INSTANCE_REGISTRY))
        .or_else(|| read_instances(&config.join(format!("{INSTANCE_REGISTRY}.bak"))))
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

/// The instance folders the Hub may use, of those `registered` (the page's list) names.
///
/// The first time, before the Hub kept a list of its own, the folders the page's list
/// names are taken over, except one holding the home folder, the Hub's folder or its
/// settings (`protected`): a list the page had rewritten could otherwise grant it the whole
/// disk. A page list that cannot be read takes over nothing and is not recorded, so a
/// later start can still take it over; recording it as empty cut off every instance.
fn load_roots(file: &Path, registered: Option<&[PathBuf]>, protected: &[PathBuf]) -> Vec<PathBuf> {
    if !file.exists() {
        let Some(registered) = registered else { return Vec::new() };
        let roots: Vec<PathBuf> = registered.iter().filter(|dir| !holds_any(dir, protected)).cloned().collect();
        if let Err(e) = write_roots(file, &roots) {
            log::warn!("Could not record the instance folders: {}", e);
        }
        return roots;
    }
    let Some(saved) = read_instances(file) else {
        log::warn!("Could not read {}; no instance folder is allowed this session", file.display());
        return Vec::new();
    };
    // Detaching an instance only edits the page's list, so a folder it no longer names is
    // not allowed this session. Nothing is removed from the file: the page's list cannot
    // be relied on, for instance after a crash while it was being written.
    match registered {
        Some(registered) => saved.into_iter().filter(|root| registered.iter().any(|r| same_path(r, root))).collect(),
        None => saved,
    }
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
            // On macOS the app local data folder is the config folder the page may use. The
            // folder itself is protected too: removing it, or moving it away, would take the
            // file with it, and the forbidden list is only checked against the path named.
            let scope = app.fs_scope();
            let mut protect = vec![file.clone()];
            protect.extend(file.parent().map(Path::to_path_buf));
            for path in protect {
                let real = resolve_existing(&path);
                for spelling in [path, real] {
                    if let Err(e) = scope.forbid_file(&spelling) {
                        log::warn!("Could not protect {}: {}", spelling.display(), e);
                    }
                }
            }
            roots = load_roots(&file, page_instances(&config).as_deref(), &protected_dirs(app));
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
    let state = app.state::<InstanceRoots>();
    // Held from reading the file to writing it: an attach and a create at the same time
    // each wrote the file as they had read it, and one of the two folders was lost
    let mut roots = state.0.lock().unwrap_or_else(|e| e.into_inner());
    let mut saved = registered_instances(&file);
    if !saved.iter().any(|p| same_path(p, dir)) {
        saved.push(dir.to_path_buf());
        write_roots(&file, &saved)?;
    }
    allow_dir(app, dir);
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
    // Compared outside the lock: comparing reads the disk, which an instance on a network
    // drive that went away can hold up for a long time
    let attached = instance_roots(app).iter().any(|root| same_path(root, &dir));
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

/// A temporary folder the Hub named: one in its own tmp folder, or a build folder in the
/// system temp folder (see `allow_build_dir`).
pub fn is_hub_tmp_dir(dir: &Path) -> bool {
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let Some(container) = dir.parent() else { return false };
    (is_hub_tmp_name(name) && root_spellings().iter().any(|root| same_path(&root.join("tmp"), container)))
        || (is_build_dir_name(name) && same_path(&std::env::temp_dir(), container))
}

/// Spelled the way the disk has it, compared component by component, and without regard
/// to case on Windows.
pub(crate) fn path_key(path: &Path) -> Vec<String> {
    resolve_existing(path)
        .components()
        .map(|c| {
            let part = c.as_os_str().to_string_lossy();
            if cfg!(windows) { part.to_lowercase() } else { part.into_owned() }
        })
        .collect()
}

pub(crate) fn same_path(a: &Path, b: &Path) -> bool {
    path_key(a) == path_key(b)
}

/// True when `dir` is `other` or one of the folders holding it.
fn holds(dir: &Path, other: &Path) -> bool {
    path_key(other).starts_with(&path_key(dir))
}

/// Folders an instance folder may not hold: the user's home folder, the Hub's own folder
/// and its settings. An instance folder is opened to the page all the way down.
fn protected_dirs<R: Runtime>(app: &AppHandle<R>) -> Vec<PathBuf> {
    [
        dirs::home_dir(),
        crate::hub_root_dir(),
        app.path().app_config_dir().ok(),
        app.path().app_local_data_dir().ok(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn holds_any(dir: &Path, protected: &[PathBuf]) -> bool {
    protected.iter().any(|p| holds(dir, p))
}

fn is_acceptable_instance_dir<R: Runtime>(app: &AppHandle<R>, dir: &Path) -> bool {
    dir.is_absolute() && dir.is_dir() && !holds_any(dir, &protected_dirs(app))
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

/// The files the page asks the user for, by kind: chart jackets, chart archives and
/// exported saves.
fn pick_extensions(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "image" => Some(&["png", "jpg", "jpeg", "webp"]),
        "chart_archive" => Some(&["zip"]),
        "save" => Some(&["optksave", "json"]),
        _ => None,
    }
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// Asks the user for files of one kind, in a dialog the Hub shows itself, and allows them.
/// The page only says which kind: with the dialog plugin's own commands it could open a
/// folder dialog on the home folder, and one click put that whole folder within its reach.
#[tauri::command]
pub async fn pick_files<R: Runtime>(
    app: AppHandle<R>,
    window: tauri::Window<R>,
    kind: String,
    label: String,
    multiple: bool,
) -> Result<Vec<String>, String> {
    let extensions = pick_extensions(&kind).ok_or_else(|| format!("Unknown kind of file: {}", kind))?;
    let dialog_app = app.clone();
    let picked = crate::library::blocking(move || {
        let dialog = dialog_app.dialog().file().add_filter(label, extensions).set_parent(&window);
        Ok(if multiple {
            dialog.blocking_pick_files().unwrap_or_default()
        } else {
            dialog.blocking_pick_file().into_iter().collect()
        })
    })
    .await?;
    let mut paths = Vec::with_capacity(picked.len());
    for file in picked {
        let path = file.into_path().map_err(|e| format!("Unusable file: {}", e))?;
        // A name typed into the dialog is not held to its filter
        if !has_extension(&path, extensions) || !path.is_file() {
            return Err(format!("{} cannot be used here", path.display()));
        }
        let scope = app.fs_scope();
        for spelling in [path.clone(), resolve_existing(&path)] {
            scope.allow_file(&spelling).map_err(|e| format!("Could not allow {}: {}", spelling.display(), e))?;
        }
        paths.push(path.to_string_lossy().into_owned());
    }
    Ok(paths)
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

pub(crate) fn is_uuid(s: &str) -> bool {
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
pub async fn create_instance_dir<R: Runtime>(app: AppHandle<R>, folder_name: String) -> Result<String, String> {
    if !folder_name.strip_prefix("OpenTaiko-").is_some_and(is_uuid) {
        return Err(format!("Refusing an unexpected instance name: {}", folder_name));
    }
    let root = crate::hub_root_dir().ok_or("The Hub folder could not be resolved")?;
    let dir = root.join(&folder_name);
    // Off the main thread, which also drives the window
    crate::library::blocking(move || {
        std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create {}: {}", dir.display(), e))?;
        remember_root(&app, &dir)?;
        Ok(dir.to_string_lossy().into_owned())
    })
    .await
}

fn is_stale(path: &Path, now: SystemTime) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(|age| age >= STALE_TMP)
}

fn has_hub_tmp_name(dir: &Path) -> bool {
    dir.file_name().and_then(|n| n.to_str()).is_some_and(is_hub_tmp_name)
}

fn has_build_dir_name(dir: &Path) -> bool {
    dir.file_name().and_then(|n| n.to_str()).is_some_and(is_build_dir_name)
}

/// What Hub 0.2 left in ~/Documents/tmp on Linux and macOS. That folder may be the user's
/// own, and a name alone also matched folders such as `20240101`, so a folder there must
/// also hold nothing but what the Hub put in it: archives, an extracted release
/// (`OpenTaiko…`) and the folders of a build (`s`, `p`, `b`). Names are in lower case, as
/// the Hub wrote them.
fn is_old_hub_tmp(dir: &Path) -> bool {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else { return false };
    if !is_hub_tmp_name(name) || name != name.to_ascii_lowercase() {
        return false;
    }
    if name.starts_with("indev-") {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    entries.flatten().all(|entry| {
        let entry_name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            matches!(entry_name.as_str(), "s" | "p" | "b") || entry_name.starts_with("OpenTaiko")
        } else {
            entry_name.to_ascii_lowercase().ends_with(".zip")
        }
    })
}

fn remove_stale_in(dir: &Path, matches: impl Fn(&Path) -> bool, now: SystemTime) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || !is_stale(&path, now) || !matches(&path) {
            continue;
        }
        match std::fs::remove_dir_all(&path) {
            Ok(()) => removed += 1,
            Err(e) => log::warn!("Could not remove leftover temporary folder {}: {}", path.display(), e),
        }
    }
    removed
}

/// What a download left beside the files it was replacing (download.rs), when a crash,
/// a forced quit or a power cut stopped it: partial files, removed, and old files set
/// aside, put back when their replacement never arrived. Symbolic links are not followed.
fn clean_stale_downloads(dir: &Path, now: SystemTime) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let mut cleaned = 0;
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        let path = entry.path();
        if kind.is_dir() {
            cleaned += clean_stale_downloads(&path, now);
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let stale = || kind.is_file() && is_stale(&path, now);
        let done = if name.ends_with(crate::download::PART_SUFFIX) && stale() {
            std::fs::remove_file(&path).is_ok()
        } else if let Some(original) = name.strip_suffix(crate::download::OLD_SUFFIX).filter(|_| stale()) {
            let dest = path.with_file_name(original);
            let outcome = if dest.exists() { std::fs::remove_file(&path) } else { std::fs::rename(&path, &dest) };
            outcome.is_ok()
        } else {
            false
        };
        cleaned += usize::from(done);
    }
    cleaned
}

/// A download or build stopped by a crash, a forced quit or a power cut never reaches the
/// cleanup that removes its folder, and one such folder can hold a whole game build.
/// Anything older than the longest operation the Hub runs is one of those. The age check
/// leaves alone a folder a second Hub window may still be writing to, and the name check
/// matters on Linux and macOS, where `tmp` sits in ~/Documents and may be the user's own.
/// Downloads go straight into the library and the instances (`instances`), where what
/// such a download leaves is cleaned up as well. `old_tmp` is where Linux and macOS kept
/// tmp before the Hub had a folder of its own, which nothing would empty any more.
///
/// Downloads leave something behind only when the Hub did not end normally (`deep`): the
/// library and every instance were otherwise walked file by file at every start, over
/// twenty thousand entries on a modest install, to find nothing.
pub fn sweep_leftovers(instances: Vec<PathBuf>, old_tmp: Option<PathBuf>, deep: bool) {
    let now = SystemTime::now();
    let mut removed = 0;
    let mut cleaned = 0;
    let root = crate::hub_root_dir();
    if let Some(root) = &root {
        removed += remove_stale_in(&root.join("tmp"), has_hub_tmp_name, now);
    }
    if let Some(old_tmp) = old_tmp {
        removed += remove_stale_in(&old_tmp, is_old_hub_tmp, now);
    }
    removed += remove_stale_in(&std::env::temp_dir(), has_build_dir_name, now);
    if deep {
        let mut trees: Vec<PathBuf> = root.iter().flat_map(|r| [r.join("Songs"), r.join("OpenTaiko")]).collect();
        // Hub 0.1's game folder is usually registered as an instance as well
        for instance in instances {
            if !trees.iter().any(|tree| same_path(tree, &instance)) {
                trees.push(instance);
            }
        }
        for tree in &trees {
            cleaned += clean_stale_downloads(tree, now);
        }
    }
    if removed > 0 {
        log::info!("Removed {} leftover temporary folder(s)", removed);
    }
    if cleaned > 0 {
        log::info!("Cleaned up {} file(s) left by interrupted downloads", cleaned);
    }
}

/// Says, in the app local data folder, whether the Hub is running: "running" from start to
/// exit, "stopped" after a normal exit.
const SESSION_FILE: &str = "session";

fn session_file<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_local_data_dir().ok().map(|dir| dir.join(SESSION_FILE))
}

/// Records that the Hub is running. True when the previous run did not end normally (a
/// crash, a forced quit, a power cut), or when no run of this version recorded anything.
pub fn begin_session<R: Runtime>(app: &AppHandle<R>) -> bool {
    session_file(app).map_or(true, |file| begin_session_in(&file))
}

fn begin_session_in(file: &Path) -> bool {
    let ended_normally = std::fs::read_to_string(file).is_ok_and(|state| state == "stopped");
    if let Err(e) = crate::writes::write_atomic(file, b"running") {
        log::warn!("Could not record the session: {}", e);
    }
    !ended_normally
}

/// Records a normal exit.
pub fn end_session<R: Runtime>(app: &AppHandle<R>) {
    if let Some(file) = session_file(app) {
        if let Err(e) = crate::writes::write_atomic(&file, b"stopped") {
            log::warn!("Could not record the end of the session: {}", e);
        }
    }
}

/// The instance folders allowed this session.
pub fn instance_roots<R: Runtime>(app: &AppHandle<R>) -> Vec<PathBuf> {
    app.state::<InstanceRoots>().0.lock().unwrap_or_else(|e| e.into_inner()).clone()
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
        assert_eq!(remove_stale_in(tmp.path(), has_hub_tmp_name, SystemTime::now()), 0);
        // Seen from seven hours later, only the Hub-named folder goes
        let later = SystemTime::now() + Duration::from_secs(7 * 60 * 60);
        assert_eq!(remove_stale_in(tmp.path(), has_hub_tmp_name, later), 1);
        assert!(!tmp.path().join("1a2b3c4d").exists());
        assert!(tmp.path().join("mine").exists());
    }

    // ~/Documents/tmp may be the user's own folder, where a name such as 20240101 is all
    // hexadecimal digits too
    #[test]
    fn the_old_documents_tmp_loses_only_what_the_hub_left() {
        let tmp = tempfile::tempdir().unwrap();
        let hub_build = tmp.path().join("1a2b3c4d");
        std::fs::create_dir_all(hub_build.join("s").join("OpenTaiko")).unwrap();
        std::fs::write(hub_build.join("src.zip"), "PK").unwrap();
        let hub_download = tmp.path().join("0f8fad5b-d9cb-469f-a165-70867728950e");
        std::fs::create_dir_all(hub_download.join("OpenTaiko.Linux.x64")).unwrap();
        std::fs::create_dir_all(tmp.path().join("indev-0f8fad5b-d9cb-469f-a165-70867728950e").join("anything")).unwrap();
        let dated = tmp.path().join("20240101");
        std::fs::create_dir_all(&dated).unwrap();
        std::fs::write(dated.join("photo.jpg"), "mine").unwrap();
        let upper = tmp.path().join("CAFEBABE");
        std::fs::create_dir_all(upper.join("s")).unwrap();

        let later = SystemTime::now() + Duration::from_secs(7 * 60 * 60);
        assert_eq!(remove_stale_in(tmp.path(), is_old_hub_tmp, later), 3);
        assert!(dated.join("photo.jpg").exists());
        assert!(upper.exists());
        assert!(!hub_build.exists() && !hub_download.exists());
    }

    #[test]
    fn interrupted_downloads_are_cleaned_up_once_old() {
        let tmp = tempfile::tempdir().unwrap();
        let song = tmp.path().join("Pop").join("Song");
        std::fs::create_dir_all(&song).unwrap();
        // A partial download, a replaced file whose replacement arrived, and one whose
        // replacement never did
        std::fs::write(song.join("song.ogg.hubpart"), "half").unwrap();
        std::fs::write(song.join("song.tja"), "new").unwrap();
        std::fs::write(song.join("song.tja.hubold"), "old").unwrap();
        std::fs::write(song.join("jacket.png.hubold"), "original").unwrap();
        std::fs::write(song.join("notes.part"), "the user's").unwrap();

        assert_eq!(clean_stale_downloads(tmp.path(), SystemTime::now()), 0);
        let later = SystemTime::now() + Duration::from_secs(7 * 60 * 60);
        assert_eq!(clean_stale_downloads(tmp.path(), later), 3);

        let mut names: Vec<String> = std::fs::read_dir(&song)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["jacket.png", "notes.part", "song.tja"]);
        assert_eq!(std::fs::read_to_string(song.join("song.tja")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(song.join("jacket.png")).unwrap(), "original");
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

        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let protected = [home.clone()];

        // The first start takes over the page's list, except a folder holding a protected
        // one, and records it
        let page = [a.clone(), b.clone(), tmp.path().to_path_buf(), home.clone()];
        assert_eq!(load_roots(&file, Some(&page), &protected), vec![a.clone(), b.clone()]);
        assert_eq!(registered_instances(&file), vec![a.clone(), b.clone()]);

        // Later, a folder only the page's list names is not allowed, and one the page's
        // list dropped (a detached instance) is not allowed this session but kept on file
        let page = [a.clone(), c.clone(), tmp.path().to_path_buf()];
        assert_eq!(load_roots(&file, Some(&page), &protected), vec![a.clone()]);
        assert_eq!(registered_instances(&file), vec![a.clone(), b.clone()]);

        // A page list that cannot be read hides nothing the Hub recorded
        assert_eq!(load_roots(&file, None, &protected), vec![a, b]);
    }

    #[test]
    fn only_a_run_that_did_not_end_normally_asks_for_a_deep_sweep() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("data").join(SESSION_FILE);
        // The first run of a version that records sessions: nothing says how the last ended
        assert!(begin_session_in(&file));
        // Still "running": that run never ended normally
        assert!(begin_session_in(&file));
        crate::writes::write_atomic(&file, b"stopped").unwrap();
        assert!(!begin_session_in(&file));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "running");
    }

    #[test]
    fn an_unreadable_list_records_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("data").join(INSTANCE_ROOTS);
        // No page list to take over: nothing is recorded, so a later start still can
        assert!(load_roots(&file, None, &[]).is_empty());
        assert!(!file.exists());

        // A damaged record allows nothing and is left for the user to look at
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "{ not json").unwrap();
        assert!(load_roots(&file, Some(&[tmp.path().join("A")]), &[]).is_empty());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ not json");
    }

    #[test]
    fn the_page_list_falls_back_on_its_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("Game");
        let list = serde_json::json!({ "instances": [ { "path": game } ] }).to_string();
        std::fs::write(tmp.path().join(format!("{INSTANCE_REGISTRY}.bak")), &list).unwrap();
        assert!(page_instances(&tmp.path().join("absent")).is_none());

        std::fs::write(tmp.path().join(INSTANCE_REGISTRY), "{ cut short").unwrap();
        assert_eq!(page_instances(tmp.path()), Some(vec![game.clone()]));
        std::fs::write(tmp.path().join(INSTANCE_REGISTRY), list.replace("Game", "Other")).unwrap();
        assert_eq!(page_instances(tmp.path()), Some(vec![tmp.path().join("Other")]));
    }

    #[test]
    fn only_folders_the_hub_named_hold_archives_to_extract() {
        let temp = std::env::temp_dir();
        assert!(is_hub_tmp_dir(&temp.join("otk-1a2b3c4d")));
        assert!(!is_hub_tmp_dir(&temp.join("1a2b3c4d")));
        assert!(!is_hub_tmp_dir(&temp.join("otk-1a2b3c4d").join("x")));
        if let Some(root) = crate::hub_root_dir() {
            assert!(is_hub_tmp_dir(&root.join("tmp").join("0f8fad5b-d9cb-469f-a165-70867728950e")));
            assert!(is_hub_tmp_dir(&root.join("tmp").join("1a2b3c4d")));
            assert!(!is_hub_tmp_dir(&root.join("tmp").join("mine")));
            assert!(!is_hub_tmp_dir(&root.join("OpenTaiko")));
        }
    }

    #[test]
    fn a_picked_file_must_have_an_offered_extension() {
        let images = pick_extensions("image").unwrap();
        assert!(has_extension(Path::new("/a/jacket.PNG"), images));
        assert!(!has_extension(Path::new("/a/OpenTaiko.exe"), images));
        assert!(!has_extension(Path::new("/a/png"), images));
        assert!(pick_extensions("folder").is_none());
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
