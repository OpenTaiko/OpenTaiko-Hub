// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assetscan;
mod binver;
mod configini;
mod fsops;
mod migrate;
mod procs;
mod scan;

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::process::Command;
use tauri::{AppHandle, Emitter};
use tokio::task;
use zip::ZipArchive;

#[tauri::command]
fn execute_external_app(os: String, path: String) -> Result<(), String> {
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

    Command::new(&full_path)
        .current_dir(dir)
        .spawn()
        .map_err(|e| format!("Failed to execute the app: {}", e))?;

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
            let file_name = file.name();

            let out_path = zip_parent_dir.join(file_name);

            if file_name.ends_with('/') {
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

fn main() {
    // WebKitGTK 2.42+ renders a blank window on several Linux setups (notably NVIDIA
    // and some Mesa drivers) unless the DMABUF renderer is disabled. Apply the standard
    // workaround by default, while still letting a user override it from the environment.
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    tauri::Builder::default()
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
            migrate::apply_migration,
            migrate::find_duplicate_song_folders,
            migrate::strip_song_content,
            migrate::remove_duplicate_song_folders,
            configini::ensure_config_tjapath
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
