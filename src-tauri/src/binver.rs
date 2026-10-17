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
        if !(0x40..=0x1000).contains(&e_lfanew) || offset + e_lfanew + 4 > data.len() {
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

    // Needs a real game binary, which the repository does not ship. Run with
    //   OTK_TEST_GAME_BINARY=<path to OpenTaiko(.exe)> OTK_TEST_GAME_VERSION=<expected>
    //   cargo test -- --ignored
    // Worth doing with a Linux single-file release (ELF apphost with an embedded
    // OpenTaiko.dll, read through the bundle scan) and with a Windows build (read from
    // its version resource), since the two are read differently.
    #[test]
    #[ignore = "needs OTK_TEST_GAME_BINARY and OTK_TEST_GAME_VERSION"]
    fn reads_the_version_of_a_real_game_binary() {
        let path = std::env::var_os("OTK_TEST_GAME_BINARY").expect("OTK_TEST_GAME_BINARY is not set");
        let expected = std::env::var("OTK_TEST_GAME_VERSION").expect("OTK_TEST_GAME_VERSION is not set");
        let version = version_from_file(std::path::Path::new(&path));
        assert_eq!(version.as_deref(), Some(expected.as_str()));
    }
}
