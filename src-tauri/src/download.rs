// Downloads a file to disk, replacing the upload plugin, which accepted any URL and wrote
// to any path. Only the hosts the Hub actually downloads from are accepted, the
// destination must be inside a folder the Hub manages (see guard.rs), and progress is
// sent at most every PROGRESS_INTERVAL rather than once per network chunk, each of which
// used to cost a script evaluation in the page.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Runtime};
use tauri_plugin_http::reqwest;
use tokio::io::AsyncWriteExt;

/// Hosts a download may start from. Release assets redirect to githubusercontent hosts,
/// which reqwest follows (https only, see `client`).
const ALLOWED_HOSTS: [&str; 5] = [
    "github.com",
    "codeload.github.com",
    "raw.githubusercontent.com",
    "objects.githubusercontent.com",
    "opentaiko.github.io",
];

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Same shape the upload plugin sent, so callers did not change: `progress` is the bytes
/// received since the previous message.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPayload {
    progress: u64,
    progress_total: u64,
    total: u64,
    transfer_speed: u64,
}

fn host_allowed(url: &reqwest::Url) -> bool {
    url.scheme() == "https" && url.host_str().is_some_and(|h| ALLOWED_HOSTS.contains(&h))
}

/// One client for every download, so connections are reused instead of a new TLS
/// handshake per file.
fn client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client);
    }
    let client = reqwest::Client::builder()
        .user_agent(concat!("OpenTaiko-Hub/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 || attempt.url().scheme() != "https" {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| format!("Could not start the download client: {e}"))?;
    Ok(CLIENT.get_or_init(|| client))
}

#[tauri::command]
pub async fn download_file<R: Runtime>(
    app: AppHandle<R>,
    url: String,
    path: String,
    on_progress: Channel<ProgressPayload>,
) -> Result<(), String> {
    let parsed = reqwest::Url::parse(&url).map_err(|e| format!("Invalid URL {url}: {e}"))?;
    if !host_allowed(&parsed) {
        return Err(format!("Refusing to download from {url}"));
    }
    let dest = crate::guard::checked(&app, &path)?;

    let response = client()?
        .get(parsed)
        .send()
        .await
        .map_err(|e| format!("Download of {url} failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Download of {url} failed: HTTP {}", response.status().as_u16()));
    }
    let total = response.content_length().unwrap_or(0);

    let file = tokio::fs::File::create(&dest)
        .await
        .map_err(|e| format!("Could not create {}: {e}", dest.display()))?;
    let mut file = tokio::io::BufWriter::new(file);
    let mut response = response;

    let started = Instant::now();
    let mut last_sent = Instant::now();
    let mut received: u64 = 0;
    let mut unsent: u64 = 0;
    let send = |received: u64, unsent: u64| {
        let secs = started.elapsed().as_secs_f64();
        let _ = on_progress.send(ProgressPayload {
            progress: unsent,
            progress_total: received,
            total,
            transfer_speed: if secs > 0.0 { (received as f64 / secs) as u64 } else { 0 },
        });
    };

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Download of {url} failed: {e}"))?
    {
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Could not write {}: {e}", dest.display()))?;
        received += chunk.len() as u64;
        unsent += chunk.len() as u64;
        if last_sent.elapsed() >= PROGRESS_INTERVAL {
            send(received, unsent);
            unsent = 0;
            last_sent = Instant::now();
        }
    }
    file.flush()
        .await
        .map_err(|e| format!("Could not write {}: {e}", dest.display()))?;
    if unsent > 0 || received == 0 {
        send(received, unsent);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_listed_https_hosts_are_accepted() {
        let ok = |u: &str| host_allowed(&reqwest::Url::parse(u).unwrap());
        assert!(ok("https://github.com/OpenTaiko/OpenTaiko/releases/download/v1/a.zip"));
        assert!(ok("https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Soundtrack/main/x.tja"));
        assert!(ok("https://codeload.github.com/OpenTaiko/OpenTaiko/zip/abc"));
        assert!(!ok("http://github.com/a.zip"));
        assert!(!ok("https://github.com.evil.example/a.zip"));
        assert!(!ok("https://example.com/a.zip"));
        assert!(!ok("file:///C:/Windows/notepad.exe"));
    }
}
