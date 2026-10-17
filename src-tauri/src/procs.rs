// Runs an external program (git-less source fetch uses HTTP, but building the
// experimental branch needs `dotnet publish`) and streams stdout/stderr lines to the
// frontend so build progress can be displayed live.
use std::collections::HashMap;
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
        // macOS: the official installer, and Homebrew on Apple Silicon
        "/usr/local/share/dotnet".to_string(),
        "/opt/homebrew/bin".to_string(),
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

/// A change to a child process's environment: `None` removes the variable.
pub(crate) type EnvChange = (String, Option<String>);

/// Undoes what an AppImage sets up for the Hub itself. The AppImage runtime and its
/// GTK hook point library, schema and data paths into the mounted image (and force
/// GDK_BACKEND=x11 for the Hub's window); a program the Hub starts would load those
/// instead of the system's own. Path lists keep their entries outside the image.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn appimage_cleanup(env: &HashMap<String, String>) -> Vec<EnvChange> {
    let mut changes = Vec::new();
    let Some(appdir) = env.get("APPDIR").filter(|dir| !dir.is_empty() && env.contains_key("APPIMAGE")) else {
        return changes;
    };
    for (key, value) in env {
        if matches!(key.as_str(), "APPDIR" | "APPIMAGE" | "ARGV0" | "OWD" | "GDK_BACKEND") {
            changes.push((key.clone(), None));
        } else if value.contains(appdir.as_str()) {
            let kept: Vec<&str> = value
                .split(':')
                .filter(|part| !part.is_empty() && !part.starts_with(appdir.as_str()))
                .collect();
            changes.push((key.clone(), (!kept.is_empty()).then(|| kept.join(":"))));
        }
    }
    changes.sort();
    changes
}

/// The environment the game gets on Linux: the same as from the official launcher
/// (special/linux_installer_standalone.bash in the game repository), which finds the
/// libraries shipped next to the game through LD_LIBRARY_PATH and runs ANGLE on Vulkan.
/// The game's bundled libraries carry no $ORIGIN path, so without this the system's
/// copies were loaded instead, or none at all.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn game_env(env: &HashMap<String, String>, game_dir: &str) -> Vec<EnvChange> {
    let mut changes = appimage_cleanup(env);
    let inherited_ld = match changes.iter().find(|(key, _)| key == "LD_LIBRARY_PATH") {
        Some((_, value)) => value.clone(),
        None => env.get("LD_LIBRARY_PATH").cloned(),
    };
    changes.retain(|(key, _)| key != "LD_LIBRARY_PATH");
    let ld = match inherited_ld.filter(|v| !v.is_empty()) {
        Some(rest) => format!("{game_dir}:{rest}"),
        None => game_dir.to_string(),
    };
    changes.push(("SDL_VIDEODRIVER".into(), None));
    changes.push(("StoreValue".into(), None));
    changes.push(("LD_LIBRARY_PATH".into(), Some(ld)));
    changes.push(("ANGLE_DEFAULT_PLATFORM".into(), Some("vulkan".into())));
    changes
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn apply_env(cmd: &mut Command, changes: Vec<EnvChange>) {
    for (key, value) in changes {
        match value {
            Some(value) => cmd.env(key, value),
            None => cmd.env_remove(key),
        };
    }
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
            let env: HashMap<String, String> = std::env::vars().collect();
            let cleanup = appimage_cleanup(&env);
            // PATH as the AppImage cleanup leaves it, before adding the dotnet locations
            let base = match cleanup.iter().find(|(key, _)| key == "PATH") {
                Some((_, value)) => value.clone().unwrap_or_default(),
                None => env.get("PATH").cloned().unwrap_or_default(),
            };
            apply_env(&mut cmd, cleanup);
            cmd.env("PATH", augmented_tool_path(&base, env.get("HOME").map(String::as_str)));
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
mod env_tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn value<'a>(changes: &'a [EnvChange], key: &str) -> Option<&'a Option<String>> {
        changes.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    #[test]
    fn outside_an_appimage_nothing_is_cleaned() {
        assert!(appimage_cleanup(&env(&[("PATH", "/usr/bin"), ("GDK_BACKEND", "wayland")])).is_empty());
    }

    #[test]
    fn an_appimage_keeps_only_paths_outside_the_image() {
        let changes = appimage_cleanup(&env(&[
            ("APPIMAGE", "/home/u/Hub.AppImage"),
            ("APPDIR", "/tmp/.mount_Hub"),
            ("XDG_DATA_DIRS", "/tmp/.mount_Hub/usr/share:/usr/share"),
            ("GSETTINGS_SCHEMA_DIR", "/tmp/.mount_Hub/usr/share/glib-2.0/schemas"),
            ("GDK_BACKEND", "x11"),
            ("HOME", "/home/u")
        ]));
        assert_eq!(value(&changes, "XDG_DATA_DIRS"), Some(&Some("/usr/share".into())));
        assert_eq!(value(&changes, "GSETTINGS_SCHEMA_DIR"), Some(&None));
        assert_eq!(value(&changes, "GDK_BACKEND"), Some(&None));
        assert_eq!(value(&changes, "APPDIR"), Some(&None));
        assert_eq!(value(&changes, "HOME"), None);
    }

    #[test]
    fn the_game_gets_the_official_launcher_environment() {
        let changes = game_env(&env(&[("SDL_VIDEODRIVER", "x11"), ("LD_LIBRARY_PATH", "/opt/lib")]), "/games/optk");
        assert_eq!(value(&changes, "LD_LIBRARY_PATH"), Some(&Some("/games/optk:/opt/lib".into())));
        assert_eq!(value(&changes, "SDL_VIDEODRIVER"), Some(&None));
        assert_eq!(value(&changes, "ANGLE_DEFAULT_PLATFORM"), Some(&Some("vulkan".into())));
    }

    #[test]
    fn the_game_does_not_inherit_the_appimage_libraries() {
        let changes = game_env(
            &env(&[
                ("APPIMAGE", "/home/u/Hub.AppImage"),
                ("APPDIR", "/tmp/.mount_Hub"),
                ("LD_LIBRARY_PATH", "/tmp/.mount_Hub/usr/lib")
            ]),
            "/games/optk"
        );
        assert_eq!(value(&changes, "LD_LIBRARY_PATH"), Some(&Some("/games/optk".into())));
        assert_eq!(changes.iter().filter(|(k, _)| k == "LD_LIBRARY_PATH").count(), 1);
    }
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
