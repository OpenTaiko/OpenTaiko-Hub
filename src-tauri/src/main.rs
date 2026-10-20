// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod library;
mod assetscan;
mod binver;
mod configini;
#[cfg(debug_assertions)]
mod devdata;
mod download;
mod fsops;
mod guard;
mod health;
mod jobs;
mod migrate;
mod procs;
mod relocate;
mod scan;
mod writes;

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;
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

/// The game binary's name without extension. Launching is limited to it, inside a folder
/// the Hub manages, so this command cannot be used to start anything else.
const GAME_BINARY: &str = "OpenTaiko";

/// Launches the game of an instance. Only the game binary of one of the Hub's instances
/// starts: a path to launch used to be accepted from anywhere the page could write,
/// including the Hub's temporary folder.
#[tauri::command]
fn launch_game(instance_dir: String, app_handle: AppHandle) -> Result<(), String> {
    let instance = guard::instance_dir(&app_handle, &instance_dir)?;
    let path = instance.join(GAME_BINARY);
    let full_path = if cfg!(windows) {
        path.with_extension("exe").to_string_lossy().into_owned()
    } else {
        path.to_string_lossy().into_owned()
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
    let mut command = Command::new(&full_path);
    command.current_dir(dir).stderr(Stdio::piped());
    #[cfg(target_os = "linux")]
    procs::apply_env(&mut command, procs::game_env(&std::env::vars().collect(), &dir.to_string_lossy()));
    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to execute the app: {}", e))?;

    log::info!("Launched {} (pid {})", full_path, child.id());
    running_games(&app_handle).push(instance.clone());

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
        {
            let mut running = running_games(&app_handle);
            if let Some(at) = running.iter().position(|dir| dir == &instance) {
                running.remove(at);
            }
        }
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

/// Instance folders whose game the Hub started and that is still running, once per copy.
#[derive(Default)]
struct RunningGames(std::sync::Mutex<Vec<std::path::PathBuf>>);

fn running_games<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> std::sync::MutexGuard<'_, Vec<std::path::PathBuf>> {
    app.state::<RunningGames>().inner().0.lock().unwrap_or_else(|e| e.into_inner())
}

/// True while a game the Hub started from `dir` runs. A game started some other way is
/// not known.
pub(crate) fn game_is_running<R: tauri::Runtime>(app: &tauri::AppHandle<R>, dir: &Path) -> bool {
    running_games(app).iter().any(|running| migrate::is_same_dir(running, dir))
}

/// For the saves page: the game keeps its save database open and writes it back when it
/// exits, which undid any change made meanwhile.
#[tauri::command]
fn game_running(instance_dir: String, app_handle: AppHandle) -> bool {
    game_is_running(&app_handle, Path::new(&instance_dir))
}

#[tauri::command]
async fn unzip_and_get_first_folder(
    zip_path: String,
    job: Option<String>,
    on_progress: tauri::ipc::Channel<f64>,
    app_handle: AppHandle,
) -> Result<String, String> {
    // The archive is extracted next to itself, so only one the Hub downloaded into a
    // temporary folder of its own: extracting any file the page could reach let page
    // script unpack a zip it had written as a save database over the game's executable.
    let zip = guard::checked(&app_handle, &zip_path)?;
    if !zip.parent().is_some_and(guard::is_hub_tmp_dir) || !download::take_downloaded_archive(&zip) {
        return Err(format!("ERR: Refusing to extract {}: not an archive the Hub downloaded", zip_path));
    }
    let cancel = jobs::token(&app_handle, &job);
    let zip_path_clone = zip_path.clone();

    task::spawn_blocking(move || {
        let file = File::open(&zip_path_clone)
            .map_err(|err| format!("ERR: Failed to open zip file: {}", err))?;
        let mut archive = ZipArchive::new(BufReader::new(file))
            .map_err(|err| format!("ERR: Failed to read zip archive: {}", err))?;

        let total_files = archive.len().max(1);
        let mut extracted_files = 0;
        let mut last_percent = usize::MAX;

        let zip_parent_dir = Path::new(&zip_path_clone)
            .parent()
            .ok_or_else(|| "ERR: Could not determine parent directory".to_string())?;

        let mut first_folder: Option<String> = None;

        for i in 0..archive.len() {
            cancel.check()?;
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
            let out_path = zip_parent_dir.join(&relative);

            // The archive's top folder, taken from the first entry that sits in a folder.
            // Many zip tools write no entries for folders themselves, so waiting for a
            // folder entry failed on those archives with "No folder found".
            if first_folder.is_none() {
                let mut parts = relative.components();
                if let Some(std::path::Component::Normal(top)) = parts.next() {
                    if is_dir || parts.next().is_some() {
                        first_folder = Some(zip_parent_dir.join(top).to_string_lossy().to_string());
                    }
                }
            }

            if is_dir {
                std::fs::create_dir_all(&out_path)
                    .map_err(|err| format!("ERR: Failed to create directory: {}", err))?;
            } else {
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|err| {
                        format!("ERR: Failed to create parent directory: {}", err)
                    })?;
                }

                // Buffered: unbuffered, a game build of a gigabyte and more was written in
                // one system call per 8 KiB read from the archive. Never over an existing
                // file: the folder is a fresh one, and nothing in it is to be replaced.
                let created = std::fs::OpenOptions::new().write(true).create_new(true).open(&out_path);
                let mut out_file = std::io::BufWriter::with_capacity(
                    library::COPY_BUFFER,
                    created.map_err(|err| format!("ERR: Failed to create {}: {}", out_path.display(), err))?,
                );
                std::io::copy(&mut file, &mut out_file)
                    .and_then(|_| std::io::Write::flush(&mut out_file))
                    .map_err(|err| format!("ERR: Failed to write file: {}", err))?;
                drop(out_file);

                // Preserve unix permissions (e.g. the executable bit of the game binary)
                #[cfg(unix)]
                if let Some(mode) = file.unix_mode() {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(mode));
                }
            }

            // Progress goes to this call's own channel, and only when the whole percentage
            // changes: an archive holds thousands of files, and a message per file was a
            // script evaluation per file in the page.
            extracted_files += 1;
            let percent = extracted_files * 100 / total_files;
            if percent != last_percent {
                last_percent = percent;
                let _ = on_progress.send(percent as f64);
            }
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

/// Bundle identifier, which decides where the log file lives. Kept in step with
/// tauri.conf.json by a test below.
const APP_IDENTIFIER: &str = "com.opentaiko.hub";

/// The folder the Hub keeps its working data in (`tmp/`, the shared `Songs/` library,
/// new instances, logs): the executable's own folder on Windows, an "OpenTaiko Hub"
/// folder in Documents elsewhere. Dev builds use their own folder instead (devdata.rs).
/// The page asks for it through `hub_root` rather than working it out a second way.
fn hub_root_dir() -> Option<std::path::PathBuf> {
    #[cfg(debug_assertions)]
    {
        Some(devdata::dev_root())
    }
    #[cfg(all(not(debug_assertions), target_os = "windows"))]
    {
        // What Tauri's resourceDir() resolves to on Windows.
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
    }
    #[cfg(all(not(debug_assertions), not(target_os = "windows")))]
    {
        // A folder of its own in Documents, or in the home directory on minimal installs
        // where XDG_DOCUMENTS_DIR is not configured. Not Documents itself: generic names
        // such as Songs or tmp there are often the user's own (relocate.rs).
        old_shared_root().map(|dir| dir.join("OpenTaiko Hub"))
    }
}

/// Where Linux and macOS kept the Hub's data before it had a folder of its own.
#[cfg(all(not(debug_assertions), not(target_os = "windows")))]
fn old_shared_root() -> Option<std::path::PathBuf> {
    dirs::document_dir().or_else(dirs::home_dir)
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

/// What moving the Hub's data did at startup, when there is something to tell the user.
struct RelocationReport(std::sync::Mutex<Option<relocate::Report>>);

/// Taken once by the page, which tells the user: a move that partly failed used to be
/// written to the log only, leaving an empty library and nothing to explain it.
#[tauri::command]
fn take_relocation_report(report: tauri::State<'_, RelocationReport>) -> Option<relocate::Report> {
    report.0.lock().unwrap_or_else(|e| e.into_inner()).take()
}

#[tauri::command]
fn hub_root() -> Result<String, String> {
    hub_root_dir()
        .map(|dir| dir.to_string_lossy().into_owned())
        .ok_or_else(|| "The Hub folder could not be resolved".to_string())
}

/// Opens a folder in the system file manager. Only folders are accepted: handing a file
/// to the system opener runs it when it is a program. The page has no opener permission
/// for paths, so this is the only way it can open one.
#[tauri::command]
fn open_folder(
    path: String,
    app_handle: AppHandle,
    logs: tauri::State<'_, LogDirectory>,
) -> Result<(), String> {
    let is_log_dir = logs.0.as_deref().is_some_and(|d| migrate::is_same_dir(d, Path::new(&path)));
    if !is_log_dir {
        guard::checked(&app_handle, &path)?;
    }
    if !Path::new(&path).is_dir() {
        return Err(format!("{} is not a folder", path));
    }
    app_handle
        .opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
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
///
/// Returns the outcome of the change, reported once the logger exists.
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

fn main() {
    install_panic_hook();

    // Data moved to a new root folder, before anything creates folders there (the log
    // folder, below) so the old ones can still move: a dev build's data out of
    // target/debug, and on Linux and macOS the Hub's folders out of Documents itself.
    let registry = dirs::config_dir().unwrap_or_default().join(APP_IDENTIFIER).join("instances.json");
    // Where Tauri's app_local_data_dir puts it (guard.rs)
    let roots = dirs::data_local_dir().unwrap_or_default().join(APP_IDENTIFIER).join(guard::INSTANCE_ROOTS);
    #[cfg(debug_assertions)]
    let relocation_report = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
        .map(|old_root| devdata::migrate_from(&old_root, &devdata::dev_root(), &registry, &roots))
        .unwrap_or_default();
    #[cfg(all(not(debug_assertions), not(target_os = "windows")))]
    let relocation_report = match (old_shared_root(), hub_root_dir()) {
        (Some(old_root), Some(new_root)) => relocate::migrate_documents_layout(&old_root, &new_root, &registry, &roots),
        _ => relocate::Report::default(),
    };
    #[cfg(all(not(debug_assertions), target_os = "windows"))]
    let relocation_report = { let _ = (&registry, &roots); relocate::Report::default() };
    for line in &relocation_report.lines {
        eprintln!("{}", line);
    }

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
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            unzip_and_get_first_folder,
            launch_game,
            game_running,
            open_folder,
            hub_root,
            binver::get_game_version,
            scan::scan_songs,
            scan::count_songs,
            assetscan::scan_asset_versions,
            procs::list_dotnet_sdks,
            procs::publish_game,
            download::download_file,
            download::install_files,
            download::fetch_cached,
            guard::allow_build_dir,
            guard::create_instance_dir,
            guard::attach_instance_dir,
            guard::pick_files,
            writes::write_settings_file,
            writes::write_version_json,
            writes::write_saves,
            writes::export_save,
            jobs::begin_job,
            jobs::cancel_job,
            jobs::end_job,
            jobs::reset_jobs,
            fsops::merge_move_dir,
            migrate::plan_migration,
            migrate::paths_are_same_dir,
            migrate::apply_migration,
            migrate::find_duplicate_song_folders,
            migrate::strip_song_content,
            migrate::remove_duplicate_song_folders,
            configini::ensure_config_tjapath,
            library::read_box_def,
            library::write_box_def,
            library::create_genre_folder,
            library::delete_library_entry,
            library::import_chart_archives,
            library::set_folder_jacket,
            library::read_song_media,
            library::set_song_preimage,
            health::ui_heartbeat,
            log_directory,
            take_relocation_report
        ])
        .manage(ui_health.clone())
        .manage(LogDirectory(log_dir.clone()))
        .manage(RelocationReport(std::sync::Mutex::new(
            (!relocation_report.is_quiet()).then(|| relocation_report.clone()),
        )))
        .manage(jobs::Jobs::default())
        .manage(RunningGames::default())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if !jobs::allow_close(window) {
                    api.prevent_close();
                }
            }
        })
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
            for line in &relocation_report.lines {
                log::info!("{}", line);
            }
            health::spawn_watcher(ui_health.clone());
            guard::init(app.handle());
            let instances = guard::instance_roots(app.handle());
            // Linux and macOS kept tmp in Documents itself before the Hub had a folder
            #[cfg(all(not(debug_assertions), not(target_os = "windows")))]
            let old_tmp = old_shared_root().map(|root| root.join("tmp"));
            #[cfg(any(debug_assertions, target_os = "windows"))]
            let old_tmp = None;
            std::thread::spawn(move || guard::sweep_leftovers(instances, old_tmp));
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

#[cfg(test)]
mod tests {
    /// The Hub's version is written in three places. The crate's one is the version
    /// downloads announce (download.rs), and it was left at 0.1.0 while the others moved.
    #[test]
    fn the_version_is_the_same_everywhere() {
        let expected = format!("\"version\": \"{}\"", env!("CARGO_PKG_VERSION"));
        assert!(include_str!("../tauri.conf.json").contains(&expected), "Cargo.toml and tauri.conf.json disagree on the version");
        assert!(include_str!("../../package.json").contains(&expected), "Cargo.toml and package.json disagree on the version");
    }

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
    /// is where a user looks for it: the Hub root the frontend also uses.
    #[test]
    fn the_log_folder_sits_with_the_hubs_other_data() {
        let dir = super::usable_log_dir().expect("a writable log folder must be found");
        assert!(super::log_file_is_writable(&dir));
        assert_eq!(dir.file_name().unwrap(), "logs");

        assert_eq!(dir, super::hub_root_dir().unwrap().join("logs"));
    }
}
