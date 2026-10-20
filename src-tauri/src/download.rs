// Network access for the page: downloads to disk, sets of files installed into a folder,
// and cached reads of the catalogs and the GitHub API.
//
// Only the hosts the Hub actually uses are accepted, destinations must be inside a folder
// the Hub manages (see guard.rs), one HTTP client is shared so connections are reused,
// and progress is sent at most every PROGRESS_INTERVAL rather than once per network
// chunk, each of which used to cost a script evaluation in the page.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tauri::ipc::{Channel, Response};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_http::reqwest;
use tokio::io::AsyncWriteExt;

use crate::jobs::{CancelToken, CANCELLED};

/// Hosts a download may start from. Release assets redirect to ASSET_HOSTS, which reqwest
/// follows (see `client`).
const DOWNLOAD_HOSTS: [&str; 4] = ["github.com", "codeload.github.com", "raw.githubusercontent.com", "opentaiko.github.io"];

/// Hosts `fetch_cached` reads from.
const READ_HOSTS: [&str; 3] = ["api.github.com", "raw.githubusercontent.com", "opentaiko.github.io"];

/// Where GitHub sends the download of a release asset.
const ASSET_HOSTS: [&str; 2] = ["objects.githubusercontent.com", "release-assets.githubusercontent.com"];

/// Hosts that serve every GitHub account's repositories, the account being the first part
/// of the path. A download from one of them must come from OWNERS: anyone can publish a
/// file there, and what the Hub downloads is installed into games it then runs.
const GITHUB_HOSTS: [&str; 3] = ["github.com", "codeload.github.com", "raw.githubusercontent.com"];

/// The OpenTaiko organisation, and the account game builds are published from (builds.ts).
const OWNERS: [&str; 2] = ["opentaiko", "0aubsq"];

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

const DEFAULT_ATTEMPTS: u32 = 5;
const MAX_ATTEMPTS: u32 = 10;
const BASE_DELAY: Duration = Duration::from_secs(1);
const MAX_DELAY: Duration = Duration::from_secs(30);
/// A server asking for a longer pause than this is not waited for.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(120);

/// Files of one `install_files` call downloaded side by side.
const FILE_CONCURRENCY: usize = 4;

/// Transfers running at once across every call. Each call also limits its own, but thirty
/// songs started one after the other opened over a hundred connections, and GitHub then
/// refused them.
const MAX_TRANSFERS: usize = 8;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Longest wait for the next piece of a response, its headers included. Not a limit on
/// the whole transfer, which a slow connection may need longer for.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// Downloads and partial files are named after their destination with these added, and
/// nothing else in the library or an instance carries them, so leftovers from a crash can
/// be removed safely (guard.rs).
pub(crate) const PART_SUFFIX: &str = ".hubpart";
/// A file being replaced, set aside until the replacement is in place.
pub(crate) const OLD_SUFFIX: &str = ".hubold";

fn transfer_slots() -> &'static tokio::sync::Semaphore {
    static SLOTS: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    SLOTS.get_or_init(|| tokio::sync::Semaphore::new(MAX_TRANSFERS))
}

/// Same shape the upload plugin sent, so callers did not change. `progress` is the bytes
/// received since the previous message; it is negative when a retry discards a partial
/// transfer, so a running sum of it stays correct.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPayload {
    progress: i64,
    progress_total: u64,
    total: u64,
    transfer_speed: u64,
}

fn host_allowed(url: &reqwest::Url, hosts: &[&str]) -> bool {
    url.scheme() == "https" && url.host_str().is_some_and(|h| hosts.contains(&h))
}

fn is_owner(owner: &str) -> bool {
    OWNERS.contains(&owner.to_ascii_lowercase().as_str())
}

fn segments(url: &reqwest::Url) -> Vec<&str> {
    url.path_segments().map(|parts| parts.collect()).unwrap_or_default()
}

/// Commits seen as the head of a branch of one of OWNERS' repositories, as
/// "owner/repository/commit" in lower case (see `remember_branch_head`).
fn branch_heads() -> &'static Mutex<HashSet<String>> {
    static HEADS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    HEADS.get_or_init(Default::default)
}

fn head_key(owner: &str, repo: &str, commit: &str) -> String {
    format!("{}/{}/{}", owner, repo, commit).to_ascii_lowercase()
}

/// Records the commit a branch of one of OWNERS' repositories points at, from the API's
/// answer for `/repos/<owner>/<repository>/branches/<branch>`. A branch name is looked up
/// in that repository only, so the commit is one its owners published there.
fn remember_branch_head(url: &reqwest::Url, body: &[u8]) {
    let parts = segments(url);
    if url.host_str() != Some("api.github.com")
        || url.query().is_some()
        || parts.len() < 5
        || parts[0] != "repos"
        || parts[3] != "branches"
        || !is_owner(parts[1])
    {
        return;
    }
    let commit = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|json| json.pointer("/commit/sha")?.as_str().map(String::from));
    if let Some(commit) = commit.filter(|c| c.len() == 40 && is_hex(c)) {
        branch_heads().lock().unwrap().insert(head_key(parts[1], parts[2], &commit));
    }
}

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The branch the Hub reads files from by name.
const BRANCHES: [&str; 1] = ["main"];

/// Whether a git reference, `rest` being the path from the reference on, names something
/// the repository's owners published: the branch the Hub reads from, or a commit recorded
/// by `remember_branch_head`. Nothing else is accepted, however it is spelled: GitHub
/// serves every commit of a repository's forks under the repository's own name, and
/// resolves names nobody chose to publish there. `refs/pull/<n>/head` was refused, but
/// `pull/<n>/head`, anyone's pull request, passed as a branch name.
fn reference_allowed(owner: &str, repo: &str, rest: &[&str]) -> bool {
    let Some(first) = rest.first() else { return false };
    BRANCHES.contains(first)
        || (first.len() == 40 && is_hex(first) && branch_heads().lock().unwrap().contains(&head_key(owner, repo, first)))
}

/// Anything served from GITHUB_HOSTS must come from OWNERS, and from what they published.
fn source_allowed(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str().filter(|h| GITHUB_HOSTS.contains(h)) else { return true };
    let parts = segments(url);
    if parts.len() < 3 || !is_owner(parts[0]) {
        return false;
    }
    match host {
        // Release assets only: releases belong to the repository itself, unlike its archives
        "github.com" => matches!(parts[2..], ["releases", "download", _, _, ..] | ["releases", "latest", "download", _, ..]),
        "raw.githubusercontent.com" => parts.len() >= 4 && reference_allowed(parts[0], parts[1], &parts[2..]),
        "codeload.github.com" => parts.len() >= 4 && parts[2] == "zip" && reference_allowed(parts[0], parts[1], &parts[3..]),
        _ => false,
    }
}

/// Where a request may be sent on to: anywhere a download or a read may start, under the
/// same rules, or where GitHub serves release assets from.
fn redirect_allowed(url: &reqwest::Url) -> bool {
    [&DOWNLOAD_HOSTS[..], &READ_HOSTS[..], &ASSET_HOSTS[..]].iter().any(|hosts| host_allowed(url, hosts)) && source_allowed(url)
}

fn parse_url(url: &str, hosts: &[&str]) -> Result<reqwest::Url, String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("Invalid URL {url}: {e}"))?;
    if host_allowed(&parsed, hosts) {
        Ok(parsed)
    } else {
        Err(format!("Refusing to download from {url}"))
    }
}

fn parse_download_url(url: &str) -> Result<reqwest::Url, String> {
    let parsed = parse_url(url, &DOWNLOAD_HOSTS)?;
    if source_allowed(&parsed) {
        Ok(parsed)
    } else {
        Err(format!("Refusing to download from {url}: not published by OpenTaiko"))
    }
}

/// One client for every request, so connections are reused instead of a new TLS
/// handshake per file.
fn client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client);
    }
    let builder = reqwest::Client::builder();
    // The tests talk to a server on this machine, which a proxy set for the machine would
    // not reach
    #[cfg(test)]
    let builder = builder.no_proxy();
    let client = builder
        .user_agent(concat!("OpenTaiko-Hub/", env!("CARGO_PKG_VERSION")))
        // Without these a connection that died without closing (Wi-Fi gone, the PC
        // asleep) kept a download waiting forever. A timed-out attempt is retried.
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            // A stopped redirect comes back as the 3xx response, refused in fetch_once
            if attempt.previous().len() >= 10 || !redirect_allowed(attempt.url()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| format!("Could not start the download client: {e}"))?;
    Ok(CLIENT.get_or_init(|| client))
}

// ---------------------------------------------------------------------------------------
// Retries
// ---------------------------------------------------------------------------------------

/// Why one attempt failed, and whether trying again can help.
#[derive(Debug)]
enum Failure {
    /// A dropped connection, a server error, a timeout or rate limiting.
    Retry { after: Option<Duration>, message: String },
    /// Anything another attempt would repeat, such as a missing file or a full disk.
    Fatal { status: Option<u16>, message: String },
}

fn is_retryable_status(status: u16) -> bool {
    status == 408 || status == 429 || (500..600).contains(&status)
}

/// Seconds since 1970 of an HTTP date, `Sun, 06 Nov 1994 08:49:37 GMT` (the only form
/// servers send now).
fn http_date_secs(value: &str) -> Option<u64> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    let [_, day, month, year, time, "GMT"] = parts.as_slice() else { return None };
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let month = MONTHS.iter().position(|m| m == month)? as i64 + 1;
    let (day, year): (i64, i64) = (day.parse().ok()?, year.parse().ok()?);
    let hms: Vec<i64> = time.split(':').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let [h, m, s] = hms.as_slice() else { return None };
    // Days from the civil date (Howard Hinnant's algorithm)
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + h * 3600 + m * 60 + s).ok()
}

/// How long a server asks to be left alone: a number of seconds, or a date. The date
/// form used to be ignored, and retries came back before the server wanted them.
fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let value = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?.trim();
    if let Ok(secs) = value.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(Duration::from_secs(http_date_secs(value)?.saturating_sub(now)))
}

/// When each host may next be asked, after one asked to be left alone for a while.
fn host_pauses() -> &'static Mutex<std::collections::HashMap<String, Instant>> {
    static PAUSES: OnceLock<Mutex<std::collections::HashMap<String, Instant>>> = OnceLock::new();
    PAUSES.get_or_init(Default::default)
}

/// Every transfer to the host waits, not only the one that was told: the others used to
/// go on asking, each to be refused in turn.
fn pause_host(url: &reqwest::Url, pause: Duration) {
    let Some(host) = url.host_str() else { return };
    let until = Instant::now() + pause.min(MAX_RETRY_AFTER);
    let mut pauses = host_pauses().lock().unwrap_or_else(|e| e.into_inner());
    let entry = pauses.entry(host.to_string()).or_insert(until);
    *entry = (*entry).max(until);
}

/// How long the host still asks to be left alone, if it does.
fn host_pause_left(url: &reqwest::Url) -> Option<Duration> {
    let until = url.host_str().and_then(|host| host_pauses().lock().unwrap_or_else(|e| e.into_inner()).get(host).copied())?;
    Some(until.saturating_duration_since(Instant::now())).filter(|wait| !wait.is_zero())
}

/// A transfer slot, taken once the host no longer asks for a pause. Checked again with the
/// slot held: transfers that were waiting for one went straight to a host that had asked
/// for a pause meanwhile, and were each refused in turn.
async fn take_slot(url: &reqwest::Url, cancel: &CancelToken) -> Result<tokio::sync::SemaphorePermit<'static>, Failure> {
    loop {
        if let Some(wait) = host_pause_left(url) {
            unless_cancelled(tokio::time::sleep(wait), cancel).await?;
        }
        let slot = unless_cancelled(transfer_slots().acquire(), cancel)
            .await?
            .map_err(|e| Failure::Fatal { status: None, message: e.to_string() })?;
        if host_pause_left(url).is_none() {
            return Ok(slot);
        }
    }
}

fn status_failure(url: &str, response: &reqwest::Response) -> Failure {
    let status = response.status().as_u16();
    let message = format!("Download of {url} failed: HTTP {status}");
    if is_retryable_status(status) {
        Failure::Retry { after: retry_after(response.headers()), message }
    } else {
        Failure::Fatal { status: Some(status), message }
    }
}

/// Exponential, capped, with up to a quarter added at random so parallel downloads that
/// failed together do not all retry in the same instant.
fn backoff(attempt: u32, retry_after: Option<Duration>) -> Duration {
    let exponential = BASE_DELAY.saturating_mul(1u32 << (attempt - 1).min(16)).min(MAX_DELAY);
    let jitter_permille = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos() % 250).unwrap_or(0);
    let jittered = exponential + exponential * jitter_permille / 1000;
    match retry_after {
        Some(asked) => jittered.max(asked.min(MAX_RETRY_AFTER)),
        None => jittered,
    }
}

fn with_suffix(dest: &Path, suffix: &str) -> PathBuf {
    let mut name = dest.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(suffix);
    dest.with_file_name(name)
}

fn part_path(dest: &Path) -> PathBuf {
    with_suffix(dest, PART_SUFFIX)
}

/// Waits for `work`, or fails with CANCELLED the moment the operation is cancelled: a
/// server that stops answering used to keep Cancel from doing anything.
async fn unless_cancelled<T>(work: impl std::future::Future<Output = T>, cancel: &CancelToken) -> Result<T, Failure> {
    tokio::select! {
        done = work => Ok(done),
        () = cancel.cancelled() => Err(cancelled()),
    }
}

/// One attempt: streams `url` into `part`. `on_total` gets the announced size (0 when
/// the server sends none), `on_bytes` every chunk. With `hash`, and a size announced (git's
/// hash of a file starts with its size), returns the file's git blob hash, worked out as it
/// arrived: each downloaded file used to be read back from disk to be checked.
async fn fetch_once(
    url: &reqwest::Url,
    part: &Path,
    cancel: &CancelToken,
    hash: bool,
    on_total: &mut (dyn FnMut(u64) + Send),
    on_bytes: &mut (dyn FnMut(i64) + Send),
) -> Result<Option<String>, Failure> {
    let label = url.as_str();
    let request = client().map_err(|message| Failure::Fatal { status: None, message })?.get(url.clone()).send();
    let mut response = unless_cancelled(request, cancel)
        .await?
        .map_err(|e| Failure::Retry { after: None, message: format!("Download of {label} failed: {e}") })?;
    if response.status().is_redirection() {
        let to = response.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()).unwrap_or("?");
        return Err(Failure::Fatal { status: None, message: format!("Download of {label} was sent to {to}, which is refused") });
    }
    if !response.status().is_success() {
        return Err(status_failure(label, &response));
    }
    let announced = response.content_length();
    on_total(announced.unwrap_or(0));
    let mut hasher = announced.filter(|_| hash).map(|len| {
        let mut hasher = Sha1::new();
        hasher.update(format!("blob {len}\0").as_bytes());
        hasher
    });

    let io_error = |e: std::io::Error| Failure::Fatal { status: None, message: format!("Could not write {}: {e}", part.display()) };
    let mut file = tokio::io::BufWriter::new(tokio::fs::File::create(part).await.map_err(io_error)?);
    let mut received: u64 = 0;
    while let Some(chunk) = unless_cancelled(response.chunk(), cancel)
        .await?
        .map_err(|e| Failure::Retry { after: None, message: format!("Download of {label} failed: {e}") })?
    {
        file.write_all(&chunk).await.map_err(io_error)?;
        if let Some(hasher) = hasher.as_mut() {
            hasher.update(&chunk);
        }
        received += chunk.len() as u64;
        on_bytes(chunk.len() as i64);
    }
    file.flush().await.map_err(io_error)?;
    if announced.is_some_and(|len| len != received) {
        return Err(Failure::Retry { after: None, message: format!("Download of {label} was cut short") });
    }
    Ok(hasher.map(|hasher| hex::encode(hasher.finalize())))
}

/// Downloads `url` to `<dest>.part`, retrying what can be retried. The partial file is
/// removed on failure; on success it is left for the caller to move into place.
async fn fetch_to_part(
    url: &reqwest::Url,
    dest: &Path,
    git_sha: Option<&str>,
    attempts: u32,
    cancel: &CancelToken,
    on_total: &mut (dyn FnMut(u64) + Send),
    on_bytes: &mut (dyn FnMut(i64) + Send),
) -> Result<PathBuf, Failure> {
    let part = part_path(dest);
    let attempts = attempts.clamp(1, MAX_ATTEMPTS);
    let mut attempt = 1;
    loop {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let mut counted: i64 = 0;
        let mut count = |n: i64| {
            counted += n;
            on_bytes(n);
        };
        let fetched = async {
            // Held for this attempt only, not through the pause before a retry
            let _slot = take_slot(url, cancel).await?;
            let hashed = fetch_once(url, &part, cancel, git_sha.is_some(), on_total, &mut count).await?;
            verify(&part, git_sha, hashed.as_deref(), url).await
        }
        .await;
        match fetched {
            Ok(()) => return Ok(part),
            Err(failure) => {
                // Bytes of a discarded attempt no longer count
                if counted != 0 {
                    on_bytes(-counted);
                }
                let _ = tokio::fs::remove_file(&part).await;
                match failure {
                    Failure::Retry { after, message } if attempt < attempts => {
                        if let Some(after) = after {
                            pause_host(url, after);
                        }
                        let delay = backoff(attempt, after);
                        log::warn!("{message} (attempt {attempt}/{attempts}), retrying in {} ms", delay.as_millis());
                        if !sleep_unless_cancelled(delay, cancel).await {
                            return Err(cancelled());
                        }
                        attempt += 1;
                    }
                    other => return Err(other),
                }
            }
        }
    }
}

/// A file whose git blob hash is known is checked before it replaces anything: one cut
/// short or altered on the way is downloaded again rather than installed. `hashed` is the
/// hash worked out during the download, when the server announced the size; otherwise the
/// file is read back.
async fn verify(part: &Path, git_sha: Option<&str>, hashed: Option<&str>, url: &reqwest::Url) -> Result<(), Failure> {
    let Some(sha) = git_sha else { return Ok(()) };
    let matches = match hashed {
        Some(hashed) => hashed.eq_ignore_ascii_case(sha),
        None => has_git_sha(part, sha)
            .await
            .map_err(|e| Failure::Fatal { status: None, message: format!("Could not read {}: {e}", part.display()) })?,
    };
    if matches {
        Ok(())
    } else {
        Err(Failure::Retry { after: None, message: format!("Download of {url} does not match the expected file") })
    }
}

fn cancelled() -> Failure {
    Failure::Fatal { status: None, message: CANCELLED.to_string() }
}

/// Waits `delay`, returning early (and false) once the operation is cancelled.
async fn sleep_unless_cancelled(delay: Duration, cancel: &CancelToken) -> bool {
    unless_cancelled(tokio::time::sleep(delay), cancel).await.is_ok()
}

/// Moves each downloaded `(part, destination)` into place, all of them or none. A file
/// that cannot be replaced (on Windows, one the game or an antivirus holds open) puts back
/// those already replaced: the old files are set aside, not deleted, until every new one
/// is in place. Partial files are removed either way.
fn commit(files: &[(PathBuf, PathBuf)]) -> Result<(), String> {
    let mut placed: Vec<(&Path, Option<PathBuf>)> = Vec::new();
    let mut failure = None;
    // A file that cannot be put back (held open by an antivirus, say) keeps its .hubold
    // copy, and is named in the error rather than left for nobody to notice
    let mut not_restored = Vec::new();
    let mut restore = |undone: std::io::Result<()>, dest: &Path| {
        if let Err(e) = undone {
            log::error!("Could not put {} back after a failed update: {e}", dest.display());
            not_restored.push(dest.display().to_string());
        }
    };
    for (part, dest) in files {
        let old = dest.exists().then(|| with_suffix(dest, OLD_SUFFIX));
        if let Some(old) = &old {
            if let Err(e) = std::fs::rename(dest, old) {
                failure = Some(format!("Could not replace {}: {e}", dest.display()));
                break;
            }
            // Dated now: a rename keeps the file's own date, and the startup sweep (guard.rs)
            // took any set-aside copy of an old file for one a crash had left
            mark_set_aside(old);
        }
        if let Err(e) = std::fs::rename(part, dest) {
            if let Some(old) = &old {
                restore(std::fs::rename(old, dest), dest);
            }
            failure = Some(format!("Could not write {}: {e}", dest.display()));
            break;
        }
        placed.push((dest, old));
    }
    let Some(failure) = failure else {
        for old in placed.into_iter().filter_map(|(_, old)| old) {
            let _ = std::fs::remove_file(old);
        }
        return Ok(());
    };
    for (dest, old) in placed.into_iter().rev() {
        let undone = match old {
            Some(old) => std::fs::rename(old, dest),
            None => std::fs::remove_file(dest),
        };
        restore(undone, dest);
    }
    for (part, _) in files {
        let _ = std::fs::remove_file(part);
    }
    if not_restored.is_empty() {
        Err(failure)
    } else {
        Err(format!("{failure}. Could not put back as they were: {}", not_restored.join(", ")))
    }
}

pub(crate) fn touch(path: &Path) -> std::io::Result<()> {
    std::fs::File::options().write(true).open(path)?.set_modified(SystemTime::now())
}

fn mark_set_aside(path: &Path) {
    if let Err(e) = touch(path) {
        log::warn!("Could not date {}: {e}", path.display());
    }
}

/// True when `path` was last changed at least `age` ago.
pub(crate) fn older_than(path: &Path, age: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|elapsed| elapsed >= age)
}

/// A cached answer nothing asked for in this long is removed.
const CACHE_UNUSED: Duration = Duration::from_secs(180 * 24 * 60 * 60);

/// Removes cached answers no longer asked for (a release looked up once, the trees of
/// commits long replaced), and temporary files a crash left. Nothing used to remove any.
/// An answer in use is dated each time it is served again (see `read_cached`).
pub fn prune_cache(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let stale = if name.ends_with(".tmp") {
            older_than(&path, Duration::from_secs(24 * 60 * 60))
        } else if let Some(key) = name.strip_suffix(".json") {
            let unused = older_than(&path, CACHE_UNUSED);
            if unused {
                let _ = std::fs::remove_file(dir.join(format!("{key}.body")));
            }
            unused
        } else {
            false
        };
        if stale {
            if let Err(e) = std::fs::remove_file(&path) {
                log::warn!("Could not remove {}: {}", path.display(), e);
            }
        }
    }
}

fn failure_message(failure: Failure) -> String {
    match failure {
        Failure::Retry { message, .. } | Failure::Fatal { message, .. } => message,
    }
}

/// Sends progress at most every PROGRESS_INTERVAL. Shared by concurrent transfers.
struct Progress {
    channel: Channel<ProgressPayload>,
    total: u64,
    started: Instant,
    last_sent: Instant,
    received: i64,
    unsent: i64,
}

impl Progress {
    fn new(channel: Channel<ProgressPayload>) -> Self {
        Self { channel, total: 0, started: Instant::now(), last_sent: Instant::now(), received: 0, unsent: 0 }
    }

    fn add(&mut self, n: i64) {
        self.received += n;
        self.unsent += n;
        if self.last_sent.elapsed() >= PROGRESS_INTERVAL {
            self.flush();
        }
    }

    fn flush(&mut self) {
        let secs = self.started.elapsed().as_secs_f64();
        let received = self.received.max(0) as u64;
        let _ = self.channel.send(ProgressPayload {
            progress: self.unsent,
            progress_total: received,
            total: self.total,
            transfer_speed: if secs > 0.0 { (received as f64 / secs) as u64 } else { 0 },
        });
        self.unsent = 0;
        self.last_sent = Instant::now();
    }
}

// ---------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------

/// Downloads one file. Retries what can be retried (dropped connections, server errors,
/// rate limiting) with an increasing delay; a missing file or any other client error
/// fails at once instead of being tried again and again.
#[tauri::command]
pub async fn download_file<R: Runtime>(
    app: AppHandle<R>,
    url: String,
    path: String,
    attempts: Option<u32>,
    job: Option<String>,
    on_progress: Channel<ProgressPayload>,
) -> Result<(), String> {
    let parsed = parse_download_url(&url)?;
    let cancel = crate::jobs::token(&app, &job);
    let dest = crate::guard::checked(&app, &path)?;
    let _claim = claim(&[&dest])?;
    let progress = Mutex::new(Progress::new(on_progress));
    let mut set_total = |total: u64| progress.lock().unwrap().total = total;
    let mut add = |n: i64| progress.lock().unwrap().add(n);
    let part = fetch_to_part(&parsed, &dest, None, attempts.unwrap_or(DEFAULT_ATTEMPTS), &cancel, &mut set_total, &mut add)
        .await
        .map_err(failure_message)?;
    let _moving = crate::jobs::in_flight(&app);
    // Checked once counted in flight: a close that cancelled everything and found nothing
    // in flight is not followed by files being placed while the Hub exits
    if let Err(e) = cancel.check() {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(e);
    }
    let placed = dest.clone();
    crate::library::blocking(move || commit(&[(part, placed)])).await?;
    if is_archive(&dest) {
        downloaded_archives().lock().unwrap().push(dest);
    }
    progress.lock().unwrap().flush();
    Ok(())
}

fn is_archive(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

/// Archives `download_file` placed this session, which `take_downloaded_archive` hands out.
fn downloaded_archives() -> &'static Mutex<Vec<PathBuf>> {
    static ARCHIVES: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
    ARCHIVES.get_or_init(Default::default)
}

/// The archive at `zip_path`, if the Hub may extract it: one `download_file` placed this
/// session in a temporary folder the Hub named, which is extracted beside it. Extracting any
/// file the page could reach let page script unpack a zip it had written as a save
/// database over the game's executable.
pub(crate) fn archive_to_extract<R: Runtime>(app: &AppHandle<R>, zip_path: &str) -> Result<PathBuf, String> {
    let zip = crate::guard::checked(app, zip_path)?;
    if zip.parent().is_some_and(crate::guard::is_hub_tmp_dir) && take_downloaded_archive(&zip) {
        Ok(zip)
    } else {
        Err(format!("Refusing to extract {zip_path}: not an archive the Hub downloaded"))
    }
}

/// True, once, for an archive `download_file` placed this session. Only those are
/// extracted: any file the page could reach used to be, and the page can have a save
/// database written with whatever bytes it likes, a zip included.
fn take_downloaded_archive(path: &Path) -> bool {
    let mut archives = downloaded_archives().lock().unwrap();
    match archives.iter().position(|a| crate::guard::same_path(a, path)) {
        Some(index) => {
            archives.swap_remove(index);
            true
        }
        None => false,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallEntry {
    url: String,
    /// Where the file goes, relative to the destination folder.
    rel_path: String,
    /// The file's git blob SHA upstream, when known. A local file with the same content
    /// is left alone instead of being downloaded again.
    git_sha: Option<String>,
    /// Its size upstream, when known: a local file of another size has changed, which
    /// costs a look at its size rather than reading it whole to hash it.
    #[serde(default)]
    size: Option<u64>,
    /// A file that may not exist upstream (a genre's box.def or image). A 404 for it is
    /// reported in `absent` instead of failing the whole call.
    #[serde(default)]
    optional: bool,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InstallReport {
    downloaded: Vec<String>,
    unchanged: Vec<String>,
    absent: Vec<String>,
}

/// A file's git blob hash, worked out as it is read, 64 KiB at a time: read whole, a song
/// update held up to a dozen audio files in memory at once just to compare them.
fn git_blob_sha1_of_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", file.metadata()?.len()).as_bytes());
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// True when `path` holds the file whose git blob hash is `sha`.
async fn has_git_sha(path: &Path, sha: &str) -> std::io::Result<bool> {
    let (path, sha) = (path.to_path_buf(), sha.to_string());
    tokio::task::spawn_blocking(move || git_blob_sha1_of_file(&path).map(|hash| hash.eq_ignore_ascii_case(&sha)))
        .await
        .map_err(std::io::Error::other)?
}

async fn is_unchanged(dest: &Path, git_sha: Option<&str>, size: Option<u64>) -> bool {
    let Some(sha) = git_sha else { return false };
    if let Some(size) = size {
        if !tokio::fs::metadata(dest).await.is_ok_and(|meta| meta.len() == size) {
            return false;
        }
    }
    has_git_sha(dest, sha).await.unwrap_or(false)
}

/// Destinations being downloaded to, across every call.
fn destinations_in_flight() -> &'static Mutex<HashSet<Vec<String>>> {
    static DESTINATIONS: OnceLock<Mutex<HashSet<Vec<String>>>> = OnceLock::new();
    DESTINATIONS.get_or_init(Default::default)
}

/// Destinations a call downloads to, released when it ends. Two downloads into one file
/// shared its partial file, and the one that finished first committed what both had
/// written into it.
struct Claim(Vec<Vec<String>>);

impl Drop for Claim {
    fn drop(&mut self) {
        let mut in_flight = destinations_in_flight().lock().unwrap_or_else(|e| e.into_inner());
        for key in &self.0 {
            in_flight.remove(key);
        }
    }
}

fn claim(dests: &[&Path]) -> Result<Claim, String> {
    let keys: Vec<Vec<String>> = dests.iter().map(|dest| crate::guard::path_key(dest)).collect();
    let mut in_flight = destinations_in_flight().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(taken) = keys.iter().position(|key| in_flight.contains(key)) {
        return Err(format!("{} is already being downloaded", dests[taken].display()));
    }
    in_flight.extend(keys.iter().cloned());
    Ok(Claim(keys))
}

enum Outcome {
    Downloaded(PathBuf),
    Unchanged,
    Absent,
}

/// Downloads a set of files into `dest_root`, FILE_CONCURRENCY at a time, straight into
/// place: there is no temporary copy to make and move afterwards.
///
/// Nothing is replaced until every file has arrived. Each one is downloaded next to its
/// destination with PART_SUFFIX added, and only once all of them succeeded are they moved
/// over the old files (see `commit`), so a failed update leaves the previous version
/// intact rather than a mix of old and new files.
#[tauri::command]
pub async fn install_files<R: Runtime>(
    app: AppHandle<R>,
    dest_root: String,
    entries: Vec<InstallEntry>,
    attempts: Option<u32>,
    job: Option<String>,
    on_progress: Channel<ProgressPayload>,
) -> Result<InstallReport, String> {
    let root = crate::guard::checked(&app, &dest_root)?;
    let cancel = crate::jobs::token(&app, &job);
    tokio::fs::create_dir_all(&root)
        .await
        .map_err(|e| format!("Could not create {}: {e}", root.display()))?;

    // Everything is validated before the first request. A file listed twice is fetched
    // once: two transfers would write the same partial file.
    let mut jobs: Vec<(InstallEntry, reqwest::Url, PathBuf)> = Vec::with_capacity(entries.len());
    let mut keys: HashSet<Vec<String>> = HashSet::new();
    for entry in entries {
        let url = parse_download_url(&entry.url)?;
        let dest = crate::library::resolve_entry_inside(&root, &entry.rel_path)?;
        if keys.insert(crate::guard::path_key(&dest)) {
            jobs.push((entry, url, dest));
        }
    }
    let _claim = claim(&jobs.iter().map(|(_, _, dest)| dest.as_path()).collect::<Vec<_>>())?;

    let attempts = attempts.unwrap_or(DEFAULT_ATTEMPTS);
    let progress = Arc::new(Mutex::new(Progress::new(on_progress)));
    let permits = Arc::new(tokio::sync::Semaphore::new(FILE_CONCURRENCY));
    let mut tasks = tokio::task::JoinSet::new();
    for (index, (entry, url, dest)) in jobs.iter().enumerate() {
        let (url, dest, git_sha, size, optional) = (url.clone(), dest.clone(), entry.git_sha.clone(), entry.size, entry.optional);
        let progress = progress.clone();
        let permits = permits.clone();
        let cancel = cancel.clone();
        tasks.spawn(async move {
            let _permit = permits.acquire_owned().await.map_err(|e| e.to_string())?;
            cancel.check()?;
            if is_unchanged(&dest, git_sha.as_deref(), size).await {
                return Ok((index, Outcome::Unchanged));
            }
            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
            }
            // Sizes are not added up: callers measure against the size the catalog gives
            let mut ignore_total = |_: u64| {};
            let mut add = |n: i64| progress.lock().unwrap().add(n);
            match fetch_to_part(&url, &dest, git_sha.as_deref(), attempts, &cancel, &mut ignore_total, &mut add).await {
                Ok(part) => Ok((index, Outcome::Downloaded(part))),
                Err(Failure::Fatal { status: Some(404), .. }) if optional => Ok((index, Outcome::Absent)),
                Err(failure) => Err(failure_message(failure)),
            }
        });
    }

    let mut outcomes: Vec<Option<Outcome>> = (0..jobs.len()).map(|_| None).collect();
    let mut error: Option<String> = None;
    while let Some(joined) = tasks.join_next().await {
        match joined.map_err(|e| e.to_string()).and_then(|r| r) {
            Ok((index, outcome)) => outcomes[index] = Some(outcome),
            Err(e) => {
                if error.is_none() {
                    error = Some(e);
                    tasks.abort_all();
                }
            }
        }
    }

    if let Some(e) = error {
        // Aborted transfers cannot clean up after themselves
        for (_, _, dest) in &jobs {
            let _ = tokio::fs::remove_file(part_path(dest)).await;
        }
        return Err(e);
    }

    let mut report = InstallReport::default();
    let mut arrived = Vec::new();
    for ((entry, _, dest), outcome) in jobs.iter().zip(outcomes) {
        match outcome {
            Some(Outcome::Downloaded(part)) => {
                arrived.push((part, dest.clone()));
                report.downloaded.push(entry.rel_path.clone());
            }
            Some(Outcome::Unchanged) => report.unchanged.push(entry.rel_path.clone()),
            Some(Outcome::Absent) | None => report.absent.push(entry.rel_path.clone()),
        }
    }
    let _moving = crate::jobs::in_flight(&app);
    // As in download_file: cancelled by a close meanwhile, nothing is placed
    if let Err(e) = cancel.check() {
        for (part, _) in &arrived {
            let _ = tokio::fs::remove_file(part).await;
        }
        return Err(e);
    }
    crate::library::blocking(move || commit(&arrived)).await?;
    progress.lock().unwrap().flush();
    Ok(report)
}

#[derive(Serialize, Deserialize, Default)]
struct CacheMeta {
    etag: Option<String>,
    last_modified: Option<String>,
    /// The URL the copy is of, for an entry kept under another key (see `cache_key`).
    #[serde(default)]
    url: Option<String>,
}

/// What a cached copy is kept under. A tree read at a commit never changes, and is kept
/// once per repository, the newest asked for: kept per URL, one per upstream commit added
/// up to tens of megabytes.
fn cache_key(url: &reqwest::Url) -> Option<String> {
    let parts = segments(url);
    match parts.as_slice() {
        ["repos", owner, repo, "git", "trees", commit] if url.host_str() == Some("api.github.com") && commit.len() == 40 && is_hex(commit) => {
            Some(format!("tree:{owner}/{repo}").to_ascii_lowercase())
        }
        _ => None,
    }
}

fn cache_paths(dir: &Path, key: &str) -> (PathBuf, PathBuf) {
    let key = hex::encode(Sha1::digest(key.as_bytes()));
    (dir.join(format!("{key}.body")), dir.join(format!("{key}.json")))
}

/// Fetches a catalog, a database or a GitHub API response, keeping a copy on disk with
/// its ETag. The next request for the same URL is conditional, and a "not modified"
/// answer is served from that copy, so nothing is downloaded again. GitHub still counts
/// such a request against its hourly limit for requests without a token: a tree read at a
/// commit, which cannot change, is served from its copy without asking at all. The body
/// is returned as raw bytes.
///
/// Offline or refused, the copy kept from before is served instead, unless `current` asks
/// for GitHub's answer of now: what the head of a branch is, read from a copy days old,
/// had songs updated to files that were already out of date, reported as a success.
#[tauri::command]
pub async fn fetch_cached<R: Runtime>(app: AppHandle<R>, url: String, current: Option<bool>) -> Result<Response, String> {
    let parsed = parse_url(&url, &READ_HOSTS)?;
    let (body, from_before) = read_cached(&app, &url, parsed.clone(), current.unwrap_or(false)).await?;
    if !from_before {
        remember_branch_head(&parsed, &body);
    }
    Ok(Response::new(body))
}

/// The body, and whether it is the copy kept from before rather than GitHub's answer now.
async fn read_cached<R: Runtime>(app: &AppHandle<R>, url: &str, parsed: reqwest::Url, current: bool) -> Result<(Vec<u8>, bool), String> {
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("http");
    let immutable = cache_key(&parsed);
    let (body_path, meta_path) = cache_paths(&dir, immutable.as_deref().unwrap_or(url));

    let cached_meta: Option<CacheMeta> = match (tokio::fs::read(&meta_path).await, body_path.exists()) {
        (Ok(bytes), true) => serde_json::from_slice::<CacheMeta>(&bytes)
            .ok()
            .filter(|meta| immutable.is_none() || meta.url.as_deref() == Some(url)),
        _ => None,
    };
    if immutable.is_some() && cached_meta.is_some() {
        let _ = touch(&meta_path);
        if let Ok(body) = tokio::fs::read(&body_path).await {
            return Ok((body, false));
        }
    }

    let mut request = client()?.get(parsed);
    if let Some(meta) = &cached_meta {
        if let Some(etag) = &meta.etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        if let Some(modified) = &meta.last_modified {
            request = request.header(reqwest::header::IF_MODIFIED_SINCE, modified);
        }
    }

    // Offline, timed out, rate limited or a server error: the copy from last time is
    // better than an error page, so it is used when there is one
    let fresh = async {
        let response = request.send().await.map_err(|e| (None, format!("Request to {url} failed: {e}")))?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_MODIFIED && cached_meta.is_some() {
            return Ok(None);
        }
        if !status.is_success() {
            return Err((Some(status.as_u16()), format!("HTTP error status: {}", status.as_u16())));
        }
        let header = |name: reqwest::header::HeaderName| {
            response.headers().get(name).and_then(|v| v.to_str().ok()).map(String::from)
        };
        let meta = CacheMeta {
            etag: header(reqwest::header::ETAG),
            last_modified: header(reqwest::header::LAST_MODIFIED),
            url: Some(url.to_string()),
        };
        let body = response.bytes().await.map_err(|e| (None, format!("Request to {url} failed: {e}")))?;
        Ok(Some((meta, body.to_vec())))
    }
    .await;

    let (meta, body) = match fresh {
        Ok(Some(fresh)) => fresh,
        Ok(None) => {
            // Still in use: kept by `prune_cache`
            let _ = touch(&meta_path);
            return tokio::fs::read(&body_path)
                .await
                .map(|body| (body, false))
                .map_err(|e| format!("Could not read the cached copy of {url}: {e}"));
        }
        Err((status, message)) => {
            let fall_back = !current && status.map_or(true, |s| s == 403 || is_retryable_status(s));
            if fall_back && cached_meta.is_some() {
                if let Ok(body) = tokio::fs::read(&body_path).await {
                    log::warn!("{message}; using the cached copy");
                    return Ok((body, true));
                }
            }
            return Err(message);
        }
    };

    // Caching is best-effort: a failure here costs the next request, never this one
    if meta.etag.is_some() || meta.last_modified.is_some() || immutable.is_some() {
        let store = async {
            tokio::fs::create_dir_all(&dir).await?;
            write_replacing(&body_path, &body).await?;
            write_replacing(&meta_path, &serde_json::to_vec(&meta).unwrap_or_default()).await
        };
        if let Err(e) = store.await {
            log::warn!("Could not cache {url}: {e}");
        }
    }
    Ok((body, false))
}

/// Writes beside `path` under a name no other write uses, then renames over it: two
/// requests for the same URL used to write the same temporary file at once.
async fn write_replacing(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = with_suffix(path, &format!(".{}-{n}.tmp", std::process::id()));
    let written = async {
        tokio::fs::write(&tmp, bytes).await?;
        tokio::fs::rename(&tmp, path).await
    }
    .await;
    if written.is_err() {
        let _ = tokio::fs::remove_file(&tmp).await;
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_listed_https_hosts_are_accepted() {
        let ok = |u: &str| host_allowed(&reqwest::Url::parse(u).unwrap(), &DOWNLOAD_HOSTS);
        assert!(ok("https://github.com/OpenTaiko/OpenTaiko/releases/download/v1/a.zip"));
        assert!(ok("https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/main/x.tja"));
        assert!(ok("https://codeload.github.com/OpenTaiko/OpenTaiko/zip/abc"));
        assert!(!ok("http://github.com/a.zip"));
        assert!(!ok("https://github.com.evil.example/a.zip"));
        assert!(!ok("https://example.com/a.zip"));
        assert!(!ok("file:///C:/Windows/notepad.exe"));
        // The API is read through fetch_cached, never downloaded to disk
        assert!(!ok("https://api.github.com/repos/x/y"));
        assert!(host_allowed(&reqwest::Url::parse("https://api.github.com/repos/x/y").unwrap(), &READ_HOSTS));
    }

    #[test]
    fn downloads_come_only_from_opentaiko_repositories() {
        let ok = |u: &str| parse_download_url(u).is_ok();
        assert!(ok("https://github.com/OpenTaiko/OpenTaiko-Skins/releases/download/system-assets/a.zip"));
        assert!(ok("https://codeload.github.com/0AuBSQ/OpenTaiko/zip/main"));
        assert!(ok("https://raw.githubusercontent.com/opentaiko/OpenTaiko-Soundtrack/main/x.tja"));
        assert!(ok("https://opentaiko.github.io/hof.db3"));
        assert!(!ok("https://raw.githubusercontent.com/someone/payload/main/OpenTaiko.exe"));
        assert!(!ok("https://github.com/someone/OpenTaiko/releases/download/v1/a.zip"));
        assert!(!ok("https://codeload.github.com/OpenTaikoFork/OpenTaiko/zip/main"));
        // Signed asset links name no account, so they are only followed as redirects
        assert!(!ok("https://objects.githubusercontent.com/github-production-release-asset/1"));
    }

    // GitHub serves the commits of a repository's forks, and its pull requests, under the
    // repository's own name: checked live, a fork's pull request head came back from
    // raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/<commit>/README.md
    #[test]
    fn a_fork_commit_under_an_opentaiko_name_is_refused() {
        let ok = |u: &str| parse_download_url(u).is_ok();
        let commit = "0b6a51e1d2c3b4a5968778695a4b3c2d1e0f1a2b";
        let raw = format!("https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/{commit}/README.md");
        let zip = format!("https://codeload.github.com/0AuBSQ/OpenTaiko/zip/{commit}");
        assert!(!ok(&raw));
        assert!(!ok(&zip));
        assert!(!ok("https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/0b6a51e/README.md"));
        // Every spelling of a pull request: checked live, GitHub served the short one
        // (`pull/22/head`), which an earlier check took for a branch name
        for reference in ["refs/pull/22/head", "pull/22/head", "pull/22/merge", "refs%2Fpull%2F22%2Fhead", "PULL/22/head"] {
            assert!(!ok(&format!("https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/{reference}/README.md")), "{reference}");
            assert!(!ok(&format!("https://codeload.github.com/0AuBSQ/OpenTaiko/zip/{reference}")), "{reference}");
        }
        // Any other name the Hub does not read from: GitHub resolves more of them than a
        // list of what to refuse could keep up with
        for reference in ["refs/heads/main", "refs/tags/v1", "v0.6.0", "feature", "HEAD", "Main", "main%2F..%2Fpull"] {
            assert!(!ok(&format!("https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/{reference}/x.tja")), "{reference}");
        }
        assert!(!ok("https://github.com/OpenTaiko/OpenTaiko/archive/main.zip"));
        assert!(!ok("https://github.com/OpenTaiko/OpenTaiko/raw/main/a.exe"));

        // Once the API named it the head of one of the repository's branches, it is the
        // repository's own, and only that repository's
        let branch = reqwest::Url::parse("https://api.github.com/repos/0AuBSQ/OpenTaiko/branches/main").unwrap();
        remember_branch_head(&branch, format!(r#"{{"name":"main","commit":{{"sha":"{commit}"}}}}"#).as_bytes());
        assert!(ok(&zip));
        assert!(!ok(&raw));

        // Only a branch's answer counts: the commits endpoint resolves fork commits too
        let other = "1111111111111111111111111111111111111111";
        let commits = reqwest::Url::parse(&format!("https://api.github.com/repos/OpenTaiko/OpenTaiko/commits/{other}")).unwrap();
        remember_branch_head(&commits, format!(r#"{{"commit":{{"sha":"{other}"}}}}"#).as_bytes());
        assert!(!ok(&format!("https://codeload.github.com/OpenTaiko/OpenTaiko/zip/{other}")));
    }

    // A set-aside copy of a file last changed a year ago was taken by the startup sweep
    // for one a crash had left, while its update was still under way
    #[test]
    fn a_file_set_aside_is_dated_when_it_was_set_aside() {
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join("song.ogg.hubold");
        std::fs::write(&old, "old").unwrap();
        let year_ago = SystemTime::now() - Duration::from_secs(365 * 24 * 60 * 60);
        std::fs::File::options().write(true).open(&old).unwrap().set_modified(year_ago).unwrap();
        mark_set_aside(&old);
        let age = SystemTime::now().duration_since(std::fs::metadata(&old).unwrap().modified().unwrap()).unwrap_or_default();
        assert!(age < Duration::from_secs(60), "{age:?}");
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "old");
    }

    #[test]
    fn only_an_archive_the_hub_downloaded_into_its_own_folder_is_extracted() {
        let app = crate::guard::mock_app();
        let temp = crate::guard::TempBuildDir::new();
        let folder = temp.0.clone();
        let zip = folder.join("package.zip");
        std::fs::write(&zip, b"PK").unwrap();
        crate::guard::allow_for_test(app.handle(), &folder);
        let path = zip.to_string_lossy().into_owned();

        // In place, but not downloaded: a save database written as a zip, say
        assert!(archive_to_extract(app.handle(), &path).is_err());
        downloaded_archives().lock().unwrap().push(zip.clone());
        assert!(archive_to_extract(app.handle(), &path).is_ok());
        // Once only
        assert!(archive_to_extract(app.handle(), &path).is_err());

        // Downloaded, but into a folder that is not one of the Hub's temporary ones
        let instance = tempfile::tempdir().unwrap();
        let elsewhere = instance.path().join("Saves.zip");
        std::fs::write(&elsewhere, b"PK").unwrap();
        crate::guard::allow_for_test(app.handle(), instance.path());
        downloaded_archives().lock().unwrap().push(elsewhere.clone());
        assert!(archive_to_extract(app.handle(), &elsewhere.to_string_lossy()).is_err());
    }

    #[test]
    fn only_downloaded_archives_are_handed_out_once() {
        let tmp = tempfile::tempdir().unwrap();
        let zip = tmp.path().join("package.zip");
        std::fs::write(&zip, b"PK").unwrap();
        assert!(!take_downloaded_archive(&zip));
        downloaded_archives().lock().unwrap().push(zip.clone());
        assert!(take_downloaded_archive(&zip));
        assert!(!take_downloaded_archive(&zip));
        assert!(is_archive(Path::new("a/Package.ZIP")));
        assert!(!is_archive(Path::new("a/Saves.db3")));
    }

    #[test]
    fn redirects_stay_with_github_and_the_same_accounts() {
        let ok = |u: &str| redirect_allowed(&reqwest::Url::parse(u).unwrap());
        assert!(ok("https://release-assets.githubusercontent.com/github-production-release-asset/1"));
        assert!(ok("https://objects.githubusercontent.com/github-production-release-asset/1"));
        assert!(ok("https://api.github.com/repositories/1/releases/latest"));
        assert!(ok("https://github.com/OpenTaiko/OpenTaiko-Hub/releases/latest/download/a.zip"));
        assert!(!ok("https://github.com/someone/OpenTaiko/releases/latest/download/a.zip"));
        assert!(!ok("https://example.com/a.zip"));
        assert!(!ok("http://objects.githubusercontent.com/a"));
    }

    #[test]
    fn only_transient_statuses_are_retried() {
        for status in [408, 429, 500, 502, 503, 504] {
            assert!(is_retryable_status(status), "{status}");
        }
        for status in [400, 401, 403, 404, 410] {
            assert!(!is_retryable_status(status), "{status}");
        }
    }

    #[test]
    fn the_delay_grows_is_capped_and_honours_retry_after() {
        let first = backoff(1, None);
        assert!(first >= BASE_DELAY && first <= BASE_DELAY * 5 / 4);
        let third = backoff(3, None);
        assert!(third >= BASE_DELAY * 4 && third <= BASE_DELAY * 5);
        assert!(backoff(20, None) <= MAX_DELAY * 5 / 4);
        assert!(backoff(1, Some(Duration::from_secs(60))) >= Duration::from_secs(60));
        assert!(backoff(1, Some(Duration::from_secs(3600))) <= MAX_RETRY_AFTER + MAX_DELAY);
    }

    #[test]
    fn a_partial_file_sits_next_to_its_destination() {
        let dest = Path::new("/songs/Pop/Song/song.ogg");
        assert_eq!(part_path(dest), Path::new("/songs/Pop/Song/song.ogg.hubpart"));
    }

    #[test]
    fn retry_after_is_read_as_seconds_or_as_a_date() {
        assert_eq!(http_date_secs("Sun, 06 Nov 1994 08:49:37 GMT"), Some(784_111_777));
        assert_eq!(http_date_secs("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(http_date_secs("Tue, 29 Feb 2028 12:00:00 GMT"), Some(1_835_438_400));
        assert_eq!(http_date_secs("Sunday, 06-Nov-94 08:49:37 GMT"), None);
        assert_eq!(http_date_secs("soon"), None);

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "120".parse().unwrap());
        assert_eq!(retry_after(&headers), Some(Duration::from_secs(120)));
        // A date already past means no wait
        headers.insert(reqwest::header::RETRY_AFTER, "Sun, 06 Nov 1994 08:49:37 GMT".parse().unwrap());
        assert_eq!(retry_after(&headers), Some(Duration::ZERO));
    }

    #[test]
    fn a_paused_host_holds_back_every_transfer_to_it() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
        let url = reqwest::Url::parse("https://paused.example/a").unwrap();
        let other = reqwest::Url::parse("https://other.example/a").unwrap();
        pause_host(&url, Duration::from_millis(300));
        assert!(host_pause_left(&other).is_none());
        assert!(host_pause_left(&url).is_some());
        let token = CancelToken::default();
        runtime.block_on(async {
            let started = Instant::now();
            drop(take_slot(&url, &token).await.unwrap());
            assert!(started.elapsed() >= Duration::from_millis(250));
            assert!(host_pause_left(&url).is_none());
        });
    }

    /// A server on this machine standing in for GitHub: answers every request with
    /// `status` and `body`, and counts the requests.
    async fn serve(status: u16, body: &'static [u8]) -> (reqwest::Url, Arc<std::sync::atomic::AtomicUsize>) {
        serve_announcing(status, body, body.len()).await
    }

    /// `serve`, announcing `announced` bytes whatever the body holds.
    async fn serve_announcing(status: u16, body: &'static [u8], announced: usize) -> (reqwest::Url, Arc<std::sync::atomic::AtomicUsize>) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = reqwest::Url::parse(&format!("http://{}/file", listener.local_addr().unwrap())).unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let counted = requests.clone();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                counted.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut buffer = [0u8; 1024];
                    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut buffer).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => request.extend_from_slice(&buffer[..n]),
                        }
                    }
                    let head = format!("HTTP/1.1 {status} Test\r\nContent-Length: {announced}\r\nConnection: close\r\n\r\n");
                    let _ = socket.write_all(head.as_bytes()).await;
                    let _ = socket.write_all(body).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        (url, requests)
    }

    fn git_sha_of(bytes: &[u8]) -> String {
        let mut hasher = Sha1::new();
        hasher.update(format!("blob {}\0", bytes.len()).as_bytes());
        hasher.update(bytes);
        hex::encode(hasher.finalize())
    }

    #[test]
    fn a_download_matching_its_hash_is_kept_for_the_commit() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("song.ogg");
        let body: &'static [u8] = b"the audio of a song";
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let (url, requests) = runtime.block_on(serve(200, body));
        let sha = git_sha_of(body);
        let part = runtime.block_on(async {
            let (mut total, mut bytes) = (|_: u64| {}, |_: i64| {});
            fetch_to_part(&url, &dest, Some(&sha), 3, &CancelToken::default(), &mut total, &mut bytes).await
        });
        let part = part.unwrap_or_else(|f| panic!("{}", failure_message(f)));
        assert_eq!(std::fs::read(&part).unwrap(), body);
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(!dest.exists(), "nothing is placed before the commit");
    }

    // The hash worked out as the file arrives is the one verification uses: checked here
    // directly, since a download that matches would pass even if it were never worked out
    // (the file is then read back)
    #[test]
    fn a_file_is_hashed_as_it_arrives() {
        let tmp = tempfile::tempdir().unwrap();
        let part = tmp.path().join("song.ogg.hubpart");
        let body: &'static [u8] = b"the audio of a song, hashed on the way";
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let (url, _) = runtime.block_on(serve(200, body));
        let hashed = runtime.block_on(async {
            let (mut total, mut bytes) = (|_: u64| {}, |_: i64| {});
            fetch_once(&url, &part, &CancelToken::default(), true, &mut total, &mut bytes).await
        });
        assert_eq!(hashed.ok().flatten(), Some(git_sha_of(body)));
        assert_eq!(std::fs::read(&part).unwrap(), body);
    }

    #[test]
    fn a_download_cut_short_is_tried_again_then_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("song.ogg");
        let body: &'static [u8] = b"half a song";
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let (url, requests) = runtime.block_on(serve_announcing(200, body, body.len() + 100));
        let failure = runtime.block_on(async {
            let (mut total, mut bytes) = (|_: u64| {}, |_: i64| {});
            fetch_to_part(&url, &dest, None, 2, &CancelToken::default(), &mut total, &mut bytes).await
        });
        assert!(matches!(failure, Err(Failure::Retry { .. })), "a short file must not be kept");
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert!(!part_path(&dest).exists());
    }

    #[test]
    fn a_download_not_matching_its_hash_is_tried_again_then_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("song.ogg");
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let (url, requests) = runtime.block_on(serve(200, b"another file"));
        let failure = runtime.block_on(async {
            let (mut total, mut bytes) = (|_: u64| {}, |_: i64| {});
            fetch_to_part(&url, &dest, Some(&git_sha_of(b"the expected file")), 2, &CancelToken::default(), &mut total, &mut bytes).await
        });
        let message = failure_message(failure.expect_err("a mismatch must fail"));
        assert!(message.contains("does not match"), "{message}");
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert!(!part_path(&dest).exists(), "the partial file is removed");
    }

    #[test]
    fn a_missing_file_is_not_asked_for_again() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let (url, requests) = runtime.block_on(serve(404, b"Not Found"));
        let failure = runtime.block_on(async {
            let (mut total, mut bytes) = (|_: u64| {}, |_: i64| {});
            fetch_to_part(&url, &tmp.path().join("box.def"), None, 5, &CancelToken::default(), &mut total, &mut bytes).await
        });
        assert!(matches!(failure, Err(Failure::Fatal { status: Some(404), .. })));
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn a_tree_at_a_commit_is_kept_once_per_repository() {
        let key = |u: &str| cache_key(&reqwest::Url::parse(u).unwrap());
        let commit = "0123456789abcdef0123456789abcdef01234567";
        let other = "89abcdef0123456789abcdef0123456789abcdef";
        let tree = key(&format!("https://api.github.com/repos/OpenTaiko/OpenTaiko-Soundtrack/git/trees/{commit}?recursive=1"));
        assert_eq!(tree.as_deref(), Some("tree:opentaiko/opentaiko-soundtrack"));
        // Another commit of the same repository replaces it
        assert_eq!(key(&format!("https://api.github.com/repos/OpenTaiko/OpenTaiko-Soundtrack/git/trees/{other}?recursive=1")), tree);
        // A branch's tree changes, and is asked for again like any other answer
        assert_eq!(key("https://api.github.com/repos/OpenTaiko/OpenTaiko-Soundtrack/git/trees/main?recursive=1"), None);
        assert_eq!(key("https://api.github.com/repos/OpenTaiko/OpenTaiko-Soundtrack/branches/main"), None);
    }

    #[test]
    fn one_destination_is_downloaded_to_once_at_a_time() {
        let tmp = tempfile::tempdir().unwrap();
        let song = tmp.path().join("song.ogg");
        let first = claim(&[&song]).unwrap();
        assert!(claim(&[&tmp.path().join("other.ogg"), &song]).is_err());
        drop(first);
        assert!(claim(&[&song]).is_ok());
    }

    #[test]
    fn a_file_is_hashed_as_git_hashes_it() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("song.ogg");
        // Larger than one read, so the hash spans several
        let bytes: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&file, &bytes).unwrap();
        assert_eq!(git_blob_sha1_of_file(&file).unwrap(), crate::scan::git_blob_sha1(&bytes));
        // git hash-object of "hello\n"
        std::fs::write(&file, "hello\n").unwrap();
        assert_eq!(git_blob_sha1_of_file(&file).unwrap(), "ce013625030ba8dba906f756967f9e9ca394464a");
    }

    #[test]
    fn a_commit_replaces_every_file_and_leaves_nothing_behind() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = (tmp.path().join("a.tja"), tmp.path().join("b.ogg"));
        std::fs::write(&a, "old a").unwrap();
        std::fs::write(part_path(&a), "new a").unwrap();
        std::fs::write(part_path(&b), "new b").unwrap();
        commit(&[(part_path(&a), a.clone()), (part_path(&b), b.clone())]).unwrap();
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "new a");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "new b");
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 2);
    }

    #[test]
    fn a_commit_that_fails_midway_puts_everything_back() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b, c) = (tmp.path().join("a.tja"), tmp.path().join("b.ogg"), tmp.path().join("c.png"));
        std::fs::write(&a, "old a").unwrap();
        std::fs::write(&b, "old b").unwrap();
        std::fs::write(part_path(&a), "new a").unwrap();
        // b's download is missing, so moving it in fails after a was replaced
        std::fs::write(part_path(&c), "new c").unwrap();
        let files = [(part_path(&a), a.clone()), (part_path(&b), b.clone()), (part_path(&c), c.clone())];
        assert!(commit(&files).is_err());
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "old a");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "old b");
        assert!(!c.exists());
        // No partial or set-aside file is left
        let mut names: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        names.sort();
        assert_eq!(names, ["a.tja", "b.ogg"]);
    }

    #[test]
    fn each_url_has_its_own_cache_entry() {
        let dir = Path::new("/cache");
        let (a, _) = cache_paths(dir, "https://raw.githubusercontent.com/a.json");
        let (b, _) = cache_paths(dir, "https://raw.githubusercontent.com/b.json");
        assert_ne!(a, b);
        assert_eq!(a.parent(), Some(dir));
    }
}
