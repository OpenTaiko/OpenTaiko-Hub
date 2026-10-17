// Runs an external program (git-less source fetch uses HTTP, but building the
// experimental branch needs `dotnet publish`) and streams stdout/stderr lines to the
// frontend so build progress can be displayed live.
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};
use tauri::ipc::Channel;

// Desktop-launched Linux apps inherit a minimal PATH that usually omits the common
// .NET SDK install locations, so `dotnet` fails to resolve even when it is installed.
// Append those locations (without disturbing the user's own PATH order).
#[cfg(unix)]
fn augmented_tool_path(base_path: &str, home: Option<&str>) -> String {
    let mut dirs: Vec<String> = base_path
        .split(':')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let mut candidates = vec![
        "/usr/local/bin".to_string(),
        "/usr/share/dotnet".to_string(),
        "/usr/lib/dotnet".to_string(),
        "/opt/dotnet".to_string(),
        "/snap/bin".to_string(),
    ];
    if let Some(home) = home {
        candidates.push(format!("{}/.dotnet", home));
    }

    for candidate in candidates {
        if !dirs.iter().any(|dir| dir == &candidate) {
            dirs.push(candidate);
        }
    }
    dirs.join(":")
}

/// How often collected output lines are sent to the page.
const OUTPUT_INTERVAL: Duration = Duration::from_millis(100);
/// A batch is sent early once it holds this many lines.
const OUTPUT_BATCH_MAX: usize = 500;

/// Collects lines from `rx` and hands them to `send` in batches: every `interval`, or as
/// soon as `max` lines are waiting. Returns once every sender is gone and the last lines
/// were sent, so a quiet stretch still delivers what arrived before it.
fn batch_lines(rx: Receiver<String>, interval: Duration, max: usize, mut send: impl FnMut(Vec<String>)) {
    let mut batch: Vec<String> = Vec::new();
    let mut last_sent = Instant::now();
    loop {
        let wait = interval.saturating_sub(last_sent.elapsed());
        let closed = match rx.recv_timeout(wait) {
            Ok(line) => {
                batch.push(line);
                false
            }
            Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => true,
        };
        if !batch.is_empty() && (closed || batch.len() >= max || last_sent.elapsed() >= interval) {
            send(std::mem::take(&mut batch));
            last_sent = Instant::now();
        } else if last_sent.elapsed() >= interval {
            last_sent = Instant::now();
        }
        if closed {
            return;
        }
    }
}

/// The only commands the page may run: listing the installed SDKs, and publishing the
/// game from source.
fn allowed_dotnet_args(args: &[String]) -> bool {
    matches!(args.first().map(String::as_str), Some("--list-sdks") if args.len() == 1)
        || args.first().map(String::as_str) == Some("publish")
}

/// Runs `dotnet` and streams its output. The program is fixed: this used to accept any
/// program and arguments, which made it a way to start anything from the page.
#[tauri::command]
pub async fn run_dotnet(
    app: tauri::AppHandle,
    args: Vec<String>,
    cwd: Option<String>,
    job: Option<String>,
    on_output: Channel<Vec<String>>,
) -> Result<i32, String> {
    let cancel = crate::jobs::token(&app, &job);
    if !allowed_dotnet_args(&args) {
        return Err(format!("Refusing to run dotnet {}", args.join(" ")));
    }
    // A publish builds whatever project sits in its folder, so it must be one the Hub
    // downloaded into its own build folder
    match &cwd {
        Some(dir) => {
            crate::guard::checked(&app, dir)?;
        }
        None if args[0] == "publish" => return Err("A build needs its source folder".into()),
        None => {}
    }
    let program = "dotnet".to_string();
    tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new(&program);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = &cwd {
            cmd.current_dir(dir);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        #[cfg(unix)]
        {
            let base = std::env::var("PATH").unwrap_or_default();
            let home = std::env::var("HOME").ok();
            cmd.env("PATH", augmented_tool_path(&base, home.as_deref()));
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to start {}: {}", program, e))?;

        // Both streams feed one batcher, which sends what arrived every
        // OUTPUT_INTERVAL. Each channel message is a script evaluation in the page, and
        // MSBuild prints thousands of lines, so one message per line kept the window busy
        // for the whole build. The batcher ends once both streams have closed.
        let (tx, rx) = mpsc::channel::<String>();
        let readers: Vec<_> = [
            child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
            child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        ]
        .into_iter()
        .flatten()
        .map(|stream| {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            })
        })
        .collect();
        drop(tx);
        let batcher = std::thread::spawn(move || {
            batch_lines(rx, OUTPUT_INTERVAL, OUTPUT_BATCH_MAX, |lines| {
                let _ = on_output.send(lines);
            })
        });

        // Polled rather than waited on, so a cancelled build can be stopped
        let status = loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|e| format!("Failed to wait for {}: {}", program, e))?
            {
                break status;
            }
            if cancel.is_cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                // The output readers are not waited for: a compiler process the build
                // started can keep the pipe open after dotnet itself is gone
                return Err(crate::jobs::CANCELLED.to_string());
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        for reader in readers {
            let _ = reader.join();
        }
        let _ = batcher.join();
        Ok(status.code().unwrap_or(-1))
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?
}

#[cfg(test)]
mod batch_tests {
    use super::batch_lines;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn lines_arrive_in_order_in_few_batches() {
        let (tx, rx) = mpsc::channel();
        for i in 0..1000 {
            tx.send(i.to_string()).unwrap();
        }
        drop(tx);
        let mut batches: Vec<Vec<String>> = Vec::new();
        batch_lines(rx, Duration::from_secs(60), 300, |b| batches.push(b));
        // Capped at 300 per batch, and the remainder is sent when the senders close
        assert_eq!(batches.iter().map(Vec::len).collect::<Vec<_>>(), vec![300, 300, 300, 100]);
        let all: Vec<String> = batches.concat();
        assert_eq!(all, (0..1000).map(|i| i.to_string()).collect::<Vec<_>>());
    }

    #[test]
    fn a_line_before_a_quiet_stretch_is_sent_without_waiting_for_more() {
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            let mut batches = Vec::new();
            batch_lines(rx, Duration::from_millis(20), 500, |b| batches.push(b));
            batches
        });
        tx.send("first".to_string()).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        tx.send("second".to_string()).unwrap();
        drop(tx);
        let batches = handle.join().unwrap();
        assert_eq!(batches, vec![vec!["first".to_string()], vec!["second".to_string()]]);
    }
}

#[cfg(test)]
mod arg_tests {
    use super::allowed_dotnet_args;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn only_the_sdk_listing_and_a_publish_are_allowed() {
        assert!(allowed_dotnet_args(&args(&["--list-sdks"])));
        assert!(allowed_dotnet_args(&args(&["publish", "OpenTaiko/OpenTaiko.csproj"])));
        assert!(!allowed_dotnet_args(&args(&["--list-sdks", "extra"])));
        assert!(!allowed_dotnet_args(&args(&["run"])));
        assert!(!allowed_dotnet_args(&args(&["exec", "payload.dll"])));
        assert!(!allowed_dotnet_args(&[]));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn appends_dotnet_locations_without_dropping_existing() {
        let result = augmented_tool_path("/usr/bin:/bin", Some("/home/user"));
        // Existing entries stay first and in order
        assert!(result.starts_with("/usr/bin:/bin"));
        // Common dotnet locations get appended
        assert!(result.split(':').any(|d| d == "/usr/share/dotnet"));
        assert!(result.split(':').any(|d| d == "/snap/bin"));
        assert!(result.split(':').any(|d| d == "/home/user/.dotnet"));
    }

    #[test]
    fn does_not_duplicate_already_present_dirs() {
        let result = augmented_tool_path("/usr/local/bin:/usr/bin", None);
        assert_eq!(result.split(':').filter(|d| *d == "/usr/local/bin").count(), 1);
        // Without HOME, no ~/.dotnet entry is added
        assert!(!result.split(':').any(|d| d.ends_with("/.dotnet")));
    }
}
