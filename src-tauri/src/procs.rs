// Runs an external program (git-less source fetch uses HTTP, but building the
// experimental branch needs `dotnet publish`) and streams stdout/stderr lines to the
// frontend so build progress can be displayed live.
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
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

/// The Hub's environment, without variables that are not valid UTF-8: `std::env::vars`
/// panics on one, which a session with an old Latin-1 locale path can carry, and took
/// the Hub down on launching the game. Those variables still reach the child unchanged;
/// only what is changed here is read.
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn utf8_env() -> HashMap<String, String> {
    utf8_only(std::env::vars_os())
}

fn utf8_only(vars: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>) -> HashMap<String, String> {
    vars.filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?))).collect()
}

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
/// How long the last output of a finished build is waited for.
const OUTPUT_GRACE: Duration = Duration::from_secs(5);

/// Stops a process and everything it started.
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        // The child leads its own process group (see run_dotnet). Signalled directly: the
        // `kill` program reads "-<pid>" as a process group or as a signal depending on
        // which one is installed, and with the second only dotnet stopped while the
        // compilers it started went on writing into the build folder.
        if let Ok(group) = libc::pid_t::try_from(child.id()) {
            // SAFETY: a system call taking two integers; nothing is borrowed
            unsafe {
                libc::killpg(group, libc::SIGKILL);
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

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

/// Runtimes the game can be built for.
const RUNTIMES: [&str; 6] = ["win-x64", "win-arm64", "linux-x64", "linux-arm64", "osx-x64", "osx-arm64"];

/// The publish command, the same as the game's build scripts except for where the output
/// goes. By default the files are copied to OpenTaiko\bin\Release\net8.0\<rid>\publish,
/// 45 characters deeper than the source they came from, which alone pushed the skin
/// folders past the Windows path limit, so both output folders sit directly in the build
/// folder: `p` for the published game, `b` for the intermediate output.
///
/// Built here rather than taken from the page: dotnet runs whatever a project, a logger
/// or an imported props file says, so the page used to be able to run anything through
/// the arguments alone. No MSBuild node or compiler server is left behind to serve later
/// builds, holding the output pipes open after the build itself has ended.
fn publish_args(build_dir: &Path, rid: &str) -> Vec<String> {
    vec![
        "publish".into(),
        "OpenTaiko/OpenTaiko.csproj".into(),
        "--configuration".into(),
        "Release".into(),
        "--self-contained".into(),
        "-p:PublishSingleFile=true".into(),
        "--runtime".into(),
        rid.into(),
        "--output".into(),
        build_dir.join("p").to_string_lossy().into_owned(),
        format!("-p:OutDir={}{}", build_dir.join("b").display(), std::path::MAIN_SEPARATOR),
        "-nodeReuse:false".into(),
        "-p:UseSharedCompilation=false".into(),
    ]
}

/// Files MSBuild, NuGet and the SDK look for in the project's folder and every folder
/// above it: an `<Exec>` in a Directory.Build.targets, or a package feed in a NuGet.Config,
/// would be picked up by the build. The game's repository has none of its own.
const INHERITED_BUILD_FILES: [&str; 7] = [
    "directory.build.props",
    "directory.build.targets",
    "directory.build.rsp",
    "directory.packages.props",
    "directory.solution.props",
    "nuget.config",
    "global.json",
];

/// The first file of INHERITED_BUILD_FILES the build of `src` would read from a folder the
/// page can fill: the project's folder and the source folder, which the build reads first
/// (they were not checked at all), the build folder, and the temporary folder holding it.
/// Not above that: what the user keeps higher up (a global.json in their home folder) is
/// theirs, as for any build they run, and refusing it blocked every build.
fn inherited_build_file(src: &Path) -> Option<PathBuf> {
    let build_dir = src.parent()?;
    let folders = [src.join("OpenTaiko"), src.to_path_buf(), build_dir.to_path_buf(), build_dir.parent()?.to_path_buf()];
    folders.iter().find_map(|dir| {
        std::fs::read_dir(dir).ok()?.flatten().map(|entry| entry.path()).find(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| INHERITED_BUILD_FILES.contains(&n.to_ascii_lowercase().as_str()))
        })
    })
}

/// Lists the installed .NET SDKs, one per line, as `dotnet --list-sdks` prints them.
#[tauri::command]
pub async fn list_dotnet_sdks(
    app: tauri::AppHandle,
    job: Option<String>,
    on_output: Channel<Vec<String>>,
) -> Result<i32, String> {
    let cancel = crate::jobs::token(&app, &job);
    let _running = crate::jobs::in_flight(&app);
    run_dotnet(vec!["--list-sdks".into()], None, cancel, on_output).await
}

/// Builds the game from source for `rid`. `src_dir` is the `s` folder of a build folder
/// the Hub created and extracted the downloaded source into; the published game lands in
/// `p` beside it. Returns dotnet's exit code.
#[tauri::command]
pub async fn publish_game(
    app: tauri::AppHandle,
    src_dir: String,
    rid: String,
    job: Option<String>,
    on_output: Channel<Vec<String>>,
) -> Result<i32, String> {
    let cancel = crate::jobs::token(&app, &job);
    if !RUNTIMES.contains(&rid.as_str()) {
        return Err(format!("Refusing to build for {}", rid));
    }
    let src = crate::guard::build_source_dir(&app, &src_dir)?;
    let build_dir = src.parent().ok_or("The build folder could not be resolved")?.to_path_buf();
    if let Some(found) = inherited_build_file(&src) {
        return Err(format!("Refusing to build: {} would be read by the build", found.display()));
    }
    // Closing the window waits until a cancelled build is gone
    let _running = crate::jobs::in_flight(&app);
    run_dotnet(publish_args(&build_dir, &rid), Some(src), cancel, on_output).await
}

/// Runs `dotnet` and streams its output. Only the two commands above call it: this used
/// to accept any program and arguments, which made it a way to start anything from the page.
async fn run_dotnet(
    args: Vec<String>,
    cwd: Option<PathBuf>,
    cancel: crate::jobs::CancelToken,
    on_output: Channel<Vec<String>>,
) -> Result<i32, String> {
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
            // A group of its own, so cancelling stops the compilers it starts too
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
            let env = utf8_env();
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
                // dotnet alone used to be stopped, and the MSBuild and compiler processes
                // it had started carried on, the Hub closed or not
                kill_tree(&mut child);
                // The output readers are not waited for: a process outside the tree can
                // still hold a pipe open
                return Err(crate::jobs::CANCELLED.to_string());
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        // The last lines are worth waiting for, but not forever: a process the build left
        // running keeps the pipes open, and the build used to look stuck until it exited
        let until = Instant::now() + OUTPUT_GRACE;
        while readers.iter().any(|r| !r.is_finished()) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50));
        }
        if readers.iter().all(|r| r.is_finished()) {
            let _ = batcher.join();
        }
        Ok(status.code().unwrap_or(-1))
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?
}

#[cfg(test)]
mod env_tests {
    use super::*;

    #[test]
    fn a_variable_that_is_not_utf8_is_left_out_instead_of_panicking() {
        use std::ffi::OsString;
        #[cfg(unix)]
        let invalid = {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(vec![b'/', 0xE9, b't', b'e'])
        };
        #[cfg(windows)]
        let invalid = {
            use std::os::windows::ffi::OsStringExt;
            OsString::from_wide(&[0x2F, 0xD800, 0x74])
        };
        let vars = vec![
            (OsString::from("PATH"), OsString::from("/usr/bin")),
            (OsString::from("OLD_LOCALE_DIR"), invalid),
        ];
        let env = utf8_only(vars.into_iter());
        assert_eq!(env.len(), 1);
        assert_eq!(env.get("PATH").map(String::as_str), Some("/usr/bin"));
    }

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
        // Fifty intervals: a busy test machine can be slow to run the batching thread
        std::thread::sleep(Duration::from_millis(1000));
        tx.send("second".to_string()).unwrap();
        drop(tx);
        let batches = handle.join().unwrap();
        assert_eq!(batches, vec![vec!["first".to_string()], vec!["second".to_string()]]);
    }
}

#[cfg(test)]
mod arg_tests {
    use super::{inherited_build_file, publish_args, RUNTIMES};
    use std::path::Path;

    #[test]
    fn the_publish_writes_only_into_its_build_folder() {
        let build = Path::new("/hub/tmp/1a2b3c4d");
        let args = publish_args(build, "linux-x64");
        assert_eq!(&args[..2], ["publish", "OpenTaiko/OpenTaiko.csproj"]);
        assert!(args.windows(2).any(|w| w[0] == "--runtime" && w[1] == "linux-x64"));
        assert!(args.windows(2).any(|w| w[0] == "--output" && Path::new(&w[1]) == build.join("p")));
        assert!(args.iter().any(|a| a.starts_with("-p:OutDir=") && a.contains("1a2b3c4d")));
        assert!(args.contains(&"-nodeReuse:false".to_string()));
        assert!(args.contains(&"-p:UseSharedCompilation=false".to_string()));
    }

    #[test]
    fn a_build_file_where_the_page_can_reach_stops_the_build() {
        let tmp = tempfile::tempdir().unwrap();
        let build = tmp.path().join("tmp").join("1a2b3c4d");
        let src = build.join("s");
        std::fs::create_dir_all(src.join("OpenTaiko")).unwrap();
        assert_eq!(inherited_build_file(&src), None);

        // What the build reads first: the project's folder and the source folder
        for (dir, name) in [(src.join("OpenTaiko"), "Directory.Build.props"), (src.clone(), "NuGet.Config"), (build.clone(), "global.json")] {
            let planted = dir.join(name);
            std::fs::write(&planted, "<Project/>").unwrap();
            assert_eq!(inherited_build_file(&src), Some(planted.clone()));
            std::fs::remove_file(&planted).unwrap();
        }
        let planted = tmp.path().join("tmp").join("Directory.Build.Targets");
        std::fs::write(&planted, "<Project/>").unwrap();
        assert_eq!(inherited_build_file(&src), Some(planted.clone()));
        std::fs::remove_file(&planted).unwrap();

        // Above the Hub's temporary folder is the user's own
        std::fs::write(tmp.path().join("global.json"), "{}").unwrap();
        assert_eq!(inherited_build_file(&src), None);
    }

    #[test]
    fn only_known_runtimes_are_built() {
        assert!(RUNTIMES.contains(&"win-x64") && RUNTIMES.contains(&"osx-arm64"));
        assert!(!RUNTIMES.contains(&"linux-Unsupported"));
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
