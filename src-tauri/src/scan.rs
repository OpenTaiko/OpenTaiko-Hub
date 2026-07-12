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
pub struct ScannedSong {
    pub rel_path: String,
    pub unique_id: Option<String>,
    pub title: Option<String>,
    pub tja_md5s: Vec<String>,
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
fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::SHIFT_JIS.decode(bytes).0.to_string(),
    }
}

fn extract_title(content: &str, prefix: &str) -> Option<String> {
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

fn parse_unique_id(bytes: &[u8]) -> Option<String> {
    let text = decode_text(bytes);
    // Some uniqueID.json files in the wild contain stray control characters
    let cleaned: String = text.chars().filter(|c| !c.is_control()).collect();
    let value: serde_json::Value = serde_json::from_str(cleaned.trim()).ok()?;
    value.get("id")?.as_str().map(|s| s.to_string())
}

struct WalkCtx<'a> {
    base: &'a Path,
    songs: Vec<ScannedSong>,
    genres: Vec<ScannedGenre>,
    scanned_dirs: usize,
    pending: Vec<ScannedSong>,
    last_emit: Instant,
    channel: &'a Channel<ScanEvent>,
}

fn maybe_emit(ctx: &mut WalkCtx, force: bool) {
    let due = ctx.pending.len() >= 20 || ctx.last_emit.elapsed().as_millis() >= 150;
    if !force && !due {
        return;
    }
    let _ = ctx.channel.send(ScanEvent::Progress {
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

    if !tja_files.is_empty() {
        tja_files.sort();
        let mut tja_md5s = Vec::with_capacity(tja_files.len());
        let mut title = None;
        for (index, tja) in tja_files.iter().enumerate() {
            if let Ok(bytes) = std::fs::read(tja) {
                let mut hasher = Md5::new();
                hasher.update(&bytes);
                tja_md5s.push(hex::encode(hasher.finalize()));
                if index == 0 {
                    let head = &bytes[..bytes.len().min(64 * 1024)];
                    title = extract_title(&decode_text(head), "TITLE:");
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
        };
        ctx.songs.push(song.clone());
        ctx.pending.push(song);
        maybe_emit(ctx, false);
    } else if !rel_path.is_empty() {
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

        let mut ctx = WalkCtx {
            base: &base,
            songs: Vec::new(),
            genres: Vec::new(),
            scanned_dirs: 0,
            pending: Vec::new(),
            last_emit: Instant::now(),
            channel: &on_event,
        };
        walk_dir(&base, &mut ctx);
        maybe_emit(&mut ctx, true);

        Ok(ScanResult {
            base_exists: true,
            songs: ctx.songs,
            genres: ctx.genres,
        })
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?
}
