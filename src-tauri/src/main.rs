// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assetscan;
mod binver;
mod configini;
mod fsops;
mod health;
mod migrate;
mod procs;
mod scan;

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::task;
use zip::ZipArchive;

/// Stderr kept from a launched game, enough to identify a missing library or a bad
/// interpreter without letting a chatty process grow this unboundedly.
const CHILD_STDERR_CAP: usize = 8 * 1024;

/// Payload of the `game-exited` event.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct GameExit {
    code: Option<i32>,
    /// Set when the process was killed by a signal (Unix), where `code` is absent.
    signal: Option<i32>,
    /// Seconds the process survived, so the frontend can tell a failed launch from a
    /// normal quit hours later.
    ran_for_secs: u64,
    stderr: String,
}

#[tauri::command]
fn execute_external_app(os: String, path: String, app_handle: AppHandle) -> Result<(), String> {
    let full_path = if os == "Win" {
        format!("{}.exe", path)
    } else {
        path
    };

    // Extract the directory from the full path
    let dir = std::path::Path::new(&full_path)
        .parent()
        .ok_or_else(|| String::from("Failed to extract directory from path"))?;

    // Release zips are packed on a Windows CI runner, which does not store the unix
    // execute bit, so the extracted game binary is not executable. Ensure it is before
    // launching (the official Linux installer does the same).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&full_path) {
            let mut perms = meta.permissions();
            perms.set_mode(perms.mode() | 0o111);
            let _ = std::fs::set_permissions(&full_path, perms);
        }
    }

    // stderr is captured rather than inherited: a game that dies immediately (missing
    // shared library, noexec mount, wrong architecture) used to be reported as a
    // successful launch, so users experienced it as "the Hub crashed" with nothing to go on.
    let mut child = Command::new(&full_path)
        .current_dir(dir)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to execute the app: {}", e))?;

    log::info!("Launched {} (pid {})", full_path, child.id());

    // Watching in a thread also reaps the child, which previously became a zombie for as
    // long as the Hub stayed open.
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let mut stderr_tail = String::new();
        if let Some(mut stream) = child.stderr.take() {
            // Keep the first CHILD_STDERR_CAP bytes but keep draining, so the child can
            // never block writing into a full pipe.
            let mut buf = [0u8; 4096];
            let mut kept: Vec<u8> = Vec::new();
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                if kept.len() < CHILD_STDERR_CAP {
                    let room = CHILD_STDERR_CAP - kept.len();
                    kept.extend_from_slice(&buf[..n.min(room)]);
                }
            }
            stderr_tail = String::from_utf8_lossy(&kept).trim().to_string();
        }

        let status = child.wait();
        let ran_for_secs = started.elapsed().as_secs();
        let code = status.as_ref().ok().and_then(|s| s.code());
        let success = status.as_ref().map(|s| s.success()).unwrap_or(false);

        // A process killed by a signal reports no exit code, which is the normal way a
        // Linux game dies on a segfault or an abort. Carry the signal so the report can
        // name it instead of showing a blank.
        #[cfg(unix)]
        let signal = {
            use std::os::unix::process::ExitStatusExt;
            status.as_ref().ok().and_then(|s| s.signal())
        };
        #[cfg(not(unix))]
        let signal: Option<i32> = None;

        if success {
            log::info!("The game exited normally after {} s", ran_for_secs);
        } else {
            log::error!(
                "The game exited (code {:?}, signal {:?}) after {} s. stderr: {}",
                code,
                signal,
                ran_for_secs,
                if stderr_tail.is_empty() { "(empty)" } else { &stderr_tail }
            );
        }

        let _ = app_handle.emit(
            "game-exited",
            GameExit {
                code,
                signal,
                ran_for_secs,
                stderr: stderr_tail,
            },
        );
    });

    Ok(())
}

#[tauri::command]
async fn unzip_and_get_first_folder(
    zip_path: String,
    app_handle: AppHandle,
) -> Result<String, String> {
    let zip_path_clone = zip_path.clone();

    task::spawn_blocking(move || {
        let file = File::open(&zip_path_clone)
            .map_err(|err| format!("ERR: Failed to open zip file: {}", err))?;
        let mut archive = ZipArchive::new(BufReader::new(file))
            .map_err(|err| format!("ERR: Failed to read zip archive: {}", err))?;

        let total_files = archive.len();
        let mut extracted_files = 0;

        let zip_parent_dir = Path::new(&zip_path_clone)
            .parent()
            .ok_or_else(|| "ERR: Could not determine parent directory".to_string())?;

        let mut first_folder: Option<String> = None;

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|err| format!("ERR: Failed to read file in archive: {}", err))?;
            let is_dir = file.name().ends_with('/');

            // Refuse entries that escape the extraction folder. `enclosed_name` rejects
            // absolute paths and any "..", which a crafted or corrupted archive could
            // otherwise use to write anywhere the user can write.
            let relative = match file.enclosed_name() {
                Some(name) => name,
                None => {
                    return Err(format!(
                        "ERR: Refusing archive entry that escapes the destination: {}",
                        file.name()
                    ))
                }
            };
            let out_path = zip_parent_dir.join(relative);

            if is_dir {
                std::fs::create_dir_all(&out_path)
                    .map_err(|err| format!("ERR: Failed to create directory: {}", err))?;
                if first_folder.is_none() {
                    first_folder = Some(out_path.to_string_lossy().to_string());
                }
            } else {
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|err| {
                        format!("ERR: Failed to create parent directory: {}", err)
                    })?;
                }

                let mut out_file = File::create(&out_path)
                    .map_err(|err| format!("ERR: Failed to create file: {}", err))?;
                std::io::copy(&mut file, &mut out_file)
                    .map_err(|err| format!("ERR: Failed to write file: {}", err))?;

                // Preserve unix permissions (e.g. the executable bit of the game binary)
                #[cfg(unix)]
                if let Some(mode) = file.unix_mode() {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(mode));
                }
            }

            // Update progress after each file is extracted
            extracted_files += 1;
            let progress = (extracted_files as f64 / total_files as f64) * 100.0;
            app_handle
                .emit("extract-progress", progress)
                .map_err(|err| format!("ERR: Failed to emit progress: {}", err))?;
        }

        first_folder.ok_or_else(|| "ERR: No folder found in the ZIP archive".to_string())
    })
    .await
    .map_err(|err| format!("ERR: Task failed: {}", err))?
}

/// Records panics before they take the process down. Without this a panic in a command
/// thread prints to a stderr no desktop-launched user ever sees, and the report reaches
/// us as "it crashed".
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".to_string());
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".to_string());
        log::error!("PANIC at {}: {}", location, message);
        eprintln!("PANIC at {}: {}", location, message);
        previous(info);
    }));
}

/// Raises the open-file soft limit to the hard limit on Linux.
///
/// WebKitGTK opens a lot of descriptors: its web and network processes inherit ours, and
/// every media element on the page gets a GStreamer pipeline holding several. On the many
/// distributions that still ship a 1024 soft limit, users hit
/// `Failed to create shared memory: Too many open files` and the page process dies,
/// leaving a blank window (reported on issues #4 and #15). Raising the soft limit is what
/// those users did by hand with `ulimit -n`; doing it here, before any child process
/// exists, means they inherit the higher limit too.
/// Bundle identifier, which decides where the log file lives. Kept in step with
/// tauri.conf.json by a test below.
const APP_IDENTIFIER: &str = "com.opentaiko.hub";

/// The folder the Hub keeps its working data in, matching `GetRootPath()` in the
/// frontend: the executable's own folder on Windows, the user's Documents folder
/// elsewhere. `tmp/` and the shared `Songs/` library already live here.
fn hub_root_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        // What Tauri's resourceDir() resolves to on Windows.
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
    }
    #[cfg(not(target_os = "windows"))]
    {
        // documentDir() with the same home-directory fallback the frontend uses, for
        // minimal installs where XDG_DOCUMENTS_DIR is not configured.
        dirs::document_dir().or_else(dirs::home_dir)
    }
}

/// True when the log plugin will actually be able to write here. Creating the folder is
/// not proof: an existing folder inside a read-only install directory creates fine and
/// then swallows every line. Probing the real log file rather than a scratch file avoids
/// leaving a stray file behind, and tests exactly what the plugin is about to open.
fn log_file_is_writable(dir: &std::path::Path) -> bool {
    std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(dir.join("OpenTaiko-Hub.log"))
        .is_ok()
}

/// Resolves and creates the folder the log file goes in.
///
/// It sits beside the Hub's other working data, because that is where someone looks for
/// it; a per-user application-data folder is somewhere nobody thinks to check. An install
/// whose own folder is read-only falls back to that folder rather than losing the log.
///
/// The log plugin treats a failure to create its folder as a fatal startup error, so
/// resolving it here means an unwritable or full directory costs us the log file rather
/// than leaving the user with no window at all.
fn usable_log_dir() -> Option<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Some(root) = hub_root_dir() {
        candidates.push(root.join("logs"));
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("Library/Logs").join(APP_IDENTIFIER));
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(data) = dirs::data_local_dir() {
        candidates.push(data.join(APP_IDENTIFIER).join("logs"));
    }

    candidates.into_iter().find(|dir| {
        std::fs::create_dir_all(dir).is_ok() && log_file_is_writable(dir)
    })
}

/// The resolved log folder, held so the UI can offer to open it. `None` when no writable
/// folder was found and logging fell back to stderr.
struct LogDirectory(Option<std::path::PathBuf>);

/// Where the log file lives, so a user filing a report can find it without being told a
/// per-platform path. `None` means there is no file to open.
#[tauri::command]
fn log_directory(dir: tauri::State<'_, LogDirectory>) -> Option<String> {
    dir.0.as_ref().map(|d| d.display().to_string())
}

/// Outcome of the descriptor-limit change, reported once the logger exists.
#[cfg(target_os = "linux")]
fn raise_open_file_limit() -> Option<String> {
    // SAFETY: plain libc calls on a stack-allocated struct; no pointers escape.
    unsafe {
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) != 0 {
            return Some("Could not read the open-file limit".to_string());
        }
        // An unlimited hard limit cannot be requested verbatim (the kernel caps it at
        // fs.nr_open), and one million is far more than this app can ever use.
        let target = if limit.rlim_max == libc::RLIM_INFINITY {
            1 << 20
        } else {
            limit.rlim_max
        };
        let previous = limit.rlim_cur;
        if previous >= target {
            return Some(format!("Open-file limit already {}", previous));
        }
        limit.rlim_cur = target;
        let outcome = if libc::setrlimit(libc::RLIMIT_NOFILE, &limit) == 0 {
            format!("Raised the open-file limit from {} to {}", previous, target)
        } else {
            format!(
                "Could not raise the open-file limit from {} (hard limit {})",
                previous, limit.rlim_max
            )
        };
        // Also to stderr: this runs before the logger exists, and a terminal launch
        // should show it immediately.
        eprintln!("{}", outcome);
        Some(outcome)
    }
}

#[cfg(not(target_os = "linux"))]
fn raise_open_file_limit() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    /// The log folder is resolved from this identifier, so a change in tauri.conf.json
    /// that is not mirrored here would silently write logs somewhere else.
    #[test]
    fn the_identifier_matches_the_bundle_config() {
        let config = include_str!("../tauri.conf.json");
        let expected = format!("\"identifier\": \"{}\"", super::APP_IDENTIFIER);
        assert!(
            config.contains(&expected),
            "APP_IDENTIFIER must match tauri.conf.json's identifier"
        );
    }

    /// Creating the folder is not proof it can be written to, so the probe has to open a
    /// file. It must also open the real log file, not a scratch file it leaves behind.
    #[test]
    fn the_writability_probe_leaves_only_the_log_file() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(super::log_file_is_writable(tmp.path()));

        let entries: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("OpenTaiko-Hub.log")]);
    }

    #[test]
    fn a_folder_that_does_not_exist_is_not_writable() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!super::log_file_is_writable(&tmp.path().join("absent")));
    }

    /// The log has to land beside the Hub's other working data (`tmp/`, `Songs/`), which
    /// is where a user looks for it. On Windows that is the executable's own folder, the
    /// same place the frontend's GetRootPath() resolves to.
    #[test]
    fn the_log_folder_sits_with_the_hubs_other_data() {
        let dir = super::usable_log_dir().expect("a writable log folder must be found");
        assert!(super::log_file_is_writable(&dir));
        assert_eq!(dir.file_name().unwrap(), "logs");

        #[cfg(target_os = "windows")]
        {
            let exe_dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
            assert_eq!(dir, exe_dir.join("logs"));
        }
    }
}

fn main() {
    install_panic_hook();

    // WebKitGTK 2.42+ renders a blank window on several Linux setups (notably NVIDIA
    // and some Mesa drivers) unless the DMABUF renderer is disabled. Apply the standard
    // workaround by default, while still letting a user override it from the environment.
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    // Must happen before the webview exists so its helper processes inherit the limit.
    let fd_limit_outcome = raise_open_file_limit();

    let ui_health = Arc::new(health::UiHealth::new());

    // Stderr is always available; the file target is added only when its folder could be
    // created, so logging can never be the reason the app fails to start.
    let mut log_targets = vec![tauri_plugin_log::Target::new(
        tauri_plugin_log::TargetKind::Stderr,
    )];
    let log_dir = usable_log_dir();
    if let Some(dir) = log_dir.clone() {
        log_targets.push(tauri_plugin_log::Target::new(
            tauri_plugin_log::TargetKind::Folder {
                path: dir,
                file_name: Some("OpenTaiko-Hub".to_string()),
            },
        ));
    }

    let app = tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets(log_targets)
                .level(log::LevelFilter::Info)
                .max_file_size(2 * 1024 * 1024)
                // The default (KeepOne) deletes the log the moment it reaches the size
                // cap, which would discard the startup block — version, log folder,
                // file-descriptor limit — that a bug report is written to carry. Keeping
                // the previous file under a dated name costs one extra file at most.
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(2))
                .build(),
        )
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_upload::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            unzip_and_get_first_folder,
            execute_external_app,
            binver::get_game_version,
            scan::scan_songs,
            assetscan::scan_asset_versions,
            procs::run_streamed,
            fsops::merge_move_dir,
            migrate::plan_migration,
            migrate::paths_are_same_dir,
            migrate::apply_migration,
            migrate::find_duplicate_song_folders,
            migrate::strip_song_content,
            migrate::remove_duplicate_song_folders,
            configini::ensure_config_tjapath,
            health::ui_heartbeat,
            log_directory
        ])
        .manage(ui_health.clone())
        .manage(LogDirectory(log_dir.clone()))
        .setup(move |app| {
            // Everything here runs after the plugin installed the global logger, so
            // anything recorded before startup is reported from this point instead.
            log::info!(
                "OpenTaiko Hub {} starting on {}",
                app.package_info().version,
                std::env::consts::OS
            );
            match &log_dir {
                Some(dir) => log::info!("Logging to {}", dir.display()),
                None => log::warn!("No writable log folder; logging to stderr only"),
            }
            if let Some(outcome) = &fd_limit_outcome {
                log::info!("{}", outcome);
            }
            health::spawn_watcher(ui_health.clone());
            Ok(())
        })
        .build(tauri::generate_context!());

    // A startup failure used to abort with a message only a terminal would show. Record
    // it, then tell the user something instead of vanishing.
    match app {
        Ok(app) => app.run(|_app_handle, _event| {}),
        Err(error) => {
            log::error!("The Hub could not start: {}", error);
            eprintln!("The Hub could not start: {}", error);
            std::process::exit(1);
        }
    }
}
