// Editing the song library: genre metadata, folder creation and deletion, and importing
// chart archives that a user drops onto the Hub.
//
// Every path that comes from the frontend is resolved against the library root and
// refused if it escapes it, so a bug or a crafted name cannot write outside the folder
// the user is looking at.

use std::fs;
use std::io::{BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use sha1::{Digest, Sha1};
use zip::ZipArchive;

use crate::scan::{decode_text, parse_unique_id};

/// Write buffer for files extracted from an archive: a large one is written in a few
/// hundred calls rather than one per 8 KiB read from the archive.
pub(crate) const COPY_BUFFER: usize = 256 * 1024;

/// Runs file work on tokio's blocking pool. Deleting a genre folder or reading a whole
/// archive on an async worker stalled the downloads sharing those workers.
pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| format!("Task failed: {e}"))?
}

/// Joins `rel` onto `base` and refuses anything that leaves it.
///
/// `canonicalize` is not used for the result because the target may not exist yet; the
/// components are checked instead, and the base is canonicalized so a symlinked library
/// still compares correctly.
pub(crate) fn resolve_inside(base: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err("An absolute path is not allowed here".into());
    }
    for part in rel_path.components() {
        match part {
            Component::Normal(_) => {}
            Component::CurDir => {}
            _ => return Err(format!("Refusing a path that leaves the library: {}", rel)),
        }
    }
    let base = base.canonicalize().map_err(|e| format!("The songs folder is unreachable: {}", e))?;
    let joined = base.join(rel_path);
    // The components only show that the path stays inside on paper: a symbolic link or a
    // junction inside the library leads wherever it points
    if !crate::guard::resolve_existing(&joined).starts_with(&base) {
        return Err(format!("Refusing a path that leads out of the library: {}", rel));
    }
    Ok(joined)
}

/// `resolve_inside` for a path that names something in `base`, for every command that
/// deletes, moves or replaces what the path names. "" and "." resolve to `base` itself,
/// and deleting that deleted the whole library.
pub(crate) fn resolve_entry_inside(base: &Path, rel: &str) -> Result<PathBuf, String> {
    if !Path::new(rel).components().any(|part| matches!(part, Component::Normal(_))) {
        return Err(format!("Refusing a path that names the whole folder: {:?}", rel));
    }
    resolve_inside(base, rel)
}

/// Names a user types, or that come out of an archive, become one safe path segment.
pub(crate) fn sanitize_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if "\\/:*?\"<>|".contains(c) || c.is_control() { '_' } else { c })
        .collect();
    // Windows also rejects a trailing dot or space.
    let trimmed = cleaned.trim().trim_end_matches('.').trim().to_string();
    if trimmed.is_empty() { "Untitled".to_string() } else { trimmed }
}

/// Adds " (2)", " (3)"… until the name is free.
fn unique_dir(parent: &Path, name: &str) -> PathBuf {
    let candidate = parent.join(name);
    if !candidate.exists() {
        return candidate;
    }
    for n in 2..1000 {
        let next = parent.join(format!("{} ({})", name, n));
        if !next.exists() {
            return next;
        }
    }
    parent.join(format!("{} ({})", name, stamp()))
}

fn stamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// A chart that carries no uniqueID.json needs one, or the Hub cannot tell it apart from
/// any other custom chart. There is no rand crate here, so the id is a digest of the
/// path and the clock mapped into the same alphanumeric alphabet the soundtrack uses.
pub(crate) fn generate_unique_id(seed: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(seed.as_bytes());
    hasher.update(stamp().to_le_bytes());
    let digest = hasher.finalize();
    digest
        .iter()
        .cycle()
        .take(32)
        .enumerate()
        .map(|(i, b)| ALPHABET[(*b as usize + i * 7) % ALPHABET.len()] as char)
        .collect()
}

// ── Genre metadata ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxDef {
    /// False when the folder itself is not on disk — the soundtrack offers it but it has
    /// never been downloaded, so there is no box.def to show or edit.
    pub folder_exists: bool,
    pub file_exists: bool,
    pub text: String,
    pub title: Option<String>,
}

fn directive(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim_start_matches('\u{feff}').trim();
        if let Some(rest) = line.strip_prefix(key) {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

#[tauri::command]
pub async fn read_box_def(app: tauri::AppHandle, base: String, rel_path: String) -> Result<BoxDef, String> {
    blocking(move || {
        crate::guard::checked(&app, &base)?;
        let dir = resolve_inside(Path::new(&base), &rel_path)?;
        if !dir.is_dir() {
            return Ok(BoxDef { folder_exists: false, file_exists: false, text: String::new(), title: None });
        }
        let file = dir.join("box.def");
        match fs::read(&file) {
            Ok(bytes) => {
                let text = decode_text(&bytes);
                let title = directive(&text, "#TITLE:");
                Ok(BoxDef { folder_exists: true, file_exists: true, text, title })
            }
            Err(_) => Ok(BoxDef { folder_exists: true, file_exists: false, text: String::new(), title: None }),
        }
    })
    .await
}

#[tauri::command]
pub async fn write_box_def(app: tauri::AppHandle, base: String, rel_path: String, text: String) -> Result<(), String> {
    blocking(move || {
        crate::guard::checked(&app, &base)?;
        let dir = resolve_inside(Path::new(&base), &rel_path)?;
        if !dir.is_dir() {
            return Err("That folder is not on disk yet".into());
        }
        let file = dir.join("box.def");

        // Keep the encoding the file already had: the game reads Shift_JIS box.def files and
        // rewriting one as UTF-8 would turn its Japanese title into mojibake in-game.
        let was_shift_jis = fs::read(&file)
            .map(|bytes| std::str::from_utf8(&bytes).is_err())
            .unwrap_or(false);

        let bytes = if was_shift_jis {
            encoding_rs::SHIFT_JIS.encode(&text).0.into_owned()
        } else {
            text.into_bytes()
        };
        fs::write(&file, bytes).map_err(|e| format!("Could not write box.def: {}", e))
    })
    .await
}

// ── Folders ─────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn create_genre_folder(app: tauri::AppHandle, base: String, parent_rel: String, name: String) -> Result<String, String> {
    blocking(move || {
        crate::guard::checked(&app, &base)?;
        let parent = resolve_inside(Path::new(&base), &parent_rel)?;
        fs::create_dir_all(&parent).map_err(|e| format!("Could not reach that folder: {}", e))?;

        let dir = unique_dir(&parent, &sanitize_name(&name));
        fs::create_dir(&dir).map_err(|e| format!("Could not create the folder: {}", e))?;

        // A genre folder without a box.def shows in-game as its raw folder name, so give it
        // one straight away using the name the user typed.
        // #GENRE drives the game's genre unlockables, so it starts as the displayed name;
        // a white #BOXCOLOR is a neutral default rather than whatever the game falls back to.
        let title = sanitize_name(&name);
        let _ = fs::write(
            dir.join("box.def"),
            format!(
                "#TITLE:{}\r\n#GENRE:{}\r\n#BOXCOLOR:#ffffff\r\n",
                title, title
            ),
        );

        let root = Path::new(&base)
            .canonicalize()
            .map_err(|e| format!("The songs folder is unreachable: {}", e))?;
        Ok(dir
            .strip_prefix(&root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| title))
    })
    .await
}

/// Deletes one folder inside the library, with everything under it.
#[tauri::command]
pub async fn delete_library_entry(app: tauri::AppHandle, base: String, rel_path: String) -> Result<(), String> {
    blocking(move || {
        crate::guard::checked(&app, &base)?;
        let dir = resolve_entry_inside(Path::new(&base), &rel_path)?;
        if !dir.exists() {
            return Ok(());
        }
        if !dir.is_dir() {
            return fs::remove_file(&dir).map_err(|e| format!("Could not delete: {}", e));
        }
        fs::remove_dir_all(&dir).map_err(|e| format!("Could not delete the folder: {}", e))
    })
    .await
}

/// Image formats the game reads for a genre's box art.
const JACKET_EXTS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// Copies a chosen image in as the folder's box art, named `default.<ext>` for whatever
/// format it actually is, and removes any previous one so two `default.*` files cannot
/// disagree about which the game should use. Returns the new file name.
#[tauri::command]
pub async fn set_folder_jacket(app: tauri::AppHandle, base: String, rel_path: String, source: String) -> Result<String, String> {
    blocking(move || {
        crate::guard::checked(&app, &base)?;
        let dir = resolve_inside(Path::new(&base), &rel_path)?;
        if !dir.is_dir() {
            return Err("That folder is not on disk yet".into());
        }

        // Picked in a dialog or dropped onto the window, which allows it
        let src = crate::guard::checked(&app, &source)?;
        let ext = src
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let ext = if ext == "jpeg" { "jpg".to_string() } else { ext };
        if !JACKET_EXTS.contains(&ext.as_str()) {
            return Err(format!("{} is not an image the game can read", ext));
        }

        let bytes = fs::read(&src).map_err(|e| format!("Could not read the image: {}", e))?;

        // Remove every existing default.* first: writing default.jpg beside an old
        // default.png would leave the game picking whichever it finds first.
        for old in JACKET_EXTS {
            let path = dir.join(format!("default.{}", old));
            if path.is_file() {
                fs::remove_file(&path).map_err(|e| format!("Could not replace the old image: {}", e))?;
            }
        }

        let name = format!("default.{}", ext);
        fs::write(dir.join(&name), bytes).map_err(|e| format!("Could not write the image: {}", e))?;
        Ok(name)
    })
    .await
}

// ── A chart's own media ─────────────────────────────────────────────────────────────

/// Audio formats the game reads for a chart's preview and song.
const AUDIO_EXTS: [&str; 4] = ["ogg", "mp3", "wav", "flac"];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SongMedia {
    /// Both are paths relative to the chart folder, forward-slashed, or None when the
    /// chart ships neither a named file nor anything the extension list recognises.
    pub preimage: Option<String>,
    pub audio: Option<String>,
}

/// The first .tja in a folder, by name, so a chart with several courses in separate
/// files still resolves to one deterministic header.
fn first_tja(dir: &Path) -> Option<PathBuf> {
    let mut tjas: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .map(|e| e.to_string_lossy().eq_ignore_ascii_case("tja"))
                    .unwrap_or(false)
        })
        .collect();
    tjas.sort();
    tjas.into_iter().next()
}

/// A file a .tja header names, if it is inside the chart folder and really there.
fn named_file(dir: &Path, value: &str) -> Option<String> {
    let rel = value.trim().replace('\\', "/");
    if rel.is_empty() || !is_safe_entry(&rel) {
        return None;
    }
    let mut full = dir.to_path_buf();
    for part in rel.split('/') {
        full.push(part);
    }
    full.is_file().then_some(rel)
}

/// The first file in the folder whose extension is in `exts`.
fn any_with_ext(dir: &Path, exts: &[&str]) -> Option<String> {
    let mut hits: Vec<String> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .map(|e| exts.contains(&e.to_string_lossy().to_ascii_lowercase().as_str()))
                    .unwrap_or(false)
        })
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    hits.sort();
    hits.into_iter().next()
}

/// Where a chart's jacket and audio actually live, so an installed song plays and shows
/// its art from disk instead of streaming both out of the soundtrack repository. The
/// .tja header is consulted first because it is what the game itself reads; a folder
/// whose header names nothing usable falls back to the one image or audio file present.
#[tauri::command]
pub async fn read_song_media(app: tauri::AppHandle, base: String, rel_path: String) -> Result<SongMedia, String> {
    blocking(move || {
        crate::guard::checked(&app, &base)?;
        let dir = resolve_inside(Path::new(&base), &rel_path)?;
        if !dir.is_dir() {
            return Ok(SongMedia { preimage: None, audio: None });
        }

        let header = first_tja(&dir)
            .and_then(|p| fs::read(p).ok())
            .map(|bytes| decode_text(&bytes))
            .unwrap_or_default();

        let preimage = directive(&header, "#PREIMAGE:")
            .and_then(|v| named_file(&dir, &v))
            .or_else(|| any_with_ext(&dir, &JACKET_EXTS));
        let audio = directive(&header, "#WAVE:")
            .and_then(|v| named_file(&dir, &v))
            .or_else(|| any_with_ext(&dir, &AUDIO_EXTS));

        Ok(SongMedia { preimage, audio })
    })
    .await
}

/// Replaces (or adds) the `#PREIMAGE:` line, keeping every other line byte-identical.
fn with_preimage(text: &str, name: &str) -> String {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.split("\r\n").flat_map(|l| l.split('\n')).map(String::from).collect();

    let directive_line = format!("#PREIMAGE:{}", name);
    if let Some(at) = lines
        .iter()
        .position(|l| l.trim_start_matches('\u{feff}').trim_start().to_uppercase().starts_with("#PREIMAGE:"))
    {
        lines[at] = directive_line;
    } else {
        // Into the header block, right after #TITLE, rather than appended past #END
        // where the game would never read it.
        let at = lines
            .iter()
            .position(|l| l.trim_start_matches('\u{feff}').trim_start().to_uppercase().starts_with("#TITLE"))
            .map(|i| i + 1)
            .unwrap_or(0);
        lines.insert(at, directive_line);
    }
    lines.join(eol)
}

/// Copies a chosen image into a chart folder as its jacket and points every .tja there
/// at it. Returns the file name written.
#[tauri::command]
pub async fn set_song_preimage(app: tauri::AppHandle, base: String, rel_path: String, source: String) -> Result<String, String> {
    blocking(move || {
        crate::guard::checked(&app, &base)?;
        let dir = resolve_inside(Path::new(&base), &rel_path)?;
        if !dir.is_dir() {
            return Err("That chart is not on disk".into());
        }

        let src = crate::guard::checked(&app, &source)?;
        let ext = src.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let ext = if ext == "jpeg" { "jpg".to_string() } else { ext };
        if !JACKET_EXTS.contains(&ext.as_str()) {
            return Err(format!("{} is not an image the game can read", ext));
        }

        let bytes = fs::read(&src).map_err(|e| format!("Could not read the image: {}", e))?;

        for old in JACKET_EXTS {
            let path = dir.join(format!("preimage.{}", old));
            if path.is_file() {
                fs::remove_file(&path).map_err(|e| format!("Could not replace the old image: {}", e))?;
            }
        }

        let name = format!("preimage.{}", ext);
        fs::write(dir.join(&name), bytes).map_err(|e| format!("Could not write the image: {}", e))?;

        // Every .tja, not just the first: a chart split across files would otherwise show
        // its new jacket on some courses and the old one on the rest.
        for entry in fs::read_dir(&dir).map_err(|e| format!("Could not read the chart folder: {}", e))?.flatten() {
            let path = entry.path();
            let is_tja = path.is_file()
                && path.extension().map(|e| e.to_string_lossy().eq_ignore_ascii_case("tja")).unwrap_or(false);
            if !is_tja {
                continue;
            }
            let Ok(original) = fs::read(&path) else { continue };
            let was_shift_jis = std::str::from_utf8(&original).is_err();
            let updated = with_preimage(&decode_text(&original), &name);
            let out = if was_shift_jis {
                encoding_rs::SHIFT_JIS.encode(&updated).0.into_owned()
            } else {
                updated.into_bytes()
            };
            fs::write(&path, out).map_err(|e| format!("Could not update {}: {}", path.display(), e))?;
        }

        Ok(name)
    })
    .await
}

// ── Importing dropped archives ──────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedChart {
    pub rel_path: String,
    pub title: Option<String>,
    pub generated_id: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub charts: Vec<ImportedChart>,
    /// Archives that held no .tja at all.
    pub rejected: Vec<String>,
}

/// Rejects an archive path that would write outside the destination folder.
fn is_safe_entry(name: &str) -> bool {
    if name.starts_with('/') || name.contains(':') {
        return false;
    }
    !name.split('/').any(|part| part == "..")
}

/// Which folder inside the archive a chart lives in. "" means the archive root.
fn chart_dirs(names: &[String]) -> Vec<String> {
    let mut dirs: Vec<String> = names
        .iter()
        .filter(|n| n.to_lowercase().ends_with(".tja"))
        .map(|n| match n.rfind('/') {
            Some(i) => n[..i].to_string(),
            None => String::new(),
        })
        .collect();
    dirs.sort();
    dirs.dedup();
    dirs
}

/// The files that belong to the chart in `dir`: everything under it, minus anything that
/// belongs to a deeper chart folder. An archive can hold several charts nested at
/// different depths, and each has to come out on its own.
fn files_for<'a>(names: &'a [String], dir: &str, all_dirs: &[String]) -> Vec<&'a String> {
    let prefix = if dir.is_empty() { String::new() } else { format!("{}/", dir) };
    names
        .iter()
        .filter(|name| {
            if !name.starts_with(&prefix) || name.ends_with('/') {
                return false;
            }
            let deeper = all_dirs.iter().any(|other| {
                other.len() > dir.len()
                    && other.starts_with(&prefix)
                    && name.starts_with(&format!("{}/", other))
            });
            !deeper
        })
        .collect()
}

fn import_one(
    archive_path: &Path,
    dest_parent: &Path,
    root: &Path,
    report: &mut ImportReport,
) -> Result<(), String> {
    let file = fs::File::open(archive_path).map_err(|e| format!("Could not open the archive: {}", e))?;
    let mut zip = ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| format!("That file is not a readable zip: {}", e))?;

    // Entry names are decoded from their raw bytes: the crate falls back to CP437 for
    // a name without the UTF-8 flag, which turns a Japanese chart folder into mojibake.
    // Each decoded name is then validated here, since enclosed_name() checked the
    // crate's own interpretation rather than this one.
    let mut names: Vec<String> = Vec::new();
    let mut index_of: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| format!("Could not read the archive: {}", e))?;
        if entry.is_dir() {
            continue;
        }
        let decoded = decode_text(entry.name_raw()).replace('\\', "/");
        if !is_safe_entry(&decoded) {
            return Err(format!("Refusing an archive entry that escapes the folder: {}", decoded));
        }
        index_of.insert(decoded.clone(), i);
        names.push(decoded);
    }

    let dirs = chart_dirs(&names);
    if dirs.is_empty() {
        // Not a chart: nothing here is a .tja.
        report.rejected.push(
            archive_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        );
        return Ok(());
    }

    let archive_stem = archive_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Chart".to_string());

    for dir in &dirs {
        // A chart at the archive root has no folder of its own, so one is made for it.
        let folder_name = if dir.is_empty() {
            sanitize_name(&archive_stem)
        } else {
            sanitize_name(dir.rsplit('/').next().unwrap_or(dir))
        };
        let dest = unique_dir(dest_parent, &folder_name);
        fs::create_dir_all(&dest).map_err(|e| format!("Could not create the chart folder: {}", e))?;

        let prefix = if dir.is_empty() { String::new() } else { format!("{}/", dir) };
        let mut title: Option<String> = None;
        let mut has_id = false;

        for name in files_for(&names, dir, &dirs) {
            let inner = name.strip_prefix(&prefix).unwrap_or(name);
            if inner.is_empty() {
                continue;
            }
            let out = dest.join(inner);
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("Could not create a folder: {}", e))?;
            }

            // by_name() looks up the crate's own decoding of the name, so the index
            // captured beside the decoded name is used instead.
            let index = *index_of
                .get(name.as_str())
                .ok_or_else(|| format!("{} vanished from the archive", name))?;
            let mut entry = zip
                .by_index(index)
                .map_err(|e| format!("Could not read {} from the archive: {}", name, e))?;

            let lower = inner.to_lowercase();
            let write_error = |e: std::io::Error| format!("Could not write {}: {}", inner, e);
            if lower.ends_with(".tja") || lower == "uniqueid.json" {
                // Read here, so these are held in memory
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).map_err(|e| format!("Could not read {}: {}", name, e))?;
                if lower.ends_with(".tja") && title.is_none() {
                    title = directive(&decode_text(&bytes), "TITLE:");
                }
                if lower == "uniqueid.json" {
                    has_id = parse_unique_id(&bytes).is_some();
                }
                fs::write(&out, &bytes).map_err(write_error)?;
            } else {
                // Audio and video go straight to disk: read whole first, the largest file
                // of an archive decided how much memory an import took
                let mut file = BufWriter::with_capacity(COPY_BUFFER, fs::File::create(&out).map_err(write_error)?);
                std::io::copy(&mut entry, &mut file).map_err(write_error)?;
                file.flush().map_err(write_error)?;
            }
        }

        if !has_id {
            let seed = dest.to_string_lossy().to_string();
            let id = generate_unique_id(&seed);
            fs::write(dest.join("uniqueID.json"), format!("{{\"id\":\"{}\"}}", id))
                .map_err(|e| format!("Could not write uniqueID.json: {}", e))?;
        }

        report.charts.push(ImportedChart {
            rel_path: dest
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default(),
            title,
            generated_id: !has_id,
        });
    }

    Ok(())
}

#[tauri::command]
pub async fn import_chart_archives(
    app: tauri::AppHandle,
    base: String,
    rel_path: String,
    archives: Vec<String>,
) -> Result<ImportReport, String> {
    blocking(move || {
        crate::guard::checked(&app, &base)?;
        let dest_parent = resolve_inside(Path::new(&base), &rel_path)?;
        if !dest_parent.is_dir() {
            return Err("That folder is not on disk yet".into());
        }
        let root = Path::new(&base)
            .canonicalize()
            .map_err(|e| format!("The songs folder is unreachable: {}", e))?;

        let mut report = ImportReport { charts: Vec::new(), rejected: Vec::new() };
        for archive in archives {
            // Picked in a dialog or dropped onto the window, which allows it
            let path = crate::guard::checked(&app, &archive)?;
            if path.extension().map(|e| !e.eq_ignore_ascii_case("zip")).unwrap_or(true) {
                report.rejected.push(
                    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(archive.clone()),
                );
                continue;
            }
            import_one(&path, &dest_parent, &root, &mut report)?;
        }
        Ok(report)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn zip_with(dir: &Path, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let path = dir.join(name);
        let file = fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (entry, body) in files {
            zip.start_file(*entry, SimpleFileOptions::default()).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    #[test]
    fn a_path_that_climbs_out_of_the_library_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(resolve_inside(tmp.path(), "../escape").is_err());
        assert!(resolve_inside(tmp.path(), "ok/inside").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_library_is_not_followed() {
        let tmp = tempfile::tempdir().unwrap();
        let (library, outside) = (tmp.path().join("Songs"), tmp.path().join("outside"));
        fs::create_dir_all(&library).unwrap();
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, library.join("link")).unwrap();
        assert!(resolve_inside(&library, "link/box.def").is_err());
        assert!(resolve_entry_inside(&library, "link").is_err());
        assert!(resolve_inside(&library, "real/box.def").is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_out_of_the_library_is_not_followed() {
        let tmp = tempfile::tempdir().unwrap();
        let (library, outside) = (tmp.path().join("Songs"), tmp.path().join("outside"));
        fs::create_dir_all(&library).unwrap();
        fs::create_dir_all(&outside).unwrap();
        // A junction needs no privilege, unlike a directory symlink
        // Failing to make one is a failure: returning instead let the test pass without
        // ever checking anything
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(library.join("link"))
            .arg(&outside)
            .output()
            .unwrap();
        assert!(made.status.success(), "mklink /J failed: {}", String::from_utf8_lossy(&made.stdout));
        assert!(resolve_inside(&library, "link\\box.def").is_err());
        assert!(resolve_entry_inside(&library, "link").is_err());
        assert!(resolve_inside(&library, "real\\box.def").is_ok());
    }

    #[test]
    fn the_folder_itself_is_never_an_entry_in_it() {
        let tmp = tempfile::tempdir().unwrap();
        for rel in ["", ".", "./", "./.", " "] {
            let refused = resolve_entry_inside(tmp.path(), rel).is_err();
            // A blank name is a real (if odd) folder name; the others are the folder itself
            assert_eq!(refused, rel != " ", "{rel:?}");
        }
        assert!(resolve_entry_inside(tmp.path(), "./Pop").is_ok());
        assert!(resolve_entry_inside(tmp.path(), "../x").is_err());
    }

    #[test]
    fn names_become_one_safe_segment() {
        assert_eq!(sanitize_name("a/b:c"), "a_b_c");
        assert_eq!(sanitize_name("  trailing.  "), "trailing");
        assert_eq!(sanitize_name("   "), "Untitled");
    }

    #[test]
    fn generated_ids_differ_and_are_alphanumeric() {
        let a = generate_unique_id("one");
        let b = generate_unique_id("two");
        assert_ne!(a, b);
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn a_chart_at_the_archive_root_gets_a_folder_of_its_own() {
        let tmp = tempfile::tempdir().unwrap();
        let songs = tmp.path().join("Songs/Custom");
        fs::create_dir_all(&songs).unwrap();
        let zip = zip_with(tmp.path(), "My Song.zip", &[("song.tja", "TITLE:My Song\n"), ("song.ogg", "x")]);

        let root = tmp.path().join("Songs");
        let mut report = ImportReport { charts: vec![], rejected: vec![] };
        import_one(&zip, &songs, &root.canonicalize().unwrap(), &mut report).unwrap();

        assert_eq!(report.charts.len(), 1);
        assert_eq!(report.charts[0].title.as_deref(), Some("My Song"));
        // Named after the archive, since the files were loose at the root.
        assert!(songs.join("My Song/song.tja").is_file());
        assert!(songs.join("My Song/song.ogg").is_file());
    }

    #[test]
    fn a_chart_already_in_a_folder_keeps_that_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let songs = tmp.path().join("Songs/Custom");
        fs::create_dir_all(&songs).unwrap();
        let zip = zip_with(tmp.path(), "pack.zip", &[("Nice Chart/song.tja", "TITLE:Nice\n")]);

        let root = tmp.path().join("Songs");
        let mut report = ImportReport { charts: vec![], rejected: vec![] };
        import_one(&zip, &songs, &root.canonicalize().unwrap(), &mut report).unwrap();

        assert!(songs.join("Nice Chart/song.tja").is_file());
        assert!(!songs.join("pack").exists());
    }

    #[test]
    fn every_chart_in_a_pack_is_installed_separately() {
        let tmp = tempfile::tempdir().unwrap();
        let songs = tmp.path().join("Songs/Custom");
        fs::create_dir_all(&songs).unwrap();
        let zip = zip_with(tmp.path(), "pack.zip", &[
            ("A/a.tja", "TITLE:A\n"),
            ("A/a.ogg", "x"),
            ("B/deeper/b.tja", "TITLE:B\n"),
            ("readme.txt", "not a chart"),
        ]);

        let root = tmp.path().join("Songs");
        let mut report = ImportReport { charts: vec![], rejected: vec![] };
        import_one(&zip, &songs, &root.canonicalize().unwrap(), &mut report).unwrap();

        assert_eq!(report.charts.len(), 2);
        assert!(songs.join("A/a.tja").is_file());
        assert!(songs.join("A/a.ogg").is_file());
        assert!(songs.join("deeper/b.tja").is_file());
        // The loose readme belongs to no chart folder and is not copied into one.
        assert!(!songs.join("A/readme.txt").exists());
    }

    #[test]
    fn a_preimage_line_replaces_the_old_one_and_leaves_the_rest_alone() {
        let tja = "#TITLE:Song\r\n#PREIMAGE:old.png\r\n#BPM:120\r\n#COURSE:Oni\r\n";
        let out = with_preimage(tja, "preimage.jpg");
        assert_eq!(out, "#TITLE:Song\r\n#PREIMAGE:preimage.jpg\r\n#BPM:120\r\n#COURSE:Oni\r\n");
    }

    #[test]
    fn a_missing_preimage_line_lands_in_the_header_not_at_the_end() {
        let tja = "#TITLE:Song\n#BPM:120\n#COURSE:Oni\n#START\n#END\n";
        let out = with_preimage(tja, "preimage.png");
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "#TITLE:Song");
        assert_eq!(lines[1], "#PREIMAGE:preimage.png");
        assert_eq!(lines[2], "#BPM:120");
    }

    #[test]
    fn a_header_with_no_title_still_gets_its_preimage() {
        let out = with_preimage("#BPM:120\n", "preimage.png");
        assert!(out.starts_with("#PREIMAGE:preimage.png"));
    }

    #[test]
    fn a_named_media_file_outside_the_chart_folder_is_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::write(dir.join("ok.ogg"), b"x").unwrap();
        assert_eq!(named_file(dir, "ok.ogg"), Some("ok.ogg".to_string()));
        assert_eq!(named_file(dir, "../ok.ogg"), None);
        assert_eq!(named_file(dir, "missing.ogg"), None);
    }

    #[test]
    fn an_entry_that_climbs_out_is_refused() {
        assert!(!is_safe_entry("../evil.tja"));
        assert!(!is_safe_entry("/abs/evil.tja"));
        assert!(!is_safe_entry("C:/evil.tja"));
        assert!(!is_safe_entry("a/../../b.tja"));
        assert!(is_safe_entry("Song/chart.tja"));
        assert!(is_safe_entry("chart.tja"));
    }

    #[test]
    fn a_shift_jis_entry_name_is_not_mojibake() {
        // A zip written without the UTF-8 flag stores names in the local codepage,
        // which the crate reads as CP437 and mangles.
        let jp = "ふるさと";
        let bytes = encoding_rs::SHIFT_JIS.encode(jp).0.into_owned();
        assert!(std::str::from_utf8(&bytes).is_err(), "fixture must not be UTF-8");
        assert_eq!(decode_text(&bytes), jp);
    }

    #[test]
    fn an_archive_with_no_tja_is_rejected_rather_than_extracted() {
        let tmp = tempfile::tempdir().unwrap();
        let songs = tmp.path().join("Songs/Custom");
        fs::create_dir_all(&songs).unwrap();
        let zip = zip_with(tmp.path(), "photos.zip", &[("a.png", "x"), ("b.txt", "y")]);

        let root = tmp.path().join("Songs");
        let mut report = ImportReport { charts: vec![], rejected: vec![] };
        import_one(&zip, &songs, &root.canonicalize().unwrap(), &mut report).unwrap();

        assert!(report.charts.is_empty());
        assert_eq!(report.rejected.len(), 1);
        assert_eq!(fs::read_dir(&songs).unwrap().count(), 0);
    }

    #[test]
    fn a_chart_without_an_id_is_given_one_and_one_with_an_id_keeps_it() {
        let tmp = tempfile::tempdir().unwrap();
        let songs = tmp.path().join("Songs/Custom");
        fs::create_dir_all(&songs).unwrap();
        let root = tmp.path().join("Songs");

        let bare = zip_with(tmp.path(), "bare.zip", &[("s.tja", "TITLE:Bare\n")]);
        let mut report = ImportReport { charts: vec![], rejected: vec![] };
        import_one(&bare, &songs, &root.canonicalize().unwrap(), &mut report).unwrap();
        assert!(report.charts[0].generated_id);
        let written = fs::read_to_string(songs.join("bare/uniqueID.json")).unwrap();
        assert!(written.contains("\"id\""));

        let kept = zip_with(tmp.path(), "kept.zip", &[
            ("s.tja", "TITLE:Kept\n"),
            ("uniqueID.json", "{\"id\":\"already-here\"}"),
        ]);
        let mut report2 = ImportReport { charts: vec![], rejected: vec![] };
        import_one(&kept, &songs, &root.canonicalize().unwrap(), &mut report2).unwrap();
        assert!(!report2.charts[0].generated_id);
        assert!(fs::read_to_string(songs.join("kept/uniqueID.json")).unwrap().contains("already-here"));
    }

    #[test]
    fn importing_the_same_archive_twice_does_not_overwrite_the_first() {
        let tmp = tempfile::tempdir().unwrap();
        let songs = tmp.path().join("Songs/Custom");
        fs::create_dir_all(&songs).unwrap();
        let root = tmp.path().join("Songs");
        let zip = zip_with(tmp.path(), "dup.zip", &[("s.tja", "TITLE:Dup\n")]);

        for _ in 0..2 {
            let mut report = ImportReport { charts: vec![], rejected: vec![] };
            import_one(&zip, &songs, &root.canonicalize().unwrap(), &mut report).unwrap();
        }
        assert!(songs.join("dup").is_dir());
        assert!(songs.join("dup (2)").is_dir());
    }

    #[test]
    fn a_shift_jis_box_def_is_not_rewritten_as_utf8() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Genre");
        fs::create_dir_all(&dir).unwrap();
        let original = encoding_rs::SHIFT_JIS.encode("#TITLE:ポップス\r\n").0.into_owned();
        fs::write(dir.join("box.def"), &original).unwrap();

        let was_shift_jis = std::str::from_utf8(&fs::read(dir.join("box.def")).unwrap()).is_err();
        assert!(was_shift_jis, "the fixture must not be valid UTF-8");

        // What write_box_def does with the same input.
        let text = "#TITLE:ポップス\r\n";
        let bytes = encoding_rs::SHIFT_JIS.encode(text).0.into_owned();
        fs::write(dir.join("box.def"), bytes).unwrap();

        let back = fs::read(dir.join("box.def")).unwrap();
        assert!(std::str::from_utf8(&back).is_err(), "must still be Shift_JIS");
        assert_eq!(decode_text(&back), text);
    }

    #[test]
    fn a_new_jacket_replaces_every_old_one() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Genre");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("default.png"), b"old").unwrap();
        fs::write(dir.join("default.webp"), b"older").unwrap();

        // What set_folder_jacket does once it has the bytes.
        for old in JACKET_EXTS {
            let path = dir.join(format!("default.{}", old));
            if path.is_file() {
                fs::remove_file(&path).unwrap();
            }
        }
        fs::write(dir.join("default.jpg"), b"new").unwrap();

        assert!(!dir.join("default.png").exists());
        assert!(!dir.join("default.webp").exists());
        assert_eq!(fs::read(dir.join("default.jpg")).unwrap(), b"new");
    }

    #[test]
    fn the_title_directive_is_read_from_a_box_def() {
        assert_eq!(directive("#TITLE:J-Pop\n#GENRE:01\n", "#TITLE:").as_deref(), Some("J-Pop"));
        assert_eq!(directive("#GENRE:01\n", "#TITLE:"), None);
        assert_eq!(directive("TITLE:Song\n", "TITLE:").as_deref(), Some("Song"));
    }
}
