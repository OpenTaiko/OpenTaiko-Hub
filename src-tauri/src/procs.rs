// Runs an external program (git-less source fetch uses HTTP, but building the
// experimental branch needs `dotnet publish`) and streams stdout/stderr lines to the
// frontend so build progress can be displayed live.
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
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
    on_output: Channel<String>,
) -> Result<i32, String> {
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

        let stdout_thread = child.stdout.take().map(|stream| {
            let channel = on_output.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    let _ = channel.send(line);
                }
            })
        });
        let stderr_thread = child.stderr.take().map(|stream| {
            let channel = on_output.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    let _ = channel.send(line);
                }
            })
        });

        let status = child
            .wait()
            .map_err(|e| format!("Failed to wait for {}: {}", program, e))?;
        if let Some(t) = stdout_thread {
            let _ = t.join();
        }
        if let Some(t) = stderr_thread {
            let _ = t.join();
        }
        Ok(status.code().unwrap_or(-1))
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?
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
