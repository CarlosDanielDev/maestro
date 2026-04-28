use crate::updater::error::UpdateError;
use crate::updater::replace::{AtomicBinaryReplacer, BinaryReplacer};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
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
    // Arc<dyn> so the replacer can move into spawn_blocking ('static + Send + Sync).
    replacer: Arc<dyn BinaryReplacer>,
}

impl Installer {
    pub fn new(dest_path: PathBuf) -> Self {
        Self {
            dest_path,
            replacer: Arc::new(AtomicBinaryReplacer::new()),
        }
    }

    /// Test-only constructor that injects a custom `BinaryReplacer`.
    #[cfg(test)]
    pub(crate) fn with_replacer(dest_path: PathBuf, replacer: Arc<dyn BinaryReplacer>) -> Self {
        Self {
            dest_path,
            replacer,
        }
    }

    /// Write bytes to a staging area (same dir, `.tmp` suffix).
    #[allow(dead_code)] // Reason: staging step retained for two-phase callers.
    pub async fn write_to_staging(&self, bytes: &[u8]) -> Result<PathBuf> {
        let staging = self.dest_path.with_extension("tmp");
        fs::write(&staging, bytes)
            .await
            .context("Failed to write staging file")?;
        Ok(staging)
    }

    /// Replace the binary at `dest_path` with `new_bytes`, delegating to the
    /// injected `BinaryReplacer`. Returns the path of the backup of the
    /// original binary on success.
    pub async fn install_with_backup(&self, new_bytes: Vec<u8>) -> Result<PathBuf, UpdateError> {
        let target = self.dest_path.clone();
        let replacer = self.replacer.clone();

        let outcome = tokio::task::spawn_blocking(move || replacer.replace(&target, &new_bytes))
            .await
            .map_err(|e| UpdateError::Internal(format!("replace task panicked: {e}")))??;
        Ok(outcome.backup_path)
    }

    fn compute_sha256_hex(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
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
    /// Returns the backup path on success.
    pub async fn download_and_install(&self, download_url: &str) -> Result<PathBuf, UpdateError> {
        if !crate::updater::is_trusted_download_url(download_url) {
            return Err(UpdateError::NetworkInterrupted {
                message: "untrusted download URL: only HTTPS from github.com is allowed"
                    .to_string(),
            });
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| UpdateError::Internal(format!("building reqwest client: {e}")))?;

        let resp = client
            .get(download_url)
            .header("User-Agent", concat!("maestro/", env!("CARGO_PKG_VERSION")))
            .send()
            .await
            .map_err(|e| UpdateError::NetworkInterrupted {
                message: format!("HTTP request failed: {e}"),
            })?;

        if !resp.status().is_success() {
            return Err(UpdateError::NetworkInterrupted {
                message: format!("HTTP {}", resp.status()),
            });
        }

        if let Some(len) = resp.content_length()
            && len > crate::updater::MAX_DOWNLOAD_SIZE
        {
            return Err(UpdateError::NetworkInterrupted {
                message: format!(
                    "binary too large: {} bytes (max {} bytes)",
                    len,
                    crate::updater::MAX_DOWNLOAD_SIZE
                ),
            });
        }

        let asset_name = download_url
            .rsplit_once('/')
            .map(|(_, name)| name)
            .unwrap_or("maestro");

        let download_fut = async {
            resp.bytes()
                .await
                .map_err(|e| UpdateError::NetworkInterrupted {
                    message: format!("download interrupted: {e}"),
                })
        };
        let checksum_fut = async {
            Self::fetch_expected_checksum(&client, download_url, asset_name)
                .await
                .map_err(|e| UpdateError::NetworkInterrupted {
                    message: format!("fetching SHA256SUMS: {e}"),
                })
        };
        let (bytes, expected_hash) = tokio::try_join!(download_fut, checksum_fut)?;

        let actual = Self::compute_sha256_hex(&bytes);
        if actual != expected_hash.to_lowercase() {
            return Err(UpdateError::ChecksumMismatch {
                expected: expected_hash,
                actual,
            });
        }

        let binary_bytes = if asset_name.ends_with(".tar.gz") {
            extract_binary_from_tar_gz(&bytes).map_err(|e| UpdateError::Internal(e.to_string()))?
        } else {
            bytes.to_vec()
        };

        self.install_with_backup(binary_bytes).await
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
#[path = "installer_tests.rs"]
mod tests;
