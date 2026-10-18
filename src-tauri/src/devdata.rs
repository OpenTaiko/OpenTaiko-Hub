// Where a development build keeps the Hub's data.
//
// A release build keeps it next to its executable on Windows (see hub_root_dir). For a
// dev build that was src-tauri/target/debug, where `cargo clean` deleted the test
// instances and songs along with the build output, and where gigabytes of game data were
// mixed into the build folder. Dev builds use <repo>/dev-hub instead, which git and
// Vite's watcher ignore, or the folder named by OTK_HUB_ROOT.

use std::path::{Path, PathBuf};

pub fn dev_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("OTK_HUB_ROOT").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap_or(manifest).join("dev-hub")
}

/// Folders the Hub creates in its root.
fn is_hub_data(name: &str) -> bool {
    matches!(name, "Songs" | "tmp" | "logs" | "OpenTaiko") || name.starts_with("OpenTaiko-")
}

/// Moves the data a dev build left next to its executable (from before dev_root existed)
/// into `new_root`, and points the instance registry and the Hub's own list of instance
/// folders at the new place. Once moved, there is nothing left to move, so this does
/// nothing on later starts. A folder that already exists at the destination is left where
/// it is. Returns what happened, for the log.
pub fn migrate_from(old_root: &Path, new_root: &Path, registry: &Path, roots: &Path) -> Vec<String> {
    let mut report = Vec::new();
    if crate::migrate::is_same_dir(old_root, new_root) {
        return report;
    }
    let Ok(entries) = std::fs::read_dir(old_root) else { return report };
    let mut moved = false;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_hub_data(&name) || !entry.path().is_dir() {
            continue;
        }
        let dest = new_root.join(&name);
        if dest.exists() {
            report.push(format!("Left {} in place: {} already exists", entry.path().display(), dest.display()));
            continue;
        }
        if let Err(e) = std::fs::create_dir_all(new_root) {
            report.push(format!("Could not create {}: {}", new_root.display(), e));
            return report;
        }
        // A rename on the same drive: gigabytes move instantly
        match std::fs::rename(entry.path(), &dest) {
            Ok(()) => {
                moved = true;
                report.push(format!("Moved dev data {} to {}", name, dest.display()));
            }
            Err(e) => report.push(format!("Could not move {}: {}", entry.path().display(), e)),
        }
    }
    if moved {
        // Every registry path under the old root moves with it
        let (old, new) = (old_root.to_string_lossy().into_owned(), new_root.to_string_lossy().into_owned());
        let moved_path = |path: &str| crate::relocate::strip_root(path, &old).map(|rest| format!("{new}{rest}"));
        crate::relocate::rewrite_registries([registry, roots], moved_path, &mut report);
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn only_hub_folders_move_and_the_registry_follows() {
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join("target").join("debug");
        let new = tmp.path().join("dev-hub");
        for dir in ["Songs/Pop", "OpenTaiko-1234", "OpenTaiko", "deps", "settings"] {
            fs::create_dir_all(old.join(dir)).unwrap();
        }
        fs::write(old.join("OpenTaiko-Hub.exe"), b"").unwrap();

        let registry = tmp.path().join("instances.json");
        let elsewhere = tmp.path().join("Games").join("OpenTaiko");
        let json = serde_json::json!({
            "version": 1,
            "activeId": "a",
            "instances": [
                { "id": "a", "path": old.join("OpenTaiko-1234") },
                { "id": "b", "path": elsewhere }
            ]
        });
        fs::write(&registry, json.to_string()).unwrap();

        let absent = tmp.path().join("absent.json");
        let report = migrate_from(&old, &new, &registry, &absent);
        assert!(!report.is_empty());

        // Hub data moved, build output and resources stayed
        assert!(new.join("Songs/Pop").is_dir());
        assert!(new.join("OpenTaiko-1234").is_dir());
        assert!(new.join("OpenTaiko").is_dir());
        assert!(old.join("deps").is_dir() && old.join("settings").is_dir());
        assert!(old.join("OpenTaiko-Hub.exe").is_file());
        assert!(!old.join("Songs").exists());

        // Only the instance that lived in the old root was repointed
        let updated: serde_json::Value = serde_json::from_str(&fs::read_to_string(&registry).unwrap()).unwrap();
        assert_eq!(updated["instances"][0]["path"], new.join("OpenTaiko-1234").to_string_lossy().as_ref());
        assert_eq!(updated["instances"][1]["path"], elsewhere.to_string_lossy().as_ref());
        assert_eq!(updated["activeId"], "a");

        // Nothing left to do on the next start
        assert!(migrate_from(&old, &new, &registry, &absent).is_empty());
    }

    #[test]
    fn a_folder_already_at_the_destination_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join("old");
        let new = tmp.path().join("new");
        fs::create_dir_all(old.join("Songs/A")).unwrap();
        fs::create_dir_all(new.join("Songs/B")).unwrap();
        let absent = tmp.path().join("absent.json");
        migrate_from(&old, &new, &absent, &absent);
        assert!(old.join("Songs/A").is_dir());
        assert!(new.join("Songs/B").is_dir());
        assert!(!new.join("Songs/A").exists());
    }
}
