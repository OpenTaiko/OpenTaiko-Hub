// Moving the Hub's data to another root folder, and repointing what refers to it: the
// instance registry, and the song library path in each instance's Config.ini.
//
// Two moves use this. Dev builds moved their data out of target/debug (devdata.rs).
// Linux and macOS used to keep it directly in the Documents folder under generic names
// (Songs, tmp, logs, OpenTaiko), where an existing ~/Documents/Songs was silently taken
// over as the library; they now use a folder of their own (see hub_root_dir).

use std::path::{Path, PathBuf};

use serde::Serialize;

/// What a move of the Hub's data did: every step for the log, and for the page, which
/// tells the user once, what moved and what could not be.
#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    #[serde(skip)]
    pub lines: Vec<String>,
    /// The folder the data moved into.
    pub new_root: String,
    /// Folders moved, as (from, to).
    pub moved: Vec<(String, String)>,
    /// Steps that failed, which the user may have to finish by hand.
    pub problems: Vec<String>,
}

impl Report {
    pub(crate) fn new(new_root: &Path) -> Self {
        Self { new_root: new_root.to_string_lossy().into_owned(), ..Self::default() }
    }

    pub(crate) fn note(&mut self, line: String) {
        self.lines.push(line);
    }

    pub(crate) fn problem(&mut self, line: String) {
        self.lines.push(line.clone());
        self.problems.push(line);
    }

    pub(crate) fn moved(&mut self, from: &Path, to: &Path) {
        self.lines.push(format!("Moved {} to {}", from.display(), to.display()));
        self.moved.push((from.to_string_lossy().into_owned(), to.to_string_lossy().into_owned()));
    }

    /// Nothing to tell the user.
    pub fn is_quiet(&self) -> bool {
        self.moved.is_empty() && self.problems.is_empty()
    }
}

/// Rewrites registry paths through `map` (None keeps a path as it is). Returns how many
/// changed.
pub(crate) fn rewrite_registry(registry: &Path, map: impl Fn(&str) -> Option<String>) -> Result<usize, String> {
    let Ok(text) = std::fs::read_to_string(registry) else { return Ok(0) };
    let mut json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut changed = 0;
    // The Hub's own list also keeps the folders it could not reach yet (guard.rs)
    for key in ["instances", "pending"] {
        let Some(list) = json.get_mut(key).and_then(|l| l.as_array_mut()) else { continue };
        for instance in list {
            let Some(path) = instance.get("path").and_then(|p| p.as_str()) else { continue };
            if let Some(new_path) = map(path) {
                instance["path"] = serde_json::Value::String(new_path);
                changed += 1;
            }
        }
    }
    if changed > 0 {
        let pretty = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
        crate::writes::write_atomic(registry, pretty.as_bytes())?;
    }
    Ok(changed)
}

/// Rewrites the instance list and the Hub's own list of instance folders (see guard.rs)
/// through the same `map`, for the log.
pub(crate) fn rewrite_registries(
    registries: [&Path; 2],
    map: impl Fn(&str) -> Option<String>,
    report: &mut Report,
) {
    for registry in registries {
        match rewrite_registry(registry, &map) {
            Ok(0) => {}
            Ok(n) => report.note(format!("Updated {} instance path(s) in {}", n, registry.display())),
            Err(e) => report.problem(format!("Could not update {}: {}", registry.display(), e)),
        }
    }
}

/// What follows `root` in `path`, separator included, when `path` is inside `root`.
/// Case-insensitive: the frontend may have written the same folder with other casing.
#[cfg_attr(not(any(debug_assertions, test)), allow(dead_code))]
pub(crate) fn strip_root<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    let head = path.get(..root.len())?;
    let rest = &path[root.len()..];
    (head.eq_ignore_ascii_case(root) && (rest.is_empty() || rest.starts_with(['\\', '/']))).then_some(rest)
}

#[cfg_attr(any(debug_assertions, target_os = "windows"), allow(dead_code))]
fn registered_paths(registry: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(registry) else { return Vec::new() };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { return Vec::new() };
    json.get("instances")
        .and_then(|l| l.as_array())
        .map(|list| list.iter().filter_map(|i| i.get("path")?.as_str()).map(PathBuf::from).collect())
        .unwrap_or_default()
}

/// True when a .tja sits within a few levels: an OpenTaiko library, not some other
/// folder that happens to be called Songs.
#[cfg_attr(any(debug_assertions, target_os = "windows"), allow(dead_code))]
fn holds_charts(dir: &Path, depth: u32) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if path.is_dir() {
            depth > 0 && holds_charts(&path, depth - 1)
        } else {
            path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("tja"))
        }
    })
}

#[cfg_attr(any(debug_assertions, target_os = "windows"), allow(dead_code))]
fn is_instance_name(name: &str) -> bool {
    name == "OpenTaiko" || name.starts_with("OpenTaiko-")
}

/// An instance folder the Hub made: named with a UUID by `create_instance_dir`, or
/// holding the version.json the Hub writes after an install (with the system it was built
/// for). The old Hub also adopted a ~/Documents/OpenTaiko it had not made, such as a clone
/// of the game repository or the official installer's folder, whose shortcuts and scripts
/// name that place; neither carries these marks.
#[cfg_attr(any(debug_assertions, target_os = "windows"), allow(dead_code))]
fn is_hub_instance(dir: &Path, name: &str) -> bool {
    if name.strip_prefix("OpenTaiko-").is_some_and(crate::guard::is_uuid) {
        return true;
    }
    std::fs::read_to_string(dir.join("version.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .is_some_and(|json| json.get("build").is_some())
}

/// Files a file manager or the system leaves in any folder.
#[cfg_attr(any(debug_assertions, target_os = "windows"), allow(dead_code))]
fn is_system_file(name: &str) -> bool {
    matches!(name, ".DS_Store" | "desktop.ini" | "Thumbs.db" | ".directory") || name.starts_with("._")
}

/// A Songs folder the Hub filled: charts within a few levels, and at the top only genre
/// folders, each holding a box.def or charts. A folder of the user's own that a chart
/// was dropped into also has their music, albums or other files at the top, and stays
/// where it is.
#[cfg_attr(any(debug_assertions, target_os = "windows"), allow(dead_code))]
fn is_hub_library(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    holds_charts(dir, 4)
        && entries.flatten().all(|entry| {
            let path = entry.path();
            if path.is_dir() {
                path.join("box.def").is_file() || holds_charts(&path, 3)
            } else {
                is_system_file(&entry.file_name().to_string_lossy())
            }
        })
}

/// Moves what an older Hub kept directly in `old_root` (Documents on Linux and macOS)
/// into `new_root`. Only what is certainly the Hub's moves (see `is_hub_instance`,
/// `is_hub_library`), and only where the Hub has run, which the registry shows; plus Hub
/// 0.1's game folder and the Hub's own log files. Nothing is moved twice: a destination
/// that exists is left alone. Only Linux and macOS release builds call it, hence the
/// dead-code allowances here for every other build.
#[cfg_attr(any(debug_assertions, target_os = "windows"), allow(dead_code))]
pub fn migrate_documents_layout(old_root: &Path, new_root: &Path, registry: &Path, roots: &Path) -> Report {
    let mut report = Report::new(new_root);
    if crate::migrate::is_same_dir(old_root, new_root) {
        return report;
    }
    let mut moves: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut plan = |from: PathBuf, to: PathBuf| {
        if from.is_dir() && !to.exists() && !moves.iter().any(|(f, _)| f == &from) {
            moves.push((from, to));
        }
    };
    // Instances a previous start moved, but stopped before updating the lists
    let mut moved_before: Vec<(PathBuf, PathBuf)> = Vec::new();

    // Hub 0.1 kept its single game folder at <root>/OpenTaiko and kept no registry; the
    // version.json it wrote there tells it apart from a clone of the game repository.
    // Moved, it is then adopted as an instance from its new place, as before.
    let legacy = old_root.join("OpenTaiko");
    if legacy.join("version.json").is_file() {
        plan(legacy, new_root.join("OpenTaiko"));
    }

    let old_songs = old_root.join("Songs");
    let new_songs = new_root.join("Songs");
    if registry.is_file() {
        for instance in registered_paths(registry) {
            let in_old_root = instance.parent().is_some_and(|parent| crate::migrate::is_same_dir(parent, old_root));
            let name = instance.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if !in_old_root || !is_instance_name(&name) {
                continue;
            }
            let to = new_root.join(&name);
            if instance.is_dir() {
                if is_hub_instance(&instance, &name) {
                    plan(instance, to);
                } else {
                    report.note(format!("Left {} in place: the Hub did not create it", instance.display()));
                }
            } else if to.is_dir() {
                moved_before.push((instance, to));
            }
        }
        if is_hub_library(&old_songs) {
            plan(old_songs.clone(), new_songs.clone());
        } else if old_songs.is_dir() && holds_charts(&old_songs, 4) {
            report.note(format!("Left {} in place: it holds more than the Hub's songs", old_songs.display()));
        }
    }

    if !moves.is_empty() {
        if let Err(e) = std::fs::create_dir_all(new_root) {
            report.problem(format!("Could not create {}: {}", new_root.display(), e));
            return report;
        }
    }
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (from, to) in moves {
        match std::fs::rename(&from, &to) {
            Ok(()) => {
                report.moved(&from, &to);
                moved.push((from, to));
            }
            Err(e) => report.problem(format!("Could not move {}: {}", from.display(), e)),
        }
    }
    moved.extend(moved_before);

    // Registry entries of the moved instances follow them
    if !moved.is_empty() {
        let renamed = |path: &str| {
            moved
                .iter()
                .find(|(from, _)| crate::migrate::is_same_dir(Path::new(path), from) || Path::new(path) == from.as_path())
                .map(|(_, to)| to.to_string_lossy().into_owned())
        };
        rewrite_registries([registry, roots], renamed, &mut report);
    }

    // Each instance's Config.ini named the old library folder. Also once the library is
    // gone from its old place without this having been done (a start that stopped
    // midway): an instance that no longer names it is left as it is.
    if moved.iter().any(|(from, _)| from == &old_songs) || (!old_songs.exists() && new_songs.is_dir()) {
        for instance in registered_paths(registry) {
            match crate::configini::repoint_songs_entry(&instance, &old_songs, &new_songs) {
                Ok(true) => report.note(format!("Pointed {} at {}", instance.join("Config.ini").display(), new_songs.display())),
                Ok(false) => {}
                Err(e) => report.problem(format!("Could not update {}: {}", instance.join("Config.ini").display(), e)),
            }
        }
    }

    move_hub_logs(&old_root.join("logs"), &new_root.join("logs"), &mut report);
    report
}

/// The Hub's log files, from the logs folder it kept in Documents. The folder goes too
/// once nothing else is left in it.
#[cfg_attr(any(debug_assertions, target_os = "windows"), allow(dead_code))]
fn move_hub_logs(old: &Path, new: &Path, report: &mut Report) {
    let Ok(entries) = std::fs::read_dir(old) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.starts_with("OpenTaiko-Hub") && name.ends_with(".log")) || new.join(&name).exists() {
            continue;
        }
        let moved = std::fs::create_dir_all(new).and_then(|()| std::fs::rename(entry.path(), new.join(&name)));
        if let Err(e) = moved {
            report.note(format!("Could not move {}: {}", entry.path().display(), e));
        }
    }
    if std::fs::remove_dir(old).is_ok() {
        report.note(format!("Removed the old log folder {}", old.display()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn registry_with(dir: &Path, paths: &[PathBuf]) -> PathBuf {
        let registry = dir.join("instances.json");
        let instances: Vec<_> = paths.iter().enumerate().map(|(i, p)| serde_json::json!({ "id": i.to_string(), "path": p })).collect();
        fs::write(&registry, serde_json::json!({ "version": 1, "instances": instances }).to_string()).unwrap();
        registry
    }

    #[test]
    fn the_hubs_own_folders_move_out_of_documents() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("Documents");
        let hub = docs.join("OpenTaiko Hub");
        let sep = std::path::MAIN_SEPARATOR;
        fs::create_dir_all(docs.join("Songs/01 Pop/Song")).unwrap();
        fs::write(docs.join("Songs/01 Pop/Song/song.tja"), "TITLE:A").unwrap();
        let instance = docs.join("OpenTaiko-0f8fad5b-d9cb-469f-a165-70867728950e");
        fs::create_dir_all(&instance).unwrap();
        let old_entry = format!("{}{sep}", docs.join("Songs").display());
        fs::write(instance.join("Config.ini"), format!("[System]\r\nTJAPath=Songs{sep};{old_entry}\r\n")).unwrap();
        // Not the Hub's: a user folder in Documents, and an instance kept elsewhere
        fs::create_dir_all(docs.join("Photos")).unwrap();
        let elsewhere = docs.join("Games").join("OpenTaiko");
        fs::create_dir_all(&elsewhere).unwrap();
        let registry = registry_with(tmp.path(), &[instance.clone(), elsewhere.clone()]);
        let roots = tmp.path().join("roots").join("instance-roots.json");
        fs::create_dir_all(roots.parent().unwrap()).unwrap();
        fs::copy(&registry, &roots).unwrap();

        let report = migrate_documents_layout(&docs, &hub, &registry, &roots);
        assert_eq!(report.moved.len(), 2);
        assert!(report.problems.is_empty());

        assert!(hub.join("Songs/01 Pop/Song/song.tja").is_file());
        assert!(hub.join("OpenTaiko-0f8fad5b-d9cb-469f-a165-70867728950e").is_dir());
        assert!(docs.join("Photos").is_dir() && elsewhere.is_dir());

        // Both lists follow the moved instance
        let paths = registered_paths(&registry);
        assert_eq!(paths, vec![hub.join("OpenTaiko-0f8fad5b-d9cb-469f-a165-70867728950e"), elsewhere.clone()]);
        assert_eq!(registered_paths(&roots), vec![hub.join("OpenTaiko-0f8fad5b-d9cb-469f-a165-70867728950e"), elsewhere]);

        let ini = fs::read_to_string(hub.join("OpenTaiko-0f8fad5b-d9cb-469f-a165-70867728950e/Config.ini")).unwrap();
        assert!(ini.contains(&format!("TJAPath=Songs{sep};{}{sep}", hub.join("Songs").display())), "{ini}");

        // Nothing left to move on the next start
        assert!(migrate_documents_layout(&docs, &hub, &registry, &roots).is_quiet());
    }

    #[test]
    fn a_songs_folder_without_charts_is_not_the_hubs() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("Documents");
        fs::create_dir_all(docs.join("Songs/Albums")).unwrap();
        fs::write(docs.join("Songs/Albums/track.mp3"), "").unwrap();
        let registry = registry_with(tmp.path(), &[]);
        assert!(migrate_documents_layout(&docs, &docs.join("OpenTaiko Hub"), &registry, &tmp.path().join("absent.json")).is_quiet());
        assert!(docs.join("Songs/Albums/track.mp3").is_file());
    }

    #[test]
    fn nothing_moves_where_the_hub_never_ran() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("Documents");
        fs::create_dir_all(docs.join("Songs/A")).unwrap();
        fs::write(docs.join("Songs/A/a.tja"), "").unwrap();
        let absent = tmp.path().join("absent.json");
        assert!(migrate_documents_layout(&docs, &docs.join("OpenTaiko Hub"), &absent, &absent).is_quiet());
        assert!(docs.join("Songs/A/a.tja").is_file());
    }

    #[test]
    fn hub_0_1s_game_folder_moves_but_a_clone_of_the_game_does_not() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("Documents");
        fs::create_dir_all(docs.join("OpenTaiko")).unwrap();
        fs::write(docs.join("OpenTaiko/version.json"), r#"{"version":"0.5.4"}"#).unwrap();
        let hub = docs.join("OpenTaiko Hub");
        let absent = tmp.path().join("absent.json");
        migrate_documents_layout(&docs, &hub, &absent, &absent);
        assert!(hub.join("OpenTaiko/version.json").is_file());

        let other = tmp.path().join("Other");
        fs::create_dir_all(other.join("OpenTaiko/.git")).unwrap();
        assert!(migrate_documents_layout(&other, &other.join("OpenTaiko Hub"), &absent, &absent).is_quiet());
        assert!(other.join("OpenTaiko/.git").is_dir());
    }

    #[test]
    fn a_registered_folder_the_hub_did_not_make_stays() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("Documents");
        let hub = docs.join("OpenTaiko Hub");
        // A clone of the game the old Hub adopted, and the official installer's folder
        fs::create_dir_all(docs.join("OpenTaiko/.git")).unwrap();
        fs::create_dir_all(docs.join("OpenTaiko-installed")).unwrap();
        fs::write(docs.join("OpenTaiko-installed/start_opentaiko.sh"), "").unwrap();
        // An instance the Hub installed into, recognised by the version.json it wrote
        fs::create_dir_all(docs.join("OpenTaiko-mine")).unwrap();
        fs::write(docs.join("OpenTaiko-mine/version.json"), r#"{"version":"0.6.0.0","build":"Linux"}"#).unwrap();
        let registry = registry_with(tmp.path(), &[docs.join("OpenTaiko"), docs.join("OpenTaiko-installed"), docs.join("OpenTaiko-mine")]);
        let absent = tmp.path().join("absent.json");

        let report = migrate_documents_layout(&docs, &hub, &registry, &absent);
        assert_eq!(report.moved.len(), 1);
        assert!(docs.join("OpenTaiko/.git").is_dir());
        assert!(docs.join("OpenTaiko-installed/start_opentaiko.sh").is_file());
        assert!(hub.join("OpenTaiko-mine/version.json").is_file());
    }

    #[test]
    fn a_songs_folder_with_the_users_own_files_stays() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("Documents");
        // A chart dropped into the user's own music folder
        fs::create_dir_all(docs.join("Songs/Dropped")).unwrap();
        fs::write(docs.join("Songs/Dropped/a.tja"), "").unwrap();
        fs::create_dir_all(docs.join("Songs/Albums")).unwrap();
        fs::write(docs.join("Songs/track.mp3"), "").unwrap();
        let registry = registry_with(tmp.path(), &[]);
        let hub = docs.join("OpenTaiko Hub");
        assert!(migrate_documents_layout(&docs, &hub, &registry, &tmp.path().join("absent.json")).moved.is_empty());
        assert!(docs.join("Songs/track.mp3").is_file() && !hub.join("Songs").exists());

        // The Hub's own library, a file manager's file aside, does move
        fs::remove_file(docs.join("Songs/track.mp3")).unwrap();
        fs::remove_dir(docs.join("Songs/Albums")).unwrap();
        fs::create_dir_all(docs.join("Songs/Empty genre")).unwrap();
        fs::write(docs.join("Songs/Empty genre/box.def"), "#TITLE:Mine").unwrap();
        fs::write(docs.join("Songs/.DS_Store"), "").unwrap();
        assert_eq!(migrate_documents_layout(&docs, &hub, &registry, &tmp.path().join("absent.json")).moved.len(), 1);
        assert!(hub.join("Songs/Dropped/a.tja").is_file());
    }

    #[test]
    fn a_move_cut_short_before_the_lists_were_updated_is_finished() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("Documents");
        let hub = docs.join("OpenTaiko Hub");
        // The instance already sits in its new place, the lists still name the old one
        fs::create_dir_all(hub.join("OpenTaiko-0f8fad5b-d9cb-469f-a165-70867728950e")).unwrap();
        let registry = registry_with(tmp.path(), &[docs.join("OpenTaiko-0f8fad5b-d9cb-469f-a165-70867728950e")]);
        let roots = tmp.path().join("roots.json");
        fs::copy(&registry, &roots).unwrap();

        let report = migrate_documents_layout(&docs, &hub, &registry, &roots);
        assert!(report.problems.is_empty());
        assert_eq!(registered_paths(&registry), vec![hub.join("OpenTaiko-0f8fad5b-d9cb-469f-a165-70867728950e")]);
        assert_eq!(registered_paths(&roots), vec![hub.join("OpenTaiko-0f8fad5b-d9cb-469f-a165-70867728950e")]);
    }

    #[test]
    fn the_hubs_old_logs_follow_and_the_users_files_stay() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("Documents");
        let hub = docs.join("OpenTaiko Hub");
        fs::create_dir_all(docs.join("logs")).unwrap();
        fs::write(docs.join("logs/OpenTaiko-Hub.log"), "old log").unwrap();
        let absent = tmp.path().join("absent.json");
        migrate_documents_layout(&docs, &hub, &absent, &absent);
        assert_eq!(fs::read_to_string(hub.join("logs/OpenTaiko-Hub.log")).unwrap(), "old log");
        assert!(!docs.join("logs").exists());

        fs::create_dir_all(docs.join("logs")).unwrap();
        fs::write(docs.join("logs/mine.txt"), "").unwrap();
        migrate_documents_layout(&docs, &hub, &absent, &absent);
        assert!(docs.join("logs/mine.txt").is_file());
    }

    #[test]
    fn a_path_is_inside_a_root_only_at_a_separator() {
        assert_eq!(strip_root("C:\\x\\debug\\OpenTaiko", "C:\\x\\debug"), Some("\\OpenTaiko"));
        assert_eq!(strip_root("c:\\X\\DEBUG\\Songs", "C:\\x\\debug"), Some("\\Songs"));
        assert_eq!(strip_root("C:\\x\\debug", "C:\\x\\debug"), Some(""));
        assert_eq!(strip_root("C:\\x\\debugger\\a", "C:\\x\\debug"), None);
        assert_eq!(strip_root("C:\\y", "C:\\x\\debug"), None);
    }
}
