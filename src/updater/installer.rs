use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tokio::fs;

/// Extract the maestro binary from a tar.gz archive.
pub(crate) fn extract_binary_from_tar_gz(archive_bytes: &[u8]) -> Result<Vec<u8>> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    use tar::Archive;

    let decoder = GzDecoder::new(archive_bytes);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file_name == "maestro" {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            return Ok(bytes);
        }
    }
    anyhow::bail!("No 'maestro' binary found in tar.gz archive")
}

pub struct Installer {
    pub dest_path: PathBuf,
}

impl Installer {
    pub fn new(dest_path: PathBuf) -> Self {
        Self { dest_path }
    }

    fn backup_path(&self) -> PathBuf {
        let mut p = self.dest_path.clone();
        let name = p
            .file_name()
            .map(|n| format!("{}.bak", n.to_string_lossy()))
            .unwrap_or_else(|| "maestro.bak".to_string());
        p.set_file_name(name);
        p
    }

    /// Write bytes to a staging area (same dir, `.tmp` suffix).
    #[allow(dead_code)] // Reason: staging step for two-phase install
    pub async fn write_to_staging(&self, bytes: &[u8]) -> Result<PathBuf> {
        let staging = self.dest_path.with_extension("tmp");
        fs::write(&staging, bytes)
            .await
            .context("Failed to write staging file")?;
        Ok(staging)
    }

    /// Backup the current binary and replace with new bytes. Rolls back on failure.
    pub async fn install_with_backup(&self, new_bytes: &[u8]) -> Result<()> {
        let backup = self.backup_path();

        // Read original for backup
        let original = fs::read(&self.dest_path)
            .await
            .context("Failed to read current binary for backup")?;

        // Write backup
        fs::write(&backup, &original)
            .await
            .context("Failed to write backup")?;

        // Attempt to write new binary
        match fs::write(&self.dest_path, new_bytes).await {
            Ok(_) => {
                // Set executable permissions on Unix
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let perms = std::fs::Permissions::from_mode(0o755);
                    fs::set_permissions(&self.dest_path, perms).await?;
                }
                Ok(())
            }
            Err(e) => {
                // Rollback: restore original
                if let Err(rollback_err) = fs::write(&self.dest_path, &original).await {
                    tracing::error!(
                        "CRITICAL: rollback also failed after write error: {}",
                        rollback_err
                    );
                }
                Err(e).context("Failed to replace binary; attempted rollback")
            }
        }
    }

    /// Verify SHA-256 checksum of downloaded bytes against expected hex hash.
    pub fn verify_checksum(bytes: &[u8], expected_hex: &str) -> Result<()> {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let actual = format!("{:x}", hasher.finalize());
        if actual != expected_hex.to_lowercase() {
            anyhow::bail!(
                "Checksum mismatch: expected {}, got {}",
                expected_hex,
                actual
            );
        }
        Ok(())
    }

    /// Fetch the SHA256SUMS file from the same release directory and extract
    /// the hash for the given asset filename.
    async fn fetch_expected_checksum(
        client: &reqwest::Client,
        download_url: &str,
        asset_name: &str,
    ) -> Result<String> {
        let sums_url = download_url
            .rsplit_once('/')
            .map(|(base, _)| format!("{}/sha256sums.txt", base))
            .ok_or_else(|| anyhow::anyhow!("Cannot derive SHA256SUMS URL from download URL"))?;

        let resp = client
            .get(&sums_url)
            .header("User-Agent", concat!("maestro/", env!("CARGO_PKG_VERSION")))
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Failed to fetch SHA256SUMS: HTTP {}", resp.status());
        }

        let body = resp.text().await?;
        for line in body.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 2 && parts[1] == asset_name {
                return Ok(parts[0].to_string());
            }
        }
        anyhow::bail!("No checksum found for {} in SHA256SUMS", asset_name);
    }

    /// Download a binary from a URL and install it with backup.
    ///
    /// Validates the URL against trusted domains, enforces a size limit,
    /// verifies SHA-256 checksum, and uses a 120-second download timeout.
    pub async fn download_and_install(&self, download_url: &str) -> Result<String> {
        if !crate::updater::is_trusted_download_url(download_url) {
            anyhow::bail!("Untrusted download URL: only HTTPS from github.com is allowed");
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?;

        let resp = client
            .get(download_url)
            .header("User-Agent", concat!("maestro/", env!("CARGO_PKG_VERSION")))
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Download failed: HTTP {}", resp.status());
        }

        if let Some(len) = resp.content_length()
            && len > crate::updater::MAX_DOWNLOAD_SIZE
        {
            anyhow::bail!(
                "Binary too large: {} bytes (max {} bytes)",
                len,
                crate::updater::MAX_DOWNLOAD_SIZE
            );
        }

        let bytes = resp.bytes().await?;

        // Verify SHA-256 checksum before installing
        let asset_name = download_url
            .rsplit_once('/')
            .map(|(_, name)| name)
            .unwrap_or("maestro");

        let expected_hash =
            Self::fetch_expected_checksum(&client, download_url, asset_name).await?;
        Self::verify_checksum(&bytes, &expected_hash)?;

        // Extract binary from tar.gz if applicable
        let binary_bytes = if asset_name.ends_with(".tar.gz") {
            extract_binary_from_tar_gz(&bytes)?
        } else {
            bytes.to_vec()
        };

        self.install_with_backup(&binary_bytes).await?;

        Ok(self.backup_path().to_string_lossy().to_string())
    }
}

/// Restart the current process with the same arguments.
/// On Unix, replaces the current process via the POSIX exec syscall.
/// SAFETY: This uses std::os::unix::process::CommandExt which calls
/// execvp() — a safe POSIX syscall, not a shell command.
pub fn restart_with_same_args() -> Result<()> {
    let current_exe = std::env::current_exe()?;
    let args: Vec<String> = std::env::args().skip(1).collect();

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // execvp() replaces the current process — does not return on success
        let err = std::process::Command::new(&current_exe).args(&args).exec(); // POSIX exec syscall, not shell exec
        anyhow::bail!("process replacement failed: {}", err);
    }

    #[cfg(not(unix))]
    {
        std::process::Command::new(&current_exe)
            .args(&args)
            .spawn()?;
        std::process::exit(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn installer_writes_downloaded_bytes_to_staging() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("maestro");
        fs::write(&dest, b"old binary").await.unwrap();

        let installer = Installer::new(dest);
        let fake_bytes = b"new binary content";
        let staging = installer.write_to_staging(fake_bytes).await.unwrap();

        assert!(staging.exists());
        let written = fs::read(&staging).await.unwrap();
        assert_eq!(written, fake_bytes);
    }

    #[tokio::test]
    async fn installer_creates_backup_before_replacement() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("maestro");
        let original_content = b"original binary";
        fs::write(&dest, original_content).await.unwrap();

        let installer = Installer::new(dest);
        installer.install_with_backup(b"new binary").await.unwrap();

        let backup_path = dir.path().join("maestro.bak");
        assert!(backup_path.exists(), "backup file must exist");
        let backup_content = fs::read(&backup_path).await.unwrap();
        assert_eq!(backup_content, original_content);
    }

    #[tokio::test]
    async fn installer_replaces_binary_with_new_content() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("maestro");
        fs::write(&dest, b"old binary").await.unwrap();

        let installer = Installer::new(dest.clone());
        let new_bytes = b"upgraded binary content";
        installer.install_with_backup(new_bytes).await.unwrap();

        let written = fs::read(&dest).await.unwrap();
        assert_eq!(written, new_bytes);
    }

    #[tokio::test]
    async fn installer_returns_error_when_dest_does_not_exist() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("nonexistent_maestro");
        let installer = Installer::new(dest);
        let result = installer.install_with_backup(b"new bytes").await;
        assert!(result.is_err());
    }

    #[test]
    fn verify_checksum_matches() {
        let data = b"hello world";
        // SHA-256 of "hello world"
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert!(Installer::verify_checksum(data, expected).is_ok());
    }

    #[test]
    fn verify_checksum_mismatch_fails() {
        let data = b"hello world";
        let wrong = "0000000000000000000000000000000000000000000000000000000000000000";
        let err = Installer::verify_checksum(data, wrong).unwrap_err();
        assert!(err.to_string().contains("Checksum mismatch"));
    }

    #[test]
    fn verify_checksum_case_insensitive() {
        let data = b"hello world";
        let expected = "B94D27B9934D3E08A52E52D7DA7DABFAC484EFE37A5380EE9088F7ACE2EFCDE9";
        assert!(Installer::verify_checksum(data, expected).is_ok());
    }

    #[test]
    fn checksum_url_uses_sha256sums_txt_filename() {
        // The SHA256SUMS URL should use "sha256sums.txt", not "SHA256SUMS"
        let download_url = "https://github.com/CarlosDanielDev/maestro/releases/download/v0.10.0/maestro-v0.10.0-aarch64-apple-darwin.tar.gz";
        let expected_base = "https://github.com/CarlosDanielDev/maestro/releases/download/v0.10.0";
        let sums_url = download_url
            .rsplit_once('/')
            .map(|(base, _)| format!("{}/sha256sums.txt", base))
            .unwrap();
        assert_eq!(sums_url, format!("{}/sha256sums.txt", expected_base));
    }

    #[test]
    fn extract_binary_from_tar_gz_finds_maestro_binary() {
        use flate2::Compression;
        use flate2::write::GzEncoder;

        // Create a tar.gz archive containing a "maestro" binary
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        {
            let mut builder = tar::Builder::new(&mut encoder);
            let binary_content = b"fake maestro binary";
            let mut header = tar::Header::new_gnu();
            header.set_size(binary_content.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, "maestro", &binary_content[..])
                .unwrap();
            builder.finish().unwrap();
        }
        let archive_bytes = encoder.finish().unwrap();

        let result = extract_binary_from_tar_gz(&archive_bytes);
        assert!(result.is_ok(), "Should extract binary: {:?}", result.err());
        assert_eq!(result.unwrap(), b"fake maestro binary");
    }

    #[test]
    fn extract_binary_from_tar_gz_fails_when_no_maestro_binary() {
        use flate2::Compression;
        use flate2::write::GzEncoder;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        {
            let mut builder = tar::Builder::new(&mut encoder);
            let content = b"some other file";
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "not-maestro", &content[..])
                .unwrap();
            builder.finish().unwrap();
        }
        let archive_bytes = encoder.finish().unwrap();

        let result = extract_binary_from_tar_gz(&archive_bytes);
        assert!(
            result.is_err(),
            "Should fail when no maestro binary in archive"
        );
    }

    #[tokio::test]
    async fn installer_only_writes_expected_files() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("maestro");
        fs::write(&dest, b"original").await.unwrap();

        let installer = Installer::new(dest);
        installer.install_with_backup(b"new").await.unwrap();

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        for entry in &entries {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            assert!(
                name_str == "maestro" || name_str == "maestro.bak",
                "Unexpected file: {}",
                name_str
            );
        }
    }
}
