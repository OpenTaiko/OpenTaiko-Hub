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

fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
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

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".part");
    dest.with_file_name(name)
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
    let mut response = client()
        .map_err(|message| Failure::Fatal { status: None, message })?
        .get(url.clone())
        .send()
        .await
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
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| Failure::Retry { after: None, message: format!("Download of {label} failed: {e}") })?
    {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
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
        let fetched = match fetch_once(url, &part, cancel, on_total, &mut count).await {
            Ok(_) => verify(&part, git_sha, url).await,
            Err(failure) => Err(failure),
        };
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
    let bytes = tokio::fs::read(part)
        .await
        .map_err(|e| Failure::Fatal { status: None, message: format!("Could not read {}: {e}", part.display()) })?;
    if crate::scan::git_blob_sha1(&bytes).eq_ignore_ascii_case(sha) {
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
    let until = Instant::now() + delay;
    while Instant::now() < until {
        if cancel.is_cancelled() {
            return false;
        }
        tokio::time::sleep((until - Instant::now()).min(Duration::from_millis(200))).await;
    }
    !cancel.is_cancelled()
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
    tokio::fs::rename(&part, &dest)
        .await
        .map_err(|e| format!("Could not write {}: {e}", dest.display()))?;
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

async fn is_unchanged(dest: &Path, git_sha: Option<&str>) -> bool {
    let Some(sha) = git_sha else { return false };
    match tokio::fs::read(dest).await {
        Ok(bytes) => crate::scan::git_blob_sha1(&bytes).eq_ignore_ascii_case(sha),
        Err(_) => false,
    }
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
/// destination as `<name>.part`, and only once all of them succeeded are they renamed
/// over the old files, so a failed update leaves the previous version intact rather than
/// a mix of old and new files.
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
    for ((entry, _, dest), outcome) in jobs.iter().zip(outcomes) {
        match outcome {
            Some(Outcome::Downloaded(part)) => {
                tokio::fs::rename(&part, dest)
                    .await
                    .map_err(|e| format!("Could not write {}: {e}", dest.display()))?;
                report.downloaded.push(entry.rel_path.clone());
            }
            Some(Outcome::Unchanged) => report.unchanged.push(entry.rel_path.clone()),
            Some(Outcome::Absent) | None => report.absent.push(entry.rel_path.clone()),
        }
    }
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
    let response = request.send().await.map_err(|e| format!("Request to {url} failed: {e}"))?;
    let status = response.status();

    if status == reqwest::StatusCode::NOT_MODIFIED && cached_meta.is_some() {
        let body = tokio::fs::read(&body_path)
            .await
            .map_err(|e| format!("Could not read the cached copy of {url}: {e}"))?;
        return Ok(Response::new(body));
    }
    if !status.is_success() {
        return Err(format!("HTTP error status: {}", status.as_u16()));
    }

    let header = |name: reqwest::header::HeaderName| {
        response.headers().get(name).and_then(|v| v.to_str().ok()).map(String::from)
    };
    let meta = CacheMeta {
        etag: header(reqwest::header::ETAG),
        last_modified: header(reqwest::header::LAST_MODIFIED),
    };
    let body = response
        .bytes()
        .await
        .map_err(|e| format!("Request to {url} failed: {e}"))?
        .to_vec();

    // Caching is best-effort: a failure here costs the next request, never this one
    if meta.etag.is_some() || meta.last_modified.is_some() {
        let store = async {
            tokio::fs::create_dir_all(&dir).await?;
            let tmp = body_path.with_extension("body.tmp");
            tokio::fs::write(&tmp, &body).await?;
            tokio::fs::rename(&tmp, &body_path).await?;
            tokio::fs::write(&meta_path, serde_json::to_vec(&meta).unwrap_or_default()).await
        };
        if let Err(e) = store.await {
            log::warn!("Could not cache {url}: {e}");
        }
    }
    Ok(Response::new(body))
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
        assert_eq!(part_path(dest), Path::new("/songs/Pop/Song/song.ogg.part"));
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
