use crate::updater::checker::{GitHubReleaseChecker, Version};
use crate::updater::error::UpdateError;
use crate::updater::replace::{AtomicBinaryReplacer, BinaryReplacer};
use crate::updater::signature::verify_sums_signature;
use anyhow::{Context, Result};
use minisign_verify::PublicKey;
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

/// One entry of the (signature-verified) SHA256SUMS file.
pub(crate) struct SumsEntry {
    pub hash: String,
    /// Filename exactly as written in the sums file (may carry a `./` prefix,
    /// as produced by `sha256sum ./*.tar.gz` in `release.yml`).
    pub raw_name: String,
}

/// Parse `TRUSTED_RELEASE_PUBLIC_KEYS`. Malformed entries are logged and
/// skipped, so a broken constant fails closed (no trusted keys → every
/// install is refused) instead of panicking at startup.
fn trusted_release_keys() -> Vec<PublicKey> {
    crate::updater::TRUSTED_RELEASE_PUBLIC_KEYS
        .iter()
        .filter_map(|b64| match PublicKey::from_base64(b64) {
            Ok(pk) => Some(pk),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "malformed TRUSTED_RELEASE_PUBLIC_KEYS entry skipped — update installs will fail closed"
                );
                None
            }
        })
        .collect()
}

pub struct Installer {
    pub dest_path: PathBuf,
    // Arc<dyn> so the replacer can move into spawn_blocking ('static + Send + Sync).
    replacer: Arc<dyn BinaryReplacer>,
    /// Keys trusted to sign `sha256sums.txt`; sourced from the compile-time
    /// `TRUSTED_RELEASE_PUBLIC_KEYS` constant, never from the release channel.
    trusted_keys: Vec<PublicKey>,
    /// URL trust policy; injectable so tests can drive a loopback release
    /// server through the full download path.
    trust_check: fn(&str) -> bool,
}

impl Installer {
    pub fn new(dest_path: PathBuf) -> Self {
        Self {
            dest_path,
            replacer: Arc::new(AtomicBinaryReplacer::new()),
            trusted_keys: trusted_release_keys(),
            trust_check: crate::updater::is_trusted_download_url,
        }
    }

    /// Test-only constructor that injects a custom `BinaryReplacer`.
    #[cfg(test)]
    pub(crate) fn with_replacer(dest_path: PathBuf, replacer: Arc<dyn BinaryReplacer>) -> Self {
        Self {
            dest_path,
            replacer,
            trusted_keys: trusted_release_keys(),
            trust_check: crate::updater::is_trusted_download_url,
        }
    }

    /// Test-only constructor injecting trusted keys and a loopback URL trust
    /// check for local release-server tests.
    #[cfg(test)]
    pub(crate) fn with_test_config(
        dest_path: PathBuf,
        replacer: Arc<dyn BinaryReplacer>,
        trusted_keys: Vec<PublicKey>,
        trust_check: fn(&str) -> bool,
    ) -> Self {
        Self {
            dest_path,
            replacer,
            trusted_keys,
            trust_check,
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
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    async fn fetch_text(client: &reqwest::Client, url: &str) -> Result<String> {
        let resp = client
            .get(url)
            .header("User-Agent", concat!("maestro/", env!("CARGO_PKG_VERSION")))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("HTTP {} for {}", resp.status(), url);
        }
        Ok(resp.text().await?)
    }

    /// Fetch the checksums file and its minisign signature from the same
    /// release directory. A missing or unreachable signature asset is
    /// `SignatureInvalid` — never a silent pass.
    async fn fetch_sums_and_signature(
        client: &reqwest::Client,
        download_url: &str,
    ) -> Result<(String, String), UpdateError> {
        let base = download_url
            .rsplit_once('/')
            .map(|(base, _)| base)
            .ok_or_else(|| UpdateError::Internal("cannot derive release base URL".to_string()))?;

        let sums = Self::fetch_text(client, &format!("{base}/sha256sums.txt"))
            .await
            .map_err(|e| UpdateError::NetworkInterrupted {
                message: format!("fetching SHA256SUMS: {e}"),
            })?;

        let sig_url = format!("{base}/sha256sums.txt.minisig");
        let signature = Self::fetch_text(client, &sig_url).await.map_err(|e| {
            UpdateError::SignatureInvalid {
                reason: format!("release signature missing or unreachable at {sig_url}: {e}"),
            }
        })?;

        Ok((sums, signature))
    }

    /// Extract the sums entry for `asset_name`, matching on the bare filename
    /// so the `./` prefix `sha256sum ./*.tar.gz` writes does not break lookup.
    fn find_sums_entry(sums: &str, asset_name: &str) -> Result<SumsEntry> {
        for line in sums.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 2 {
                let file_name = std::path::Path::new(parts[1])
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if file_name == asset_name {
                    return Ok(SumsEntry {
                        hash: parts[0].to_string(),
                        raw_name: parts[1].to_string(),
                    });
                }
            }
        }
        anyhow::bail!("No checksum found for {asset_name} in SHA256SUMS")
    }

    /// Release tag embedded in a sums-file entry, e.g.
    /// `./maestro-v0.10.0-aarch64-apple-darwin.tar.gz` → `0.10.0`.
    fn candidate_version_from_raw_name(raw_name: &str) -> Option<String> {
        let file_name = std::path::Path::new(raw_name).file_name()?.to_str()?;
        let suffix = GitHubReleaseChecker::platform_asset_suffix();
        file_name
            .strip_prefix("maestro-v")?
            .strip_suffix(&format!("-{suffix}.tar.gz"))
            .map(str::to_string)
    }

    /// Download a binary from a URL and install it with backup.
    ///
    /// Validates the URL against trusted domains, enforces a size limit,
    /// verifies the minisign signature over `sha256sums.txt` against
    /// compile-time trusted keys before trusting its contents, verifies the
    /// SHA-256 checksum, re-checks at install time that the candidate version
    /// is strictly newer than the running binary (rollback protection), and
    /// uses a 120-second download timeout.
    /// Returns the backup path on success.
    pub async fn download_and_install(&self, download_url: &str) -> Result<PathBuf, UpdateError> {
        if !(self.trust_check)(download_url) {
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
        let sums_fut = async { Self::fetch_sums_and_signature(&client, download_url).await };
        let (bytes, (sums, signature)) = tokio::try_join!(download_fut, sums_fut)?;

        // The sums file is untrusted until its minisign signature verifies
        // against the compile-time trusted keys — which never travel through
        // the release channel being verified. Fail closed: missing or
        // invalid signature → no checksum comparison, no install.
        verify_sums_signature(sums.as_bytes(), &signature, &self.trusted_keys)?;

        let entry = Self::find_sums_entry(&sums, asset_name).map_err(|e| {
            UpdateError::NetworkInterrupted {
                message: e.to_string(),
            }
        })?;

        let actual = Self::compute_sha256_hex(&bytes);
        if actual != entry.hash.to_lowercase() {
            return Err(UpdateError::ChecksumMismatch {
                expected: entry.hash,
                actual,
            });
        }

        // Install-time rollback check: the version recorded in the verified
        // sums must be strictly newer than the running binary.
        let candidate =
            Self::candidate_version_from_raw_name(&entry.raw_name).ok_or_else(|| {
                UpdateError::Internal(format!(
                    "cannot derive release version from sums entry {}",
                    entry.raw_name
                ))
            })?;
        let current = env!("CARGO_PKG_VERSION");
        if !Version::is_newer_than(&candidate, current)
            .map_err(|e| UpdateError::Internal(format!("candidate version: {e}")))?
        {
            return Err(UpdateError::DowngradeRejected {
                current: current.to_string(),
                candidate,
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

#[cfg(test)]
#[path = "installer_signature_tests.rs"]
mod signature_tests;
