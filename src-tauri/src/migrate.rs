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
use crate::library::resolve_entry_inside;
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

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPlan {
    pub items: Vec<MigrationItem>,
    pub new_count: usize,
    pub identical_count: usize,
    pub conflict_count: usize,
}

/// What happens to one song of the instance. An unknown value is refused when the
/// request is read rather than falling through to one of these.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MigrationAction {
    /// A new song: move it into the shared library.
    Move,
    /// Keep the library's copy and delete the instance's.
    KeepGlobal,
    /// Replace the library's copy with the instance's.
    UseInstance,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MigrationDecision {
    pub src_rel_path: String,
    pub action: MigrationAction,
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
    // Both sides being the same folder means there is nothing to move, and every song
    // would otherwise be reported as "identical" — whose resolution deletes the source.
    if is_same_dir(src, dest) {
        return MigrationPlan::default();
    }
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

/// Lets the frontend ask the same question the guards ask, instead of comparing
/// lowercased strings, which cannot see through a symlink.
#[tauri::command]
pub async fn paths_are_same_dir(app: tauri::AppHandle, a: String, b: String) -> bool {
    // Answering for folders outside the scope would tell the page which exist
    if crate::guard::checked(&app, &a).is_err() || crate::guard::checked(&app, &b).is_err() {
        return false;
    }
    tokio::task::spawn_blocking(move || is_same_dir(&PathBuf::from(&a), &PathBuf::from(&b)))
        .await
        .unwrap_or(false)
}

#[tauri::command]
pub async fn plan_migration(app: tauri::AppHandle, src_songs: String, dest_songs: String) -> Result<MigrationPlan, String> {
    crate::guard::checked(&app, &src_songs)?;
    crate::guard::checked(&app, &dest_songs)?;
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
    // The destructive guard. Resolving "keep_global" deletes the source copy, so if the
    // instance's Songs folder is a link to the shared library this would delete the
    // library itself.
    if is_same_dir(src, dest) {
        return Err(
            "Refusing to migrate: the instance's Songs folder is the shared library itself"
                .to_string(),
        );
    }
    std::fs::create_dir_all(dest).map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;

    let mut summary = MigrateSummary::default();
    let mut moved_rels: Vec<PathBuf> = Vec::new();

    for decision in decisions {
        // Relative paths come from the page: each must stay inside its Songs folder, since
        // the folders they name get deleted or replaced
        let src_dir = resolve_entry_inside(src, &decision.src_rel_path)?;
        let dest_dir = resolve_entry_inside(dest, &decision.src_rel_path)?;
        if !src_dir.is_dir() {
            continue;
        }
        match decision.action {
            MigrationAction::KeepGlobal => {
                remove_path(&src_dir)?;
                summary.discarded += 1;
            }
            MigrationAction::UseInstance => {
                // Drop the conflicting global song, which may sit at a different rel path
                if let Some(dest_rel) = &decision.dest_rel_path {
                    let global = resolve_entry_inside(dest, dest_rel)?;
                    if global != dest_dir {
                        remove_path(&global)?;
                    }
                }
                move_dir(&src_dir, &dest_dir)?;
                summary.replaced += 1;
                moved_rels.push(PathBuf::from(&decision.src_rel_path));
            }
            MigrationAction::Move => {
                move_dir(&src_dir, &dest_dir)?;
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

    // Prune the genre folders we just emptied. Leaving them behind would make the game
    // show a second, empty box next to the shared library's real one, because a stable
    // instance reads both its local Songs folder and the shared library.
    let mut ancestors: Vec<PathBuf> = seen.into_iter().collect();
    // Deepest first, so a parent can be pruned once its children are gone
    ancestors.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for ancestor in ancestors {
        let src_dir = src.join(&ancestor);
        // Only ever remove a song-less folder whose metadata now lives in the library
        if src_dir.is_dir() && !contains_tja(&src_dir) && dest.join(&ancestor).is_dir() {
            let _ = std::fs::remove_dir_all(&src_dir);
        }
    }

    Ok(summary)
}

/// True when both paths name the same folder on disk, following symlinks.
///
/// A raw `==` is not enough: an instance whose Songs folder is a symlink (or a bind
/// mount) to the shared library spells a different path for the same directory, and
/// every operation here would then treat the library as a separate copy to prune.
pub(crate) fn is_same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        // An unreadable path cannot be proven different, so fall back to the literal
        // comparison rather than assuming they are distinct.
        _ => a == b,
    }
}

/// Following symlinks can cycle forever, so the walk is bounded. Real chart trees are a
/// handful of levels deep.
const MAX_CHART_SCAN_DEPTH: usize = 24;

/// True when the folder holds at least one .tja anywhere beneath it.
pub(crate) fn contains_tja(dir: &Path) -> bool {
    contains_tja_within(dir, MAX_CHART_SCAN_DEPTH)
}

fn contains_tja_within(dir: &Path, depth_left: usize) -> bool {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return false,
    };
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        // metadata() follows symlinks where DirEntry::file_type() does not. A song
        // folder linked in from another drive must still count as holding charts,
        // otherwise the duplicate cleaner reads it as an empty box and deletes it.
        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_dir() {
            subdirs.push(path);
        } else if meta.is_file()
            && entry.file_name().to_string_lossy().to_lowercase().ends_with(".tja")
        {
            return true;
        }
    }
    if depth_left == 0 {
        // Too deep to finish checking. Report "holds charts" when anything is left
        // unexamined, so a folder is never deleted on an unproven guess.
        return !subdirs.is_empty();
    }
    subdirs
        .iter()
        .any(|sub| contains_tja_within(sub, depth_left - 1))
}

/// True when the folder holds no file at all anywhere beneath it.
fn is_recursively_empty(dir: &Path) -> bool {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return false,
    };
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        match entry.file_type() {
            Ok(t) if t.is_dir() => subdirs.push(entry.path()),
            Ok(t) if t.is_file() => return false,
            _ => {}
        }
    }
    subdirs.iter().all(|sub| is_recursively_empty(sub))
}

/// Removes the chart content shipped inside a build's own Songs folder, keeping the
/// functional scaffolding (the box.def-only boxes such as X1 Favorite, X2 Recent, the
/// search boxes and the local drop folders).
///
/// A local `dotnet publish` copies the repository's whole Songs tree, so an
/// experimental build would otherwise install its own copies of songs alongside the
/// shared library. Returns the number of song folders removed.
#[tauri::command]
pub async fn strip_song_content(app: tauri::AppHandle, songs_dir: String) -> Result<usize, String> {
    crate::guard::checked(&app, &songs_dir)?;
    strip_song_content_in(songs_dir).await
}

async fn strip_song_content_in(songs_dir: String) -> Result<usize, String> {
    tokio::task::spawn_blocking(move || {
        let base = PathBuf::from(&songs_dir);
        if !base.is_dir() {
            return 0;
        }

        let mut song_dirs = Vec::new();
        collect_song_dirs(&base, &mut song_dirs);
        let mut removed = 0;
        for dir in song_dirs {
            // Never remove the Songs folder itself, only song folders inside it
            if dir == base {
                continue;
            }
            if std::fs::remove_dir_all(&dir).is_ok() {
                removed += 1;
            }
        }

        // Drop containers that held nothing but those songs, deepest first
        let mut leftovers = Vec::new();
        collect_dirs(&base, &mut leftovers);
        leftovers.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
        for dir in leftovers {
            if dir != base && is_recursively_empty(&dir) {
                let _ = std::fs::remove_dir_all(&dir);
            }
        }

        removed
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))
}

fn collect_dirs(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let path = entry.path();
                collect_dirs(&path, out);
                out.push(path);
            }
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateFolder {
    pub rel_path: String,
    pub file_count: usize,
}

fn count_files(dir: &Path) -> usize {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return 0,
    };
    let mut total = 0;
    for entry in entries.flatten() {
        match entry.file_type() {
            Ok(t) if t.is_dir() => total += count_files(&entry.path()),
            Ok(t) if t.is_file() => total += 1,
            _ => {}
        }
    }
    total
}

/// Finds top-level folders in an instance's Songs folder that hold no chart at all while
/// the shared library has a folder of the same name that does. Because the game reads
/// both paths, each of these renders as a duplicate, empty box.
///
/// Folders with no counterpart in the library are never reported, which is what keeps
/// the game's own boxes (X1 Favorite, X2 Recent, the search boxes, the local drop
/// folders) untouched.
#[tauri::command]
pub async fn find_duplicate_song_folders(
    app: tauri::AppHandle,
    instance_songs: String,
    global_songs: String,
) -> Result<Vec<DuplicateFolder>, String> {
    crate::guard::checked(&app, &instance_songs)?;
    crate::guard::checked(&app, &global_songs)?;
    find_duplicates_in(instance_songs, global_songs).await
}

async fn find_duplicates_in(instance_songs: String, global_songs: String) -> Result<Vec<DuplicateFolder>, String> {
    tokio::task::spawn_blocking(move || {
        let src = PathBuf::from(&instance_songs);
        let dest = PathBuf::from(&global_songs);
        let mut out = Vec::new();
        if !src.is_dir() || !dest.is_dir() {
            return out;
        }
        // Never report anything when both paths are the same folder
        if is_same_dir(&src, &dest) {
            return out;
        }
        let entries = match std::fs::read_dir(&src) {
            Ok(entries) => entries,
            Err(_) => return out,
        };
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let path = entry.path();
            let counterpart = dest.join(entry.file_name());
            if !contains_tja(&path) && counterpart.is_dir() && contains_tja(&counterpart) {
                out.push(DuplicateFolder {
                    rel_path: entry.file_name().to_string_lossy().to_string(),
                    file_count: count_files(&path),
                });
            }
        }
        out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        out
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))
}

/// Removes the given top-level folders from an instance's Songs folder. Each one is
/// re-checked to be chart-less and duplicated in the library before deletion, so a
/// stale UI list can never delete real songs.
#[tauri::command]
pub async fn remove_duplicate_song_folders(
    app: tauri::AppHandle,
    instance_songs: String,
    global_songs: String,
    rel_paths: Vec<String>,
) -> Result<usize, String> {
    crate::guard::checked(&app, &instance_songs)?;
    remove_duplicates_in(instance_songs, global_songs, rel_paths).await
}

async fn remove_duplicates_in(
    instance_songs: String,
    global_songs: String,
    rel_paths: Vec<String>,
) -> Result<usize, String> {
    tokio::task::spawn_blocking(move || {
        let src = PathBuf::from(&instance_songs);
        let dest = PathBuf::from(&global_songs);
        if is_same_dir(&src, &dest) {
            return Err("Refusing to prune: both paths are the same folder".to_string());
        }
        let mut removed = 0;
        for rel in rel_paths {
            // Plain child names only: never traverse out of the Songs folder, and never "."
            // (the Songs folder itself, which is chart-less after stripping)
            let mut parts = Path::new(&rel).components();
            if !matches!((parts.next(), parts.next()), (Some(std::path::Component::Normal(_)), None))
                || rel.contains('/')
                || rel.contains('\\')
            {
                continue;
            }
            let path = src.join(&rel);
            let counterpart = dest.join(&rel);
            if path.is_dir()
                && !contains_tja(&path)
                && counterpart.is_dir()
                && contains_tja(&counterpart)
            {
                std::fs::remove_dir_all(&path)
                    .map_err(|e| format!("Failed to remove {}: {}", path.display(), e))?;
                removed += 1;
            }
        }
        Ok(removed)
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?
}

#[tauri::command]
pub async fn apply_migration(
    app: tauri::AppHandle,
    src_songs: String,
    dest_songs: String,
    decisions: Vec<MigrationDecision>,
) -> Result<MigrateSummary, String> {
    crate::guard::checked(&app, &src_songs)?;
    crate::guard::checked(&app, &dest_songs)?;
    let _moving = crate::jobs::in_flight(&app);
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

    #[test]
    fn a_folder_is_the_same_as_itself_however_it_is_spelled() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        fs::create_dir_all(base.join("Songs")).unwrap();

        assert!(is_same_dir(&base.join("Songs"), &base.join("Songs")));
        // A detour through "." and ".." resolves to the same directory
        assert!(is_same_dir(
            &base.join("Songs"),
            &base.join("Songs").join(".").join("..").join("Songs")
        ));

        fs::create_dir_all(base.join("Other")).unwrap();
        assert!(!is_same_dir(&base.join("Songs"), &base.join("Other")));
    }

    #[test]
    fn a_decision_cannot_reach_outside_the_songs_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("Instance/Songs");
        let dest = tmp.path().join("Songs");
        write(&src.join("01 Pop/Song/oni.tja"), "TITLE:A\n");
        write(&tmp.path().join("Precious/keep.txt"), "x");
        fs::create_dir_all(&dest).unwrap();

        for (src_rel, dest_rel) in [("../../Precious", None), ("01 Pop/Song", Some("../Precious"))] {
            let decisions = vec![MigrationDecision {
                src_rel_path: src_rel.into(),
                action: MigrationAction::UseInstance,
                dest_rel_path: dest_rel.map(String::from),
            }];
            assert!(apply_plan(&src, &dest, &decisions).is_err());
        }
        assert!(tmp.path().join("Precious/keep.txt").exists());
    }

    #[test]
    fn migrating_a_folder_onto_itself_is_refused_rather_than_deleting_it() {
        let tmp = tempfile::tempdir().unwrap();
        let songs = tmp.path().join("Songs");
        write(&songs.join("01 Pop/Song/oni.tja"), "TITLE:A
");

        // The plan must be empty, so the UI never offers the migration...
        let plan = build_plan(&songs, &songs);
        assert!(plan.items.is_empty(), "nothing can be migrated onto itself");

        // ...and applying one anyway must refuse instead of deleting the library.
        let decisions = vec![MigrationDecision {
            src_rel_path: "01 Pop/Song".into(),
            action: MigrationAction::KeepGlobal,
            dest_rel_path: Some("01 Pop/Song".into()),
        }];
        let result = apply_plan(&songs, &songs, &decisions);
        assert!(result.is_err(), "applying onto itself must be refused");
        assert!(
            songs.join("01 Pop/Song/oni.tja").exists(),
            "the chart must survive the refusal"
        );
    }

    #[cfg(unix)]
    #[test]
    fn charts_reachable_only_through_a_symlink_still_count_as_charts() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();

        // A real chart folder somewhere else on disk...
        let elsewhere = tmp.path().join("elsewhere/Song");
        write(&elsewhere.join("oni.tja"), "TITLE:A\n");

        // ...linked into an otherwise chart-less box, the way a user splits a library
        // across drives. DirEntry::file_type() does not traverse the link, so this used
        // to read as an empty box and the duplicate cleaner would delete it.
        let box_dir = tmp.path().join("Songs/01 Pop");
        fs::create_dir_all(&box_dir).unwrap();
        symlink(&elsewhere, box_dir.join("Song")).unwrap();

        assert!(
            contains_tja(&box_dir),
            "a box whose only charts arrive through a link must not read as empty"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_songs_folder_symlinked_to_the_library_is_recognised_as_the_same_folder() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();

        let library = tmp.path().join("Library/Songs");
        write(&library.join("01 Pop/Song/oni.tja"), "TITLE:A\n");

        // The instance points its Songs folder at the shared library, which is exactly
        // the arrangement that used to make the migration delete the library.
        let instance = tmp.path().join("Instance/Songs");
        fs::create_dir_all(instance.parent().unwrap()).unwrap();
        symlink(&library, &instance).unwrap();

        assert!(
            is_same_dir(&instance, &library),
            "a symlinked Songs folder is the same folder as its target"
        );
        assert!(
            build_plan(&instance, &library).items.is_empty(),
            "nothing may be proposed for migration"
        );

        let decisions = vec![MigrationDecision {
            src_rel_path: "01 Pop/Song".into(),
            action: MigrationAction::KeepGlobal,
            dest_rel_path: Some("01 Pop/Song".into()),
        }];
        assert!(apply_plan(&instance, &library, &decisions).is_err());
        assert!(
            library.join("01 Pop/Song/oni.tja").exists(),
            "the library must survive"
        );
    }

    #[test]
    fn a_folder_whose_charts_are_only_reachable_through_a_link_is_not_empty() {
        // contains_tja guards a delete, so it must follow links rather than report a
        // linked-in song folder as a chart-less duplicate box.
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real");
        write(&real.join("Song/oni.tja"), "TITLE:A
");
        assert!(contains_tja(&real), "a plain chart tree holds charts");

        let empty = tmp.path().join("empty");
        fs::create_dir_all(empty.join("nested")).unwrap();
        assert!(!contains_tja(&empty), "a box with no charts holds none");
    }

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
            MigrationDecision { src_rel_path: "01 Pop/New".into(), action: MigrationAction::Move, dest_rel_path: None },
            MigrationDecision { src_rel_path: "01 Pop/Same".into(), action: MigrationAction::KeepGlobal, dest_rel_path: Some("01 Pop/Same".into()) },
            MigrationDecision { src_rel_path: "01 Pop/Diff".into(), action: MigrationAction::UseInstance, dest_rel_path: Some("05 Old/Diff".into()) },
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

        // The emptied genre folder is pruned, so the game cannot show a duplicate,
        // empty box next to the shared library's real one
        assert!(!src.join("01 Pop").exists(), "emptied genre folder should be pruned");
    }

    #[test]
    fn prune_keeps_folders_that_still_hold_charts() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("inst/Songs");
        let dest = tmp.path().join("global/Songs");

        write(&src.join("01 Pop/box.def"), "#TITLE:Pop");
        write(&src.join("01 Pop/Moved/song.tja"), "TITLE:Moved\n");
        write(&src.join("01 Pop/Kept/song.tja"), "TITLE:Kept\n");

        // Only migrate one of the two songs
        let decisions = vec![MigrationDecision {
            src_rel_path: "01 Pop/Moved".into(),
            action: MigrationAction::Move,
            dest_rel_path: None,
        }];
        apply_plan(&src, &dest, &decisions).unwrap();

        // The genre folder still holds a chart, so it must survive
        assert!(src.join("01 Pop/Kept/song.tja").is_file());
        assert!(src.join("01 Pop").is_dir(), "folder with remaining charts must be kept");
    }

    #[test]
    fn strip_removes_shipped_songs_but_keeps_functional_boxes() {
        let tmp = tempfile::tempdir().unwrap();
        let songs = tmp.path().join("publish/Songs");

        // Real chart content a local publish would carry over
        write(&songs.join("05 Chapter V/box.def"), "#TITLE:V");
        write(&songs.join("05 Chapter V/Song A/song.tja"), "TITLE:A\n");
        write(&songs.join("05 Chapter V/Song A/song.ogg"), "audio");
        write(&songs.join("S1 Dan-i Dojo/box.def"), "#TITLE:Dan");
        write(&songs.join("S1 Dan-i Dojo/Dan 1/dan.tja"), "TITLE:D\n");
        // Functional scaffolding that must survive untouched
        write(&songs.join("X1 Favorite/box.def"), "#TITLE:Fav");
        write(&songs.join("X1 Favorite/! Keep this folder empty !"), "");
        write(&songs.join("L2 Custom Charts/box.def"), "#TITLE:Custom");
        write(&songs.join("L2 Custom Charts/01 Pop/box.def"), "#TITLE:Pop");
        write(&songs.join("L3 Downloaded Songs/box.def"), "#TITLE:DL");

        let removed = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(strip_song_content_in(songs.to_str().unwrap().into()))
            .unwrap();
        assert_eq!(removed, 2, "both song folders should be removed");

        // No chart survives anywhere under Songs
        assert!(!contains_tja(&songs), "no chart may remain in the build's Songs folder");
        assert!(!songs.join("05 Chapter V/Song A").exists());
        assert!(!songs.join("S1 Dan-i Dojo/Dan 1").exists());
        // Scaffolding, including nested genre boxes, is intact
        assert!(songs.join("X1 Favorite/box.def").is_file());
        assert!(songs.join("X1 Favorite/! Keep this folder empty !").is_file());
        assert!(songs.join("L2 Custom Charts/01 Pop/box.def").is_file());
        assert!(songs.join("L3 Downloaded Songs/box.def").is_file());
        // Boxes whose own box.def remains are kept (their metadata is still meaningful)
        assert!(songs.join("S1 Dan-i Dojo/box.def").is_file());
        assert!(songs.join("05 Chapter V/box.def").is_file());
        // And the Songs folder itself always survives
        assert!(songs.is_dir());
    }

    #[test]
    fn a_loose_tja_at_the_root_does_not_swallow_the_whole_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("inst/Songs");
        let dest = tmp.path().join("global/Songs");

        // A chart sitting directly in Songs/ next to real song folders
        write(&src.join("Stray.tja"), "TITLE:Stray\n");
        write(&src.join("01 Pop/Song A/song.tja"), "TITLE:A\n");

        let plan = build_plan(&src, &dest);
        let paths: Vec<&str> = plan.items.iter().map(|i| i.src.rel_path.as_str()).collect();
        assert_eq!(paths, vec!["01 Pop/Song A"], "the root must not be planned as one song");

        // Migrating moves the song, never the whole Songs folder
        let decisions = vec![MigrationDecision {
            src_rel_path: "01 Pop/Song A".into(),
            action: MigrationAction::Move,
            dest_rel_path: None,
        }];
        apply_plan(&src, &dest, &decisions).unwrap();
        assert!(src.join("Stray.tja").is_file(), "the loose chart stays where it was");
        assert!(src.is_dir(), "the Songs folder itself is never moved away");
        assert!(dest.join("01 Pop/Song A/song.tja").is_file());
    }

    #[test]
    fn strip_is_a_noop_without_a_songs_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("publish/Songs");
        let removed = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(strip_song_content_in(missing.to_str().unwrap().into()))
            .unwrap();
        assert_eq!(removed, 0);
    }

    #[test]
    fn finds_only_chartless_folders_duplicated_in_the_library() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("inst/Songs");
        let dest = tmp.path().join("global/Songs");

        // Chart-less leftover whose library counterpart has songs → a duplicate box
        write(&src.join("01 Chapter I/box.def"), "#TITLE:I");
        write(&src.join("01 Chapter I/default.png"), "png");
        write(&dest.join("01 Chapter I/Song/song.tja"), "TITLE:S\n");
        // Shipped scaffolding with a populated library counterpart → also a duplicate
        write(&src.join("S1 Dan-i Dojo/box.def"), "#TITLE:Dan");
        write(&dest.join("S1 Dan-i Dojo/Dan/dan.tja"), "TITLE:D\n");
        // The game's own boxes have no counterpart in the library → must be kept
        write(&src.join("X1 Favorite/box.def"), "#TITLE:Fav");
        write(&src.join("L3 Downloaded Songs/box.def"), "#TITLE:DL");
        // A local folder that still holds a chart → must be kept
        write(&src.join("L2 Custom Charts/Mine/mine.tja"), "TITLE:Mine\n");
        write(&dest.join("L2 Custom Charts/Other/other.tja"), "TITLE:Other\n");

        let src_s = src.to_str().unwrap();
        let dest_s = dest.to_str().unwrap();
        let found = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(find_duplicates_in(src_s.into(), dest_s.into()))
            .unwrap();
        let names: Vec<&str> = found.iter().map(|f| f.rel_path.as_str()).collect();
        assert_eq!(names, vec!["01 Chapter I", "S1 Dan-i Dojo"]);

        // Removal takes only those, and re-verifies before deleting
        let removed = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(remove_duplicates_in(
                src_s.into(),
                dest_s.into(),
                vec![
                    "01 Chapter I".into(),
                    "S1 Dan-i Dojo".into(),
                    "X1 Favorite".into(),      // no counterpart: must be refused
                    "L2 Custom Charts".into(), // holds a chart: must be refused
                    "../escape".into(),        // path traversal: must be refused
                ],
            ))
            .unwrap();
        assert_eq!(removed, 2);
        assert!(!src.join("01 Chapter I").exists());
        assert!(!src.join("S1 Dan-i Dojo").exists());
        assert!(src.join("X1 Favorite/box.def").is_file(), "game boxes must be kept");
        assert!(src.join("L3 Downloaded Songs/box.def").is_file());
        assert!(src.join("L2 Custom Charts/Mine/mine.tja").is_file());
    }

    #[test]
    fn the_songs_folder_itself_is_never_a_duplicate() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("inst/Songs");
        let dest = tmp.path().join("global/Songs");
        // A stripped instance library holds no chart, and the shared one does
        write(&src.join("X1 Favorite/box.def"), "#TITLE:Fav");
        write(&dest.join("Pop/Song/song.tja"), "TITLE:S\n");

        let removed = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(remove_duplicates_in(
                src.to_str().unwrap().into(),
                dest.to_str().unwrap().into(),
                vec![".".into(), "".into()],
            ))
            .unwrap();
        assert_eq!(removed, 0);
        assert!(src.join("X1 Favorite/box.def").is_file());
    }

    #[test]
    fn a_migration_never_names_a_whole_songs_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("inst/Songs");
        let dest = tmp.path().join("global/Songs");
        write(&src.join("Mine/mine.tja"), "TITLE:Mine\n");
        write(&dest.join("Pop/Song/song.tja"), "TITLE:S\n");
        let decision = MigrationDecision {
            src_rel_path: "Mine".into(),
            action: MigrationAction::UseInstance,
            dest_rel_path: Some(".".into()),
        };
        assert!(apply_plan(&src, &dest, &[decision]).is_err());
        assert!(dest.join("Pop/Song/song.tja").is_file(), "the shared library must survive");
    }
}
