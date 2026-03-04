use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::AgentError;

const GCS_BUCKET: &str = "https://storage.googleapis.com/claude-code-dist-86c565f3-f756-42ad-8dfa-d59b1c096819/claude-code-releases";

#[derive(Deserialize)]
struct Manifest {
    platforms: HashMap<String, PlatformInfo>,
}

#[derive(Deserialize)]
struct PlatformInfo {
    checksum: String,
}

/// Downloads and installs the Claude Code CLI binary to the specified file path.
///
/// This function:
/// 1. Validates the file extension (`.exe` on Windows, no extension on Unix)
/// 2. Fetches the latest version string from the release channel
/// 3. Downloads the manifest to get the SHA-256 checksum
/// 4. Downloads the platform-appropriate binary
/// 5. Verifies the SHA-256 checksum
/// 6. Writes the binary to the given path
/// 7. Sets executable permissions (unix)
///
/// Returns the path to the installed binary.
///
/// The CLI is **never installed implicitly** — this function must be called
/// explicitly by the user.
pub fn install_cli(path: &Path) -> Result<PathBuf, AgentError> {
    // Validate extension
    let expected_ext = if cfg!(target_os = "windows") { Some("exe") } else { None };
    let actual_ext = path.extension().and_then(|e| e.to_str());
    if actual_ext != expected_ext {
        return Err(AgentError::InvalidExtension);
    }

    let platform = detect_platform()?;

    // 1. Fetch latest version
    let version_url = format!("{GCS_BUCKET}/latest");
    let version = fetch_text(&version_url)?;
    let version = version.trim();

    // 2. Fetch manifest for checksums
    let manifest_url = format!("{GCS_BUCKET}/{version}/manifest.json");
    let manifest_text = fetch_text(&manifest_url)?;
    let manifest: Manifest = serde_json::from_str(&manifest_text)
        .map_err(|e| AgentError::DownloadFailed(anyhow::anyhow!("failed to parse manifest: {e}")))?;

    let platform_info = manifest
        .platforms
        .get(platform)
        .ok_or(AgentError::UnsupportedPlatform)?;

    // 3. Download binary
    let binary_url = format!("{GCS_BUCKET}/{version}/{platform}/{}", binary_name());
    let binary_data = fetch_bytes(&binary_url)?;

    // 4. Verify checksum
    let mut hasher = Sha256::new();
    hasher.update(&binary_data);
    let hash = format!("{:x}", hasher.finalize());

    if hash != platform_info.checksum {
        return Err(AgentError::ChecksumMismatch);
    }

    // 5. Create parent directory and write binary
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AgentError::DownloadFailed(anyhow::anyhow!("failed to create directory: {e}")))?;
    }

    fs::write(path, &binary_data)
        .map_err(|e| AgentError::DownloadFailed(anyhow::anyhow!("failed to write binary: {e}")))?;

    // 6. Set executable permissions (unix)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .map_err(|e| AgentError::DownloadFailed(anyhow::anyhow!("failed to set permissions: {e}")))?;
    }

    Ok(path.to_path_buf())
}

fn binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "claude.exe"
    } else {
        "claude"
    }
}

fn detect_platform() -> Result<&'static str, AgentError> {
    if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        Ok("darwin-arm64")
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "x86_64") {
        Ok("darwin-x64")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        Ok("linux-arm64")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        Ok("linux-x64")
    } else if cfg!(target_os = "windows") && cfg!(target_arch = "x86_64") {
        Ok("win32-x64")
    } else if cfg!(target_os = "windows") && cfg!(target_arch = "aarch64") {
        Ok("win32-arm64")
    } else {
        Err(AgentError::UnsupportedPlatform)
    }
}

fn fetch_text(url: &str) -> Result<String, AgentError> {
    let response = ureq::get(url)
        .call()
        .map_err(|e| AgentError::DownloadFailed(anyhow::anyhow!("{e}")))?;

    response
        .into_body()
        .read_to_string()
        .map_err(|e| AgentError::DownloadFailed(anyhow::anyhow!("{e}")))
}

fn fetch_bytes(url: &str) -> Result<Vec<u8>, AgentError> {
    let response = ureq::get(url)
        .call()
        .map_err(|e| AgentError::DownloadFailed(anyhow::anyhow!("{e}")))?;

    let mut bytes = Vec::new();
    response
        .into_body()
        .as_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| AgentError::DownloadFailed(anyhow::anyhow!("{e}")))?;

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_exe_extension_on_unix() {
        if cfg!(target_os = "windows") {
            return;
        }
        let path = Path::new("/tmp/claude.exe");
        let err = install_cli(path).unwrap_err();
        assert!(matches!(err, AgentError::InvalidExtension));
    }

    #[test]
    fn rejects_random_extension() {
        let path = Path::new("/tmp/claude.bin");
        let err = install_cli(path).unwrap_err();
        assert!(matches!(err, AgentError::InvalidExtension));
    }

    #[test]
    fn detect_platform_returns_ok() {
        let result = detect_platform();
        assert!(result.is_ok());
        let platform = result.unwrap();
        assert!(
            ["darwin-arm64", "darwin-x64", "linux-arm64", "linux-x64", "win32-x64", "win32-arm64"]
                .contains(&platform)
        );
    }

    #[test]
    fn binary_name_no_extension_on_unix() {
        if cfg!(target_os = "windows") {
            return;
        }
        assert_eq!(binary_name(), "claude");
    }

    #[test]
    fn binary_name_exe_on_windows() {
        if !cfg!(target_os = "windows") {
            return;
        }
        assert_eq!(binary_name(), "claude.exe");
    }
}
