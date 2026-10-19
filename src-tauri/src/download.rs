// Network access for the page: downloads to disk, sets of files installed into a folder,
// and cached reads of the catalogs and the GitHub API.
//
// Only the hosts the Hub actually uses are accepted, destinations must be inside a folder
// the Hub manages (see guard.rs), one HTTP client is shared so connections are reused,
// and progress is sent at most every PROGRESS_INTERVAL rather than once per network
// chunk, each of which used to cost a script evaluation in the page.

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

fn owner_allowed(url: &reqwest::Url) -> bool {
    let on_github = url.host_str().is_some_and(|h| GITHUB_HOSTS.contains(&h));
    !on_github
        || url
            .path_segments()
            .and_then(|mut parts| parts.next())
            .is_some_and(|owner| OWNERS.contains(&owner.to_ascii_lowercase().as_str()))
}

/// Where a request may be sent on to: anywhere a download or a read may start, under the
/// same account rule, or where GitHub serves release assets from.
fn redirect_allowed(url: &reqwest::Url) -> bool {
    [&DOWNLOAD_HOSTS[..], &READ_HOSTS[..], &ASSET_HOSTS[..]].iter().any(|hosts| host_allowed(url, hosts)) && owner_allowed(url)
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
    if owner_allowed(&parsed) {
        Ok(parsed)
    } else {
        Err(format!("Refusing to download from {url}: not an OpenTaiko repository"))
    }
}

/// One client for every request, so connections are reused instead of a new TLS
/// handshake per file.
fn client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client);
    }
    let client = reqwest::Client::builder()
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

async fn wait_for_host(url: &reqwest::Url, cancel: &CancelToken) -> Result<(), Failure> {
    let until = url.host_str().and_then(|host| host_pauses().lock().unwrap_or_else(|e| e.into_inner()).get(host).copied());
    match until.map(|until| until.saturating_duration_since(Instant::now())) {
        Some(wait) if !wait.is_zero() => unless_cancelled(tokio::time::sleep(wait), cancel).await,
        _ => Ok(()),
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
/// the server sends none), `on_bytes` every chunk.
async fn fetch_once(
    url: &reqwest::Url,
    part: &Path,
    cancel: &CancelToken,
    on_total: &mut (dyn FnMut(u64) + Send),
    on_bytes: &mut (dyn FnMut(i64) + Send),
) -> Result<u64, Failure> {
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
    on_total(response.content_length().unwrap_or(0));

    let io_error = |e: std::io::Error| Failure::Fatal { status: None, message: format!("Could not write {}: {e}", part.display()) };
    let mut file = tokio::io::BufWriter::new(tokio::fs::File::create(part).await.map_err(io_error)?);
    let mut received: u64 = 0;
    while let Some(chunk) = unless_cancelled(response.chunk(), cancel)
        .await?
        .map_err(|e| Failure::Retry { after: None, message: format!("Download of {label} failed: {e}") })?
    {
        file.write_all(&chunk).await.map_err(io_error)?;
        received += chunk.len() as u64;
        on_bytes(chunk.len() as i64);
    }
    file.flush().await.map_err(io_error)?;
    Ok(received)
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
            wait_for_host(url, cancel).await?;
            // Held for this attempt only, not through the pause before a retry
            let _slot = unless_cancelled(transfer_slots().acquire(), cancel)
                .await?
                .map_err(|e| Failure::Fatal { status: None, message: e.to_string() })?;
            fetch_once(url, &part, cancel, on_total, &mut count).await?;
            verify(&part, git_sha, url).await
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
/// short or altered on the way is downloaded again rather than installed.
async fn verify(part: &Path, git_sha: Option<&str>, url: &reqwest::Url) -> Result<(), Failure> {
    let Some(sha) = git_sha else { return Ok(()) };
    let matches = has_git_sha(part, sha)
        .await
        .map_err(|e| Failure::Fatal { status: None, message: format!("Could not read {}: {e}", part.display()) })?;
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
    for (part, dest) in files {
        let old = dest.exists().then(|| with_suffix(dest, OLD_SUFFIX));
        if let Some(old) = &old {
            if let Err(e) = std::fs::rename(dest, old) {
                failure = Some(format!("Could not replace {}: {e}", dest.display()));
                break;
            }
        }
        if let Err(e) = std::fs::rename(part, dest) {
            if let Some(old) = &old {
                let _ = std::fs::rename(old, dest);
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
        let _ = match old {
            Some(old) => std::fs::rename(old, dest),
            None => std::fs::remove_file(dest),
        };
    }
    for (part, _) in files {
        let _ = std::fs::remove_file(part);
    }
    Err(failure)
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
    let progress = Mutex::new(Progress::new(on_progress));
    let mut set_total = |total: u64| progress.lock().unwrap().total = total;
    let mut add = |n: i64| progress.lock().unwrap().add(n);
    let part = fetch_to_part(&parsed, &dest, None, attempts.unwrap_or(DEFAULT_ATTEMPTS), &cancel, &mut set_total, &mut add)
        .await
        .map_err(failure_message)?;
    let _moving = crate::jobs::in_flight(&app);
    crate::library::blocking(move || commit(&[(part, dest)])).await?;
    progress.lock().unwrap().flush();
    Ok(())
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

async fn is_unchanged(dest: &Path, git_sha: Option<&str>) -> bool {
    let Some(sha) = git_sha else { return false };
    has_git_sha(dest, sha).await.unwrap_or(false)
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

    // Everything is validated before the first request
    let mut jobs = Vec::with_capacity(entries.len());
    for entry in entries {
        let url = parse_download_url(&entry.url)?;
        let dest = crate::library::resolve_entry_inside(&root, &entry.rel_path)?;
        jobs.push((entry, url, dest));
    }

    let attempts = attempts.unwrap_or(DEFAULT_ATTEMPTS);
    let progress = Arc::new(Mutex::new(Progress::new(on_progress)));
    let permits = Arc::new(tokio::sync::Semaphore::new(FILE_CONCURRENCY));
    let mut tasks = tokio::task::JoinSet::new();
    for (index, (entry, url, dest)) in jobs.iter().enumerate() {
        let (url, dest, git_sha, optional) = (url.clone(), dest.clone(), entry.git_sha.clone(), entry.optional);
        let progress = progress.clone();
        let permits = permits.clone();
        let cancel = cancel.clone();
        tasks.spawn(async move {
            let _permit = permits.acquire_owned().await.map_err(|e| e.to_string())?;
            cancel.check()?;
            if is_unchanged(&dest, git_sha.as_deref()).await {
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
    crate::library::blocking(move || commit(&arrived)).await?;
    progress.lock().unwrap().flush();
    Ok(report)
}

#[derive(Serialize, Deserialize, Default)]
struct CacheMeta {
    etag: Option<String>,
    last_modified: Option<String>,
}

fn cache_paths(dir: &Path, url: &str) -> (PathBuf, PathBuf) {
    let key = hex::encode(Sha1::digest(url.as_bytes()));
    (dir.join(format!("{key}.body")), dir.join(format!("{key}.json")))
}

/// Fetches a catalog, a database or a GitHub API response, keeping a copy on disk with
/// its ETag. The next request for the same URL is conditional, and a "not modified"
/// answer is served from that copy: nothing is downloaded again, and GitHub does not count
/// such requests against its hourly API limit. The body is returned as raw bytes.
#[tauri::command]
pub async fn fetch_cached<R: Runtime>(app: AppHandle<R>, url: String) -> Result<Response, String> {
    let parsed = parse_url(&url, &READ_HOSTS)?;
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("http");
    let (body_path, meta_path) = cache_paths(&dir, &url);

    let cached_meta: Option<CacheMeta> = match (tokio::fs::read(&meta_path).await, body_path.exists()) {
        (Ok(bytes), true) => serde_json::from_slice(&bytes).ok(),
        _ => None,
    };

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
        };
        let body = response.bytes().await.map_err(|e| (None, format!("Request to {url} failed: {e}")))?;
        Ok(Some((meta, body.to_vec())))
    }
    .await;

    let (meta, body) = match fresh {
        Ok(Some(fresh)) => fresh,
        Ok(None) => {
            let body = tokio::fs::read(&body_path)
                .await
                .map_err(|e| format!("Could not read the cached copy of {url}: {e}"))?;
            return Ok(Response::new(body));
        }
        Err((status, message)) => {
            let fall_back = status.map_or(true, |s| s == 403 || is_retryable_status(s));
            if fall_back && cached_meta.is_some() {
                if let Ok(body) = tokio::fs::read(&body_path).await {
                    log::warn!("{message}; using the cached copy");
                    return Ok(Response::new(body));
                }
            }
            return Err(message);
        }
    };

    // Caching is best-effort: a failure here costs the next request, never this one
    if meta.etag.is_some() || meta.last_modified.is_some() {
        let store = async {
            tokio::fs::create_dir_all(&dir).await?;
            write_replacing(&body_path, &body).await?;
            write_replacing(&meta_path, &serde_json::to_vec(&meta).unwrap_or_default()).await
        };
        if let Err(e) = store.await {
            log::warn!("Could not cache {url}: {e}");
        }
    }
    Ok(Response::new(body))
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
        assert!(ok("https://codeload.github.com/0AuBSQ/OpenTaiko/zip/abc"));
        assert!(ok("https://raw.githubusercontent.com/opentaiko/OpenTaiko-Soundtrack/main/x.tja"));
        assert!(ok("https://opentaiko.github.io/hof.db3"));
        assert!(!ok("https://raw.githubusercontent.com/someone/payload/main/OpenTaiko.exe"));
        assert!(!ok("https://github.com/someone/OpenTaiko/releases/download/v1/a.zip"));
        assert!(!ok("https://codeload.github.com/OpenTaikoFork/OpenTaiko/zip/abc"));
        // Signed asset links name no account, so they are only followed as redirects
        assert!(!ok("https://objects.githubusercontent.com/github-production-release-asset/1"));
    }

    #[test]
    fn redirects_stay_with_github_and_the_same_accounts() {
        let ok = |u: &str| redirect_allowed(&reqwest::Url::parse(u).unwrap());
        assert!(ok("https://release-assets.githubusercontent.com/github-production-release-asset/1"));
        assert!(ok("https://objects.githubusercontent.com/github-production-release-asset/1"));
        assert!(ok("https://api.github.com/repositories/1/releases/latest"));
        assert!(ok("https://github.com/OpenTaiko/OpenTaiko-Hub/releases/latest"));
        assert!(!ok("https://github.com/someone/OpenTaiko/releases/latest"));
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
        let token = CancelToken::default();
        runtime.block_on(async {
            let started = Instant::now();
            wait_for_host(&other, &token).await.unwrap();
            assert!(started.elapsed() < Duration::from_millis(100));
            wait_for_host(&url, &token).await.unwrap();
            assert!(started.elapsed() >= Duration::from_millis(250));
        });
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
