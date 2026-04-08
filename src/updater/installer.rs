use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::fs;

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

    /// Download a binary from a URL and install it with backup.
    ///
    /// Validates the URL against trusted domains, enforces a size limit,
    /// and uses a 120-second download timeout.
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

        if let Some(len) = resp.content_length() {
            if len > crate::updater::MAX_DOWNLOAD_SIZE {
                anyhow::bail!(
                    "Binary too large: {} bytes (max {} bytes)",
                    len,
                    crate::updater::MAX_DOWNLOAD_SIZE
                );
            }
        }

        let bytes = resp.bytes().await?;
        self.install_with_backup(&bytes).await?;

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
