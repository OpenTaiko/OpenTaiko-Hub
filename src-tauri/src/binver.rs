// Reads the game version directly from the built binary:
// - Windows builds: PE version resource of OpenTaiko.exe (the apphost carries the app's version).
// - Linux single-file builds: the ELF apphost has no version resource, but the .NET bundle
//   embeds OpenTaiko.dll (a PE image) uncompressed, so we scan for embedded PE headers and
//   read the version resource of the one whose OriginalFilename is OpenTaiko.dll.
use pelite::pe64::Pe as Pe64;
use pelite::pe32::Pe as Pe32;
use pelite::FileMap;

const GAME_DLL_NAME: &str = "OpenTaiko.dll";

fn version_from_info(version_info: pelite::resources::version_info::VersionInfo) -> (Option<String>, Option<String>) {
    let version = version_info.fixed().map(|fixed| {
        let v = fixed.dwFileVersion;
        format!("{}.{}.{}.{}", v.Major, v.Minor, v.Patch, v.Build)
    });

    let mut original_filename: Option<String> = None;
    for lang in version_info.translation() {
        if let Some(value) = version_info.value(*lang, "OriginalFilename") {
            let trimmed = value.trim_end_matches('\0').trim().to_string();
            if !trimmed.is_empty() {
                original_filename = Some(trimmed);
                break;
            }
        }
    }

    (version, original_filename)
}

fn version_from_pe(data: &[u8]) -> Option<(String, Option<String>)> {
    let (version, original_filename) = match pelite::PeFile::from_bytes(data).ok()? {
        pelite::Wrap::T32(pe) => version_from_info(pe.resources().ok()?.version_info().ok()?),
        pelite::Wrap::T64(pe) => version_from_info(pe.resources().ok()?.version_info().ok()?),
    };
    version.map(|v| (v, original_filename))
}

/// Scans a .NET single-file bundle (or any blob) for embedded PE images and returns the
/// file version of the one matching `want_filename`.
fn scan_embedded_pe_version(data: &[u8], want_filename: &str) -> Option<String> {
    let finder = memchr::memmem::Finder::new(b"MZ");
    for offset in finder.find_iter(data) {
        if offset + 0x40 > data.len() {
            break;
        }
        let e_lfanew =
            u32::from_le_bytes([data[offset + 0x3C], data[offset + 0x3D], data[offset + 0x3E], data[offset + 0x3F]])
                as usize;
        // Sanity bounds: managed images keep the PE header close to the DOS header
        if e_lfanew < 0x40 || e_lfanew > 0x1000 || offset + e_lfanew + 4 > data.len() {
            continue;
        }
        if &data[offset + e_lfanew..offset + e_lfanew + 4] != b"PE\0\0" {
            continue;
        }
        if let Some((version, Some(original))) = version_from_pe(&data[offset..]) {
            if original.eq_ignore_ascii_case(want_filename) {
                return Some(version);
            }
        }
    }
    None
}

fn version_from_file(path: &std::path::Path) -> Option<String> {
    let map = FileMap::open(path).ok()?;
    let data = map.as_ref();
    if let Some((version, _)) = version_from_pe(data) {
        return Some(version);
    }
    scan_embedded_pe_version(data, GAME_DLL_NAME)
}

#[tauri::command]
pub async fn get_game_version(instance_dir: String) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || {
        let dir = std::path::PathBuf::from(&instance_dir);
        for candidate in ["OpenTaiko.exe", "OpenTaiko.dll", "OpenTaiko"] {
            let path = dir.join(candidate);
            if !path.is_file() {
                continue;
            }
            if let Some(version) = version_from_file(&path) {
                return Ok(Some(version));
            }
        }
        Ok(None)
    })
    .await
    .map_err(|e| format!("Task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    // Linux single-file release binary (ELF apphost with embedded OpenTaiko.dll),
    // when present, must report the release version via the bundle scan.
    #[test]
    fn reads_version_from_linux_single_file_bundle() {
        let path = std::path::Path::new(concat!(
            "C:/Users/raphael/AppData/Local/Temp/claude/c--Users-raphael-Documents-GitHub-OpenTaiko-Hub/",
            "ec0f8566-0080-4132-9b36-0fbdad314293/scratchpad/OpenTaiko-linux-elf"
        ));
        if !path.is_file() {
            eprintln!("skipped: no downloaded linux release binary");
            return;
        }
        let version = version_from_file(path);
        assert_eq!(version.as_deref(), Some("0.6.0.107"));
    }

    // Local build of the 0.6.1 branch, when present, must report its csproj version.
    #[test]
    fn reads_version_from_local_game_build() {
        let path = std::path::Path::new(
            "C:/Users/raphael/Documents/GitHub/OpenTaiko/OpenTaiko/bin/Release/net8.0/OpenTaiko.exe",
        );
        if !path.is_file() {
            eprintln!("skipped: no local game build");
            return;
        }
        let version = version_from_file(path);
        assert_eq!(version.as_deref(), Some("0.6.1.0"));
    }
}
