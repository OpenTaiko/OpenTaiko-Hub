// Moving the Hub's data to another root folder, and repointing what refers to it: the
// instance registry, and the song library path in each instance's Config.ini.
//
// Two moves use this. Dev builds moved their data out of target/debug (devdata.rs).
// Linux and macOS used to keep it directly in the Documents folder under generic names
// (Songs, tmp, logs, OpenTaiko), where an existing ~/Documents/Songs was silently taken
// over as the library; they now use a folder of their own (see hub_root_dir).

use std::path::{Path, PathBuf};

/// Rewrites registry paths through `map` (None keeps a path as it is). Returns how many
/// changed.
pub(crate) fn rewrite_registry(registry: &Path, map: impl Fn(&str) -> Option<String>) -> Result<usize, String> {
    let Ok(text) = std::fs::read_to_string(registry) else { return Ok(0) };
    let mut json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut changed = 0;
    if let Some(list) = json.get_mut("instances").and_then(|l| l.as_array_mut()) {
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
        std::fs::write(registry, pretty).map_err(|e| e.to_string())?;
    }
    Ok(changed)
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

/// Moves what an older Hub kept directly in `old_root` (Documents on Linux and macOS)
/// into `new_root`. Only what is certainly the Hub's moves: registered instances it
/// created there, a Songs folder that holds charts (only where the Hub has run, which
/// the registry shows), and Hub 0.1's game folder. Nothing is moved twice: a destination
/// that exists is left alone. Returns what happened, for the log. Only Linux and macOS
/// release builds call it, hence the dead-code allowances here for every other build.
#[cfg_attr(any(debug_assertions, target_os = "windows"), allow(dead_code))]
pub fn migrate_documents_layout(old_root: &Path, new_root: &Path, registry: &Path) -> Vec<String> {
    let mut report = Vec::new();
    if crate::migrate::is_same_dir(old_root, new_root) {
        return report;
    }
    let mut moves: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut plan = |from: PathBuf, to: PathBuf| {
        if from.is_dir() && !to.exists() && !moves.iter().any(|(f, _)| f == &from) {
            moves.push((from, to));
        }
    };

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
            if in_old_root && is_instance_name(&name) {
                plan(instance.clone(), new_root.join(&name));
            }
        }
        if holds_charts(&old_songs, 4) {
            plan(old_songs.clone(), new_songs.clone());
        }
    }
    if moves.is_empty() {
        return report;
    }

    if let Err(e) = std::fs::create_dir_all(new_root) {
        report.push(format!("Could not create {}: {}", new_root.display(), e));
        return report;
    }
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (from, to) in moves {
        match std::fs::rename(&from, &to) {
            Ok(()) => {
                report.push(format!("Moved {} to {}", from.display(), to.display()));
                moved.push((from, to));
            }
            Err(e) => report.push(format!("Could not move {}: {}", from.display(), e)),
        }
    }

    // Registry entries of the moved instances follow them
    let renamed = |path: &str| {
        moved
            .iter()
            .find(|(from, _)| crate::migrate::is_same_dir(Path::new(path), from) || Path::new(path) == from.as_path())
            .map(|(_, to)| to.to_string_lossy().into_owned())
    };
    match rewrite_registry(registry, renamed) {
        Ok(0) => {}
        Ok(n) => report.push(format!("Updated {} instance path(s) in {}", n, registry.display())),
        Err(e) => report.push(format!("Could not update {}: {}", registry.display(), e)),
    }

    // Each instance's Config.ini named the old library folder
    if moved.iter().any(|(from, _)| from == &old_songs) {
        for instance in registered_paths(registry) {
            match crate::configini::repoint_songs_entry(&instance, &old_songs, &new_songs) {
                Ok(true) => report.push(format!("Pointed {} at {}", instance.join("Config.ini").display(), new_songs.display())),
                Ok(false) => {}
                Err(e) => report.push(format!("Could not update {}: {}", instance.join("Config.ini").display(), e)),
            }
        }
    }
    report
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
        let instance = docs.join("OpenTaiko-1234");
        fs::create_dir_all(&instance).unwrap();
        let old_entry = format!("{}{sep}", docs.join("Songs").display());
        fs::write(instance.join("Config.ini"), format!("[System]\r\nTJAPath=Songs{sep};{old_entry}\r\n")).unwrap();
        // Not the Hub's: a user folder in Documents, and an instance kept elsewhere
        fs::create_dir_all(docs.join("Photos")).unwrap();
        let elsewhere = docs.join("Games").join("OpenTaiko");
        fs::create_dir_all(&elsewhere).unwrap();
        let registry = registry_with(tmp.path(), &[instance.clone(), elsewhere.clone()]);

        let report = migrate_documents_layout(&docs, &hub, &registry);
        assert!(!report.is_empty());

        assert!(hub.join("Songs/01 Pop/Song/song.tja").is_file());
        assert!(hub.join("OpenTaiko-1234").is_dir());
        assert!(docs.join("Photos").is_dir() && elsewhere.is_dir());

        let paths = registered_paths(&registry);
        assert_eq!(paths, vec![hub.join("OpenTaiko-1234"), elsewhere]);

        let ini = fs::read_to_string(hub.join("OpenTaiko-1234/Config.ini")).unwrap();
        assert!(ini.contains(&format!("TJAPath=Songs{sep};{}{sep}", hub.join("Songs").display())), "{ini}");

        // Nothing left to move on the next start
        assert!(migrate_documents_layout(&docs, &hub, &registry).is_empty());
    }

    #[test]
    fn a_songs_folder_without_charts_is_not_the_hubs() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("Documents");
        fs::create_dir_all(docs.join("Songs/Albums")).unwrap();
        fs::write(docs.join("Songs/Albums/track.mp3"), "").unwrap();
        let registry = registry_with(tmp.path(), &[]);
        assert!(migrate_documents_layout(&docs, &docs.join("OpenTaiko Hub"), &registry).is_empty());
        assert!(docs.join("Songs/Albums/track.mp3").is_file());
    }

    #[test]
    fn nothing_moves_where_the_hub_never_ran() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("Documents");
        fs::create_dir_all(docs.join("Songs/A")).unwrap();
        fs::write(docs.join("Songs/A/a.tja"), "").unwrap();
        assert!(migrate_documents_layout(&docs, &docs.join("OpenTaiko Hub"), &tmp.path().join("absent.json")).is_empty());
        assert!(docs.join("Songs/A/a.tja").is_file());
    }

    #[test]
    fn hub_0_1s_game_folder_moves_but_a_clone_of_the_game_does_not() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = tmp.path().join("Documents");
        fs::create_dir_all(docs.join("OpenTaiko")).unwrap();
        fs::write(docs.join("OpenTaiko/version.json"), r#"{"version":"0.5.4"}"#).unwrap();
        let hub = docs.join("OpenTaiko Hub");
        migrate_documents_layout(&docs, &hub, &tmp.path().join("absent.json"));
        assert!(hub.join("OpenTaiko/version.json").is_file());

        let other = tmp.path().join("Other");
        fs::create_dir_all(other.join("OpenTaiko/.git")).unwrap();
        assert!(migrate_documents_layout(&other, &other.join("OpenTaiko Hub"), &tmp.path().join("absent.json")).is_empty());
        assert!(other.join("OpenTaiko/.git").is_dir());
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
