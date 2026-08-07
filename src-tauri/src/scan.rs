// Native song library scanner. Replaces the old JS crawler (one IPC round-trip per
// file) with a single command that walks the whole tree, hashes .tja files, extracts
// titles and genre metadata, and streams progress batches to the frontend.
use md5::{Digest, Md5};
use serde::Serialize;
use sha1::Sha1;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tauri::ipc::Channel;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChartDifficulty {
    pub course: String, // Easy | Normal | Hard | Oni | Edit | Tower | Dan | (raw)
    pub level: f64,     // TJA LEVEL, may be fractional (e.g. 10.6)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScannedSong {
    pub rel_path: String,
    pub unique_id: Option<String>,
    pub title: Option<String>,
    pub tja_md5s: Vec<String>,
    pub difficulties: Vec<ChartDifficulty>,
    pub side: Option<String>, // Ex (Spicy tower) | Normal (Sweet) | (raw), from SIDE:
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScannedGenre {
    pub rel_path: String,
    pub title: Option<String>,
    pub box_def_sha1: Option<String>,
    pub preimage_sha1: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub base_exists: bool,
    pub songs: Vec<ScannedSong>,
    pub genres: Vec<ScannedGenre>,
}

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ScanEvent {
    #[serde(rename_all = "camelCase")]
    Progress {
        scanned_dirs: usize,
        songs_found: usize,
        batch: Vec<ScannedSong>,
    },
}

/// UTF-8 (with optional BOM) first, Shift_JIS fallback — same heuristic the game uses
/// for most text files.
pub(crate) fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::SHIFT_JIS.decode(bytes).0.to_string(),
    }
}

fn normalize_course(raw: &str) -> String {
    match raw.trim().to_lowercase().as_str() {
        "0" | "easy" => "Easy".to_string(),
        "1" | "normal" | "futsuu" => "Normal".to_string(),
        "2" | "hard" | "muzukashii" => "Hard".to_string(),
        "3" | "oni" => "Oni".to_string(),
        "4" | "edit" | "ura" | "uraoni" | "ura oni" | "ura-oni" => "Edit".to_string(),
        "5" | "tower" => "Tower".to_string(),
        "6" | "dan" | "dan-i" => "Dan".to_string(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

/// Returns the value of a `KEY:` line (case-insensitive), stripped of an inline `//`
/// comment. TJA keywords/values are ASCII, so this parses raw bytes and covers the whole
/// file (course headers can sit far past the metadata block).
fn directive_value(line: &[u8], key: &[u8]) -> Option<String> {
    let mut start = 0;
    while start < line.len() && matches!(line[start], b' ' | b'\t' | b'\r' | 0xEF | 0xBB | 0xBF) {
        start += 1;
    }
    let rest = &line[start..];
    if rest.len() < key.len() || !rest[..key.len()].eq_ignore_ascii_case(key) {
        return None;
    }
    let mut value = &rest[key.len()..];
    if let Some(pos) = value.windows(2).position(|w| w == b"//") {
        value = &value[..pos];
    }
    Some(String::from_utf8_lossy(value).trim().to_string())
}

fn normalize_side(raw: &str) -> String {
    match raw.trim().to_lowercase().as_str() {
        "2" | "ex" | "ura" => "Ex".to_string(),
        "1" | "normal" | "omote" => "Normal".to_string(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

/// The SIDE: of a tower chart — "Ex" is the Spicy side, otherwise Sweet.
fn parse_side(bytes: &[u8]) -> Option<String> {
    for line in bytes.split(|&b| b == b'\n') {
        if let Some(value) = directive_value(line, b"SIDE:") {
            if !value.is_empty() {
                return Some(normalize_side(&value));
            }
        }
    }
    None
}

/// Parses the per-course LEVEL values from a .tja. When a LEVEL appears before any
/// COURSE header, it belongs to the default Oni course.
fn parse_difficulties(bytes: &[u8]) -> Vec<ChartDifficulty> {
    let mut result: Vec<ChartDifficulty> = Vec::new();
    let mut current = "Oni".to_string();
    for line in bytes.split(|&b| b == b'\n') {
        if let Some(value) = directive_value(line, b"COURSE:") {
            current = normalize_course(&value);
        } else if let Some(value) = directive_value(line, b"LEVEL:") {
            if let Ok(level) = value.parse::<f64>() {
                if let Some(existing) = result.iter_mut().find(|d| d.course == current) {
                    existing.level = level;
                } else {
                    result.push(ChartDifficulty { course: current.clone(), level });
                }
            }
        }
    }
    result
}

pub(crate) fn extract_title(content: &str, prefix: &str) -> Option<String> {
    for line in content.lines().take(200) {
        let line = line.trim_start_matches('\u{feff}').trim();
        if let Some(rest) = line.strip_prefix(prefix) {
            let title = rest.trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }
    None
}

/// Git blob SHA-1 ("blob <len>\0" + content) so local files can be compared against
/// the GitHub trees API of the soundtrack repository.
fn git_blob_sha1(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()).as_bytes());
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub(crate) fn parse_unique_id(bytes: &[u8]) -> Option<String> {
    let text = decode_text(bytes);
    // Some uniqueID.json files in the wild contain stray control characters
    let cleaned: String = text.chars().filter(|c| !c.is_control()).collect();
    let value: serde_json::Value = serde_json::from_str(cleaned.trim()).ok()?;
    value.get("id")?.as_str().map(|s| s.to_string())
}

// The emit sink is a closure so the walker stays decoupled from Tauri's Channel and
// can be exercised in unit tests.
struct WalkCtx<'a> {
    base: &'a Path,
    songs: Vec<ScannedSong>,
    genres: Vec<ScannedGenre>,
    scanned_dirs: usize,
    pending: Vec<ScannedSong>,
    last_emit: Instant,
    emit: &'a mut dyn FnMut(ScanEvent),
}

fn maybe_emit(ctx: &mut WalkCtx, force: bool) {
    let due = ctx.pending.len() >= 20 || ctx.last_emit.elapsed().as_millis() >= 150;
    if !force && !due {
        return;
    }
    (ctx.emit)(ScanEvent::Progress {
        scanned_dirs: ctx.scanned_dirs,
        songs_found: ctx.songs.len(),
        batch: std::mem::take(&mut ctx.pending),
    });
    ctx.last_emit = Instant::now();
}

fn walk_dir(dir: &Path, ctx: &mut WalkCtx) {
    ctx.scanned_dirs += 1;
    maybe_emit(ctx, false);

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    let mut subdirs: Vec<PathBuf> = Vec::new();
    let mut tja_files: Vec<PathBuf> = Vec::new();
    let mut has_box_def = false;
    let mut has_preimage = false;

    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            subdirs.push(entry.path());
        } else if file_type.is_file() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.ends_with(".tja") {
                tja_files.push(entry.path());
            } else if name == "box.def" {
                has_box_def = true;
            } else if name == "default.png" {
                has_preimage = true;
            }
        }
    }

    let rel_path = dir
        .strip_prefix(ctx.base)
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();

    // The library root is never a song, whatever it holds: a loose .tja dropped
    // directly into Songs/ would otherwise end the walk immediately and hide every
    // song below it.
    if !tja_files.is_empty() && !rel_path.is_empty() {
        // A folder holding a .tja is one song. Its subfolders (replay data, extra
        // assets like "Adulation"'s folder) belong to the song, so treat it as a leaf
        // and do NOT descend into them, otherwise they surface as phantom genres.
        tja_files.sort();
        let mut tja_md5s = Vec::with_capacity(tja_files.len());
        let mut title = None;
        let mut difficulties = Vec::new();
        let mut side = None;
        for (index, tja) in tja_files.iter().enumerate() {
            if let Ok(bytes) = std::fs::read(tja) {
                let mut hasher = Md5::new();
                hasher.update(&bytes);
                tja_md5s.push(hex::encode(hasher.finalize()));
                if index == 0 {
                    let head = &bytes[..bytes.len().min(64 * 1024)];
                    title = extract_title(&decode_text(head), "TITLE:");
                    difficulties = parse_difficulties(&bytes);
                    side = parse_side(&bytes);
                }
            }
        }
        let unique_id = std::fs::read(dir.join("uniqueID.json"))
            .ok()
            .and_then(|bytes| parse_unique_id(&bytes));

        let song = ScannedSong {
            rel_path: rel_path.clone(),
            unique_id,
            title,
            tja_md5s,
            difficulties,
            side,
        };
        ctx.songs.push(song.clone());
        ctx.pending.push(song);
        maybe_emit(ctx, false);
        return;
    }

    if !rel_path.is_empty() {
        let mut box_def_sha1 = None;
        let mut title = None;
        if has_box_def {
            if let Ok(bytes) = std::fs::read(dir.join("box.def")) {
                box_def_sha1 = Some(git_blob_sha1(&bytes));
                title = extract_title(&decode_text(&bytes), "#TITLE:");
            }
        }
        let preimage_sha1 = if has_preimage {
            std::fs::read(dir.join("default.png"))
                .ok()
                .map(|bytes| git_blob_sha1(&bytes))
        } else {
            None
        };
        ctx.genres.push(ScannedGenre {
            rel_path,
            title,
            box_def_sha1,
            preimage_sha1,
        });
    }

    // Only genre folders descend further; song folders returned above.
    for sub in subdirs {
        walk_dir(&sub, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_blob_sha1_matches_known_vectors() {
        // `git hash-object` of empty content and of "hello\n"
        assert_eq!(git_blob_sha1(b""), "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
        assert_eq!(git_blob_sha1(b"hello\n"), "ce013625030ba8dba906f756967f9e9ca394464a");
    }

    #[test]
    fn extracts_tja_and_box_def_titles() {
        assert_eq!(
            extract_title("// comment\nTITLE:Test Song\nSUBTITLE:--x", "TITLE:"),
            Some("Test Song".to_string())
        );
        assert_eq!(
            extract_title("#TITLE:J-Pop\n#GENRE:01\n", "#TITLE:"),
            Some("J-Pop".to_string())
        );
        assert_eq!(extract_title("no title here", "TITLE:"), None);
    }

    #[test]
    fn decodes_utf8_bom_and_shift_jis() {
        let with_bom = [0xEF, 0xBB, 0xBF, b'T', b'I', b'T', b'L', b'E', b':', b'A'];
        assert_eq!(decode_text(&with_bom), "TITLE:A");
        // "タイトル" in Shift_JIS
        let sjis = [0x83, 0x5E, 0x83, 0x43, 0x83, 0x67, 0x83, 0x8B];
        assert_eq!(decode_text(&sjis), "タイトル");
    }

    #[test]
    fn parses_unique_id_with_control_chars() {
        assert_eq!(parse_unique_id(b"{\"id\": \"abc123\"}"), Some("abc123".to_string()));
        assert_eq!(
            parse_unique_id(b"\xEF\xBB\xBF{\"id\": \"abc123\"}\x00"),
            Some("abc123".to_string())
        );
        assert_eq!(parse_unique_id(b"not json"), None);
    }

    fn write(path: &std::path::Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn parses_named_and_numeric_courses() {
        let tja = "TITLE:X\nCOURSE:Oni\nLEVEL:9\n#START\n#END\nCOURSE:2\nLEVEL:6 // comment\n";
        let diffs = parse_difficulties(tja.as_bytes());
        assert_eq!(diffs.len(), 2);
        assert!(diffs.iter().any(|d| d.course == "Oni" && d.level == 9.0));
        assert!(diffs.iter().any(|d| d.course == "Hard" && d.level == 6.0));
    }

    #[test]
    fn level_before_course_defaults_to_oni() {
        let diffs = parse_difficulties(b"TITLE:X\nLEVEL:8\n");
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].course, "Oni");
        assert_eq!(diffs[0].level, 8.0);
    }

    #[test]
    fn parses_fractional_levels() {
        let diffs = parse_difficulties(b"COURSE:Oni\nLEVEL:10.6\n");
        assert_eq!(diffs.len(), 1);
        assert!((diffs[0].level - 10.6).abs() < 1e-9);
    }

    #[test]
    fn parses_tower_side() {
        assert_eq!(parse_side(b"TITLE:T\nSIDE:Ex\nCOURSE:Tower\n").as_deref(), Some("Ex"));
        assert_eq!(parse_side(b"SIDE:2\n").as_deref(), Some("Ex"));
        assert_eq!(parse_side(b"SIDE:1\n").as_deref(), Some("Normal"));
        assert_eq!(parse_side(b"TITLE:no side\n"), None);
    }

    #[test]
    fn a_loose_tja_at_the_library_root_does_not_hide_the_songs() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();

        // A chart dropped straight into Songs/ used to end the walk right there
        write(&base.join("Stray.tja"), "TITLE:Stray\n");
        write(&base.join("01 Pop/box.def"), "#TITLE:Pop");
        write(&base.join("01 Pop/Song A/song.tja"), "TITLE:A\n");
        write(&base.join("02 Anime/Song B/song.tja"), "TITLE:B\n");

        let (songs, genres) = collect_tree(base, &mut |_| {});

        let mut paths: Vec<&str> = songs.iter().map(|s| s.rel_path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec!["01 Pop/Song A", "02 Anime/Song B"], "songs below the root must still be found");
        // The root itself is never a song, so it never appears as one
        assert!(!songs.iter().any(|s| s.rel_path.is_empty()));
        let genre_paths: Vec<&str> = genres.iter().map(|g| g.rel_path.as_str()).collect();
        assert!(genre_paths.contains(&"01 Pop"));
    }

    #[test]
    fn song_with_asset_and_replay_subfolders_is_one_song_not_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();

        // A genre folder holding one song that ships extra subfolders
        write(&base.join("01 Pop/box.def"), "#TITLE:Pop");
        write(&base.join("01 Pop/Adulation/oni.tja"), "TITLE:Adulation\n");
        write(&base.join("01 Pop/Adulation/uniqueID.json"), "{\"id\":\"adu\"}");
        // Its asset + replay subfolders must NOT become their own songs/genres
        write(&base.join("01 Pop/Adulation/assets/bg.png"), "img");
        write(&base.join("01 Pop/Adulation/replay/p1.rpl"), "replay");

        let (songs, genres) = collect_tree(base, &mut |_| {});

        // Exactly one song, at the song folder itself
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].rel_path, "01 Pop/Adulation");
        assert_eq!(songs[0].unique_id.as_deref(), Some("adu"));

        // The only genre is the real one; the song's subfolders are not listed
        let genre_paths: Vec<&str> = genres.iter().map(|g| g.rel_path.as_str()).collect();
        assert_eq!(genre_paths, vec!["01 Pop"]);
        assert!(!genre_paths.iter().any(|p| p.contains("Adulation")));
    }
}

#[tauri::command]
pub async fn scan_songs(base_dir: String, on_event: Channel<ScanEvent>) -> Result<ScanResult, String> {
    tokio::task::spawn_blocking(move || {
        let base = PathBuf::from(&base_dir);
        if !base.is_dir() {
            return Ok(ScanResult {
                base_exists: false,
                songs: Vec::new(),
                genres: Vec::new(),
            });
        }

        let (songs, genres) = collect_tree(&base, &mut |event| {
            let _ = on_event.send(event);
        });

        Ok(ScanResult {
            base_exists: true,
            songs,
            genres,
        })
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?
}

/// Walks the library under `base`, streaming progress through `emit`, and returns the
/// full set of scanned songs and genre folders.
fn collect_tree(base: &Path, emit: &mut dyn FnMut(ScanEvent)) -> (Vec<ScannedSong>, Vec<ScannedGenre>) {
    let mut ctx = WalkCtx {
        base,
        songs: Vec::new(),
        genres: Vec::new(),
        scanned_dirs: 0,
        pending: Vec::new(),
        last_emit: Instant::now(),
        emit,
    };
    walk_dir(base, &mut ctx);
    maybe_emit(&mut ctx, true);
    (ctx.songs, ctx.genres)
}
