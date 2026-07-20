// Two-phase consolidation of an instance's local Songs folder into the shared global
// library.
//
// `plan_migration` classifies every song folder in the instance against the global
// folder — matched by uniqueId first, then by relative path — as:
//   - "new":       nothing comparable in the global folder → safe to move
//   - "identical": same uniqueId AND same chart hashes → the instance copy is redundant
//   - "conflict":  same uniqueId/path but different content → needs a user decision
//
// `apply_migration` executes the per-song decisions. Every decided song leaves the
// instance folder (moved, replaced into global, or discarded), so once the user applies
// a plan the instance no longer has user charts and the "transfer songs" prompt stops
// reappearing.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::fsops::{collect_song_dirs, copy_dir_recursive};
use crate::scan::{decode_text, extract_title, parse_unique_id};
use md5::{Digest, Md5};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SongEntry {
    pub rel_path: String,
    pub unique_id: Option<String>,
    pub title: Option<String>,
    pub md5s: Vec<String>,
    /// Last-edited time of the chart (max mtime of its .tja files), unix seconds.
    pub modified: Option<i64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MigrationItem {
    pub status: String, // "new" | "identical" | "conflict"
    pub src: SongEntry,
    pub dest: Option<SongEntry>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPlan {
    pub items: Vec<MigrationItem>,
    pub new_count: usize,
    pub identical_count: usize,
    pub conflict_count: usize,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MigrationDecision {
    pub src_rel_path: String,
    pub action: String, // "move" | "keep_global" | "use_instance"
    pub dest_rel_path: Option<String>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct MigrateSummary {
    pub moved: usize,
    pub replaced: usize,
    pub discarded: usize,
}

fn latest_modified_secs(paths: &[PathBuf]) -> Option<i64> {
    let mut latest: Option<i64> = None;
    for path in paths {
        if let Ok(meta) = std::fs::metadata(path) {
            if let Ok(modified) = meta.modified() {
                if let Ok(dur) = modified.duration_since(UNIX_EPOCH) {
                    let secs = dur.as_secs() as i64;
                    latest = Some(latest.map_or(secs, |l| l.max(secs)));
                }
            }
        }
    }
    latest
}

fn read_song_entry(dir: &Path, base: &Path) -> SongEntry {
    let rel_path = dir
        .strip_prefix(base)
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();

    let mut tjas: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_file()).unwrap_or(false)
                && entry.file_name().to_string_lossy().to_lowercase().ends_with(".tja")
            {
                tjas.push(entry.path());
            }
        }
    }
    tjas.sort();

    let mut md5s = Vec::with_capacity(tjas.len());
    let mut title = None;
    for (index, tja) in tjas.iter().enumerate() {
        if let Ok(bytes) = std::fs::read(tja) {
            let mut hasher = Md5::new();
            hasher.update(&bytes);
            md5s.push(hex::encode(hasher.finalize()));
            if index == 0 {
                let head = &bytes[..bytes.len().min(64 * 1024)];
                title = extract_title(&decode_text(head), "TITLE:");
            }
        }
    }
    // Compare by hash set, independent of file ordering
    md5s.sort();

    let unique_id = std::fs::read(dir.join("uniqueID.json"))
        .ok()
        .and_then(|bytes| parse_unique_id(&bytes));
    // Fall back to the folder mtime when the song has no readable .tja files
    let time_sources = if tjas.is_empty() { vec![dir.to_path_buf()] } else { tjas.clone() };
    let modified = latest_modified_secs(&time_sources);

    SongEntry { rel_path, unique_id, title, md5s, modified }
}

fn scan_entries(base: &Path) -> Vec<SongEntry> {
    let mut dirs = Vec::new();
    collect_song_dirs(base, &mut dirs);
    dirs.iter().map(|dir| read_song_entry(dir, base)).collect()
}

fn build_plan(src: &Path, dest: &Path) -> MigrationPlan {
    let src_entries = if src.is_dir() { scan_entries(src) } else { Vec::new() };
    let dest_entries = if dest.is_dir() { scan_entries(dest) } else { Vec::new() };

    let mut by_uid: HashMap<String, SongEntry> = HashMap::new();
    let mut by_rel: HashMap<String, SongEntry> = HashMap::new();
    for entry in &dest_entries {
        if let Some(uid) = &entry.unique_id {
            by_uid.entry(uid.clone()).or_insert_with(|| entry.clone());
        }
        by_rel.entry(entry.rel_path.clone()).or_insert_with(|| entry.clone());
    }

    let mut items = Vec::new();
    let (mut new_count, mut identical_count, mut conflict_count) = (0usize, 0usize, 0usize);
    for src_entry in src_entries {
        let dest_match = src_entry
            .unique_id
            .as_ref()
            .and_then(|uid| by_uid.get(uid))
            .or_else(|| by_rel.get(&src_entry.rel_path))
            .cloned();

        let status = match &dest_match {
            None => {
                new_count += 1;
                "new"
            }
            Some(dest_entry) if dest_entry.md5s == src_entry.md5s => {
                identical_count += 1;
                "identical"
            }
            Some(_) => {
                conflict_count += 1;
                "conflict"
            }
        };
        items.push(MigrationItem {
            status: status.to_string(),
            src: src_entry,
            dest: dest_match,
        });
    }

    MigrationPlan { items, new_count, identical_count, conflict_count }
}

#[tauri::command]
pub async fn plan_migration(src_songs: String, dest_songs: String) -> Result<MigrationPlan, String> {
    tokio::task::spawn_blocking(move || build_plan(&PathBuf::from(&src_songs), &PathBuf::from(&dest_songs)))
        .await
        .map_err(|e| format!("Task failed: {e}"))
}

fn remove_path(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        std::fs::remove_dir_all(path).map_err(|e| format!("Failed to remove {}: {}", path.display(), e))
    } else if path.exists() {
        std::fs::remove_file(path).map_err(|e| format!("Failed to remove {}: {}", path.display(), e))
    } else {
        Ok(())
    }
}

fn move_dir(src: &Path, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
    }
    if dest.exists() {
        remove_path(dest)?;
    }
    if std::fs::rename(src, dest).is_ok() {
        return Ok(());
    }
    copy_dir_recursive(src, dest)?;
    std::fs::remove_dir_all(src).map_err(|e| format!("Failed to remove {}: {}", src.display(), e))
}

fn apply_plan(src: &Path, dest: &Path, decisions: &[MigrationDecision]) -> Result<MigrateSummary, String> {
    if !src.is_dir() {
        return Ok(MigrateSummary::default());
    }
    std::fs::create_dir_all(dest).map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;

    let mut summary = MigrateSummary::default();
    let mut moved_rels: Vec<PathBuf> = Vec::new();

    for decision in decisions {
        let src_dir = src.join(&decision.src_rel_path);
        if !src_dir.is_dir() {
            continue;
        }
        match decision.action.as_str() {
            "keep_global" => {
                remove_path(&src_dir)?;
                summary.discarded += 1;
            }
            "use_instance" => {
                // Drop the conflicting global song, which may sit at a different rel path
                if let Some(dest_rel) = &decision.dest_rel_path {
                    let global = dest.join(dest_rel);
                    if global != dest.join(&decision.src_rel_path) {
                        remove_path(&global)?;
                    }
                }
                move_dir(&src_dir, &dest.join(&decision.src_rel_path))?;
                summary.replaced += 1;
                moved_rels.push(PathBuf::from(&decision.src_rel_path));
            }
            _ => {
                // "move" (new song) and any unknown action: place it in the shared library
                move_dir(&src_dir, &dest.join(&decision.src_rel_path))?;
                summary.moved += 1;
                moved_rels.push(PathBuf::from(&decision.src_rel_path));
            }
        }
    }

    // Bring over genre metadata (box.def / default.png) for every ancestor of a moved song
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for rel in &moved_rels {
        let mut current = rel.parent();
        while let Some(ancestor) = current {
            if ancestor.as_os_str().is_empty() {
                break;
            }
            if seen.insert(ancestor.to_path_buf()) {
                for file in ["box.def", "default.png"] {
                    let src_file = src.join(ancestor).join(file);
                    let dest_file = dest.join(ancestor).join(file);
                    if src_file.is_file() && !dest_file.exists() {
                        let _ = std::fs::copy(&src_file, &dest_file);
                    }
                }
            }
            current = ancestor.parent();
        }
    }

    Ok(summary)
}

#[tauri::command]
pub async fn apply_migration(
    src_songs: String,
    dest_songs: String,
    decisions: Vec<MigrationDecision>,
) -> Result<MigrateSummary, String> {
    tokio::task::spawn_blocking(move || {
        apply_plan(&PathBuf::from(&src_songs), &PathBuf::from(&dest_songs), &decisions)
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn item<'a>(plan: &'a MigrationPlan, rel: &str) -> &'a MigrationItem {
        plan.items.iter().find(|i| i.src.rel_path == rel).expect("item present")
    }

    #[test]
    fn classifies_new_identical_and_conflict() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("inst/Songs");
        let dest = tmp.path().join("global/Songs");

        // New: not in global
        write(&src.join("01 Pop/New/song.tja"), "TITLE:New\n");
        write(&src.join("01 Pop/New/uniqueID.json"), "{\"id\":\"new1\"}");
        // Identical: same uid, same content
        write(&src.join("01 Pop/Same/song.tja"), "TITLE:Same\n");
        write(&src.join("01 Pop/Same/uniqueID.json"), "{\"id\":\"same1\"}");
        write(&dest.join("01 Pop/Same/song.tja"), "TITLE:Same\n");
        write(&dest.join("01 Pop/Same/uniqueID.json"), "{\"id\":\"same1\"}");
        // Conflict: same uid, different content, and at a DIFFERENT path in global
        write(&src.join("01 Pop/Diff/song.tja"), "TITLE:Diff instance\n");
        write(&src.join("01 Pop/Diff/uniqueID.json"), "{\"id\":\"diff1\"}");
        write(&dest.join("05 Anime/DiffElsewhere/song.tja"), "TITLE:Diff global\n");
        write(&dest.join("05 Anime/DiffElsewhere/uniqueID.json"), "{\"id\":\"diff1\"}");

        let plan = build_plan(&src, &dest);
        assert_eq!(plan.new_count, 1);
        assert_eq!(plan.identical_count, 1);
        assert_eq!(plan.conflict_count, 1);
        assert_eq!(item(&plan, "01 Pop/New").status, "new");
        assert_eq!(item(&plan, "01 Pop/Same").status, "identical");
        let conflict = item(&plan, "01 Pop/Diff");
        assert_eq!(conflict.status, "conflict");
        // Conflict is matched by uniqueId even though the global path differs
        assert_eq!(conflict.dest.as_ref().unwrap().rel_path, "05 Anime/DiffElsewhere");
    }

    #[test]
    fn apply_moves_discards_and_replaces_then_empties_instance() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("inst/Songs");
        let dest = tmp.path().join("global/Songs");

        write(&src.join("01 Pop/box.def"), "#TITLE:Pop");
        write(&src.join("01 Pop/New/song.tja"), "TITLE:New\n");
        write(&src.join("01 Pop/Same/song.tja"), "TITLE:Same\n");
        write(&dest.join("01 Pop/Same/song.tja"), "TITLE:Same\n");
        write(&src.join("01 Pop/Diff/song.tja"), "TITLE:instance wins\n");
        write(&src.join("01 Pop/Diff/uniqueID.json"), "{\"id\":\"d\"}");
        write(&dest.join("05 Old/Diff/song.tja"), "TITLE:global loses\n");
        write(&dest.join("05 Old/Diff/uniqueID.json"), "{\"id\":\"d\"}");

        let decisions = vec![
            MigrationDecision { src_rel_path: "01 Pop/New".into(), action: "move".into(), dest_rel_path: None },
            MigrationDecision { src_rel_path: "01 Pop/Same".into(), action: "keep_global".into(), dest_rel_path: Some("01 Pop/Same".into()) },
            MigrationDecision { src_rel_path: "01 Pop/Diff".into(), action: "use_instance".into(), dest_rel_path: Some("05 Old/Diff".into()) },
        ];
        let summary = apply_plan(&src, &dest, &decisions).unwrap();

        assert_eq!(summary.moved, 1);
        assert_eq!(summary.discarded, 1);
        assert_eq!(summary.replaced, 1);

        // New song moved with its genre metadata
        assert_eq!(fs::read_to_string(dest.join("01 Pop/New/song.tja")).unwrap(), "TITLE:New\n");
        assert_eq!(fs::read_to_string(dest.join("01 Pop/box.def")).unwrap(), "#TITLE:Pop");
        // Instance version won: global copy removed from its old location, placed at instance's path
        assert!(!dest.join("05 Old/Diff").exists());
        assert_eq!(fs::read_to_string(dest.join("01 Pop/Diff/song.tja")).unwrap(), "TITLE:instance wins\n");
        // Every decided instance song is gone → the prompt won't reappear
        let mut leftover = Vec::new();
        collect_song_dirs(&src, &mut leftover);
        assert!(leftover.is_empty(), "instance should hold no more song folders");
    }
}
