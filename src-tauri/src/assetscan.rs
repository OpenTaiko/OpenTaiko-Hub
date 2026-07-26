// Native scan for installed skins / characters / puchicharas. Replaces the old JS
// crawler (one IPC round-trip per directory + per config file — thousands of them on a
// full build, which floods the WebView event loop and freezes the UI). Walks the asset
// base folder once, finds every folder holding the marker config file, and reads its
// version — all natively, returned in a single call.
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::scan::decode_text;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssetVersion {
    pub rel_path: String,
    pub version: String,
}

/// Mirrors the game/Hub convention: the first line whose key (left of '=') contains a
/// word ending in "Version" provides the asset version.
fn extract_asset_version(content: &str) -> Option<String> {
    for line in content.lines() {
        let Some(eq) = line.find('=') else { continue };
        let key = &line[..eq];
        let has_version_word = key
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|w| !w.is_empty() && w.ends_with("Version"));
        if has_version_word {
            let value = line[eq + 1..].trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn walk(dir: &Path, base: &Path, target: &str, out: &mut Vec<AssetVersion>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    let mut subdirs: Vec<PathBuf> = Vec::new();
    let mut has_target = false;
    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            subdirs.push(entry.path());
        } else if file_type.is_file() && entry.file_name().to_string_lossy() == target {
            has_target = true;
        }
    }

    if has_target {
        let rel_path = dir
            .strip_prefix(base)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let version = std::fs::read(dir.join(target))
            .ok()
            .and_then(|bytes| extract_asset_version(&decode_text(&bytes)))
            .unwrap_or_else(|| "Unknown".to_string());
        out.push(AssetVersion { rel_path, version });
    }

    for sub in subdirs {
        walk(&sub, base, target, out);
    }
}

#[tauri::command]
pub async fn scan_asset_versions(base_dir: String, target_file: String) -> Result<Vec<AssetVersion>, String> {
    tokio::task::spawn_blocking(move || {
        let base = PathBuf::from(&base_dir);
        let mut out = Vec::new();
        if base.is_dir() {
            walk(&base, &base, &target_file, &mut out);
        }
        Ok(out)
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_version_from_config_lines() {
        assert_eq!(extract_asset_version("SkinName=Foo\nSkinVersion=0.6.1.0\n").as_deref(), Some("0.6.1.0"));
        assert_eq!(extract_asset_version("charaVersion = 1.2 \n").as_deref(), Some("1.2"));
        assert_eq!(extract_asset_version("Version=3\n").as_deref(), Some("3"));
        assert_eq!(extract_asset_version("Name=NoVersionHere\n"), None);
        // A value that itself contains '=' must not be mistaken for the key
        assert_eq!(extract_asset_version("Author=a=b\nVersion=9\n").as_deref(), Some("9"));
    }

    #[test]
    fn finds_asset_folders_at_any_depth() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        std::fs::create_dir_all(base.join("SkinA")).unwrap();
        std::fs::write(base.join("SkinA/SkinConfig.ini"), "SkinVersion=1.0.0.0").unwrap();
        std::fs::create_dir_all(base.join("Group/SkinB")).unwrap();
        std::fs::write(base.join("Group/SkinB/SkinConfig.ini"), "SkinVersion=2.0.0.0").unwrap();
        // A folder without the marker must be ignored
        std::fs::create_dir_all(base.join("NotASkin/assets")).unwrap();
        std::fs::write(base.join("NotASkin/assets/x.png"), "img").unwrap();

        let mut out = Vec::new();
        walk(base, base, "SkinConfig.ini", &mut out);
        let mut paths: Vec<_> = out.iter().map(|a| (a.rel_path.as_str(), a.version.as_str())).collect();
        paths.sort();
        assert_eq!(paths, vec![("Group/SkinB", "2.0.0.0"), ("SkinA", "1.0.0.0")]);
    }
}
