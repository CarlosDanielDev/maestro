pub mod checker;
pub mod installer;
pub mod restart;

/// GitHub repository for version checks and binary downloads.
pub const GITHUB_REPO: &str = "CarlosDanielDev/maestro";

/// Trusted domains for binary downloads.
const TRUSTED_DOWNLOAD_HOSTS: &[&str] = &["github.com", "objects.githubusercontent.com"];

/// Maximum binary download size (200 MB).
pub const MAX_DOWNLOAD_SIZE: u64 = 200 * 1024 * 1024;

/// Validate that a download URL points to a trusted GitHub domain over HTTPS.
pub fn is_trusted_download_url(url: &str) -> bool {
    // Only allow HTTPS URLs from trusted GitHub domains
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or("");
    TRUSTED_DOWNLOAD_HOSTS
        .iter()
        .any(|&trusted| host == trusted)
}

/// State machine for the upgrade notification in the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradeState {
    /// No upgrade notification visible.
    Hidden,
    /// Banner shown with available release info.
    Available(ReleaseInfo),
    /// Download in progress.
    Downloading { version: String },
    /// Binary replaced successfully, asking for restart confirmation.
    ReadyToRestart {
        version: String,
        backup_path: String,
    },
    /// Upgrade failed with error message.
    Failed(String),
}

/// Information about an available release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub tag: String,
    pub version: String,
    pub download_url: String,
}

impl UpgradeState {
    pub fn is_visible(&self) -> bool {
        !matches!(self, Self::Hidden)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_state_starts_hidden() {
        let state = UpgradeState::Hidden;
        assert!(!state.is_visible());
    }

    #[test]
    fn upgrade_state_available_is_visible() {
        let state = UpgradeState::Available(ReleaseInfo {
            tag: "v0.6.0".into(),
            version: "0.6.0".into(),
            download_url: "https://example.com/release".into(),
        });
        assert!(state.is_visible());
    }

    #[test]
    fn upgrade_state_downloading_is_visible() {
        let state = UpgradeState::Downloading {
            version: "0.6.0".into(),
        };
        assert!(state.is_visible());
    }

    #[test]
    fn upgrade_state_ready_to_restart_is_visible() {
        let state = UpgradeState::ReadyToRestart {
            version: "0.6.0".into(),
            backup_path: "/tmp/maestro.bak".into(),
        };
        assert!(state.is_visible());
    }

    #[test]
    fn upgrade_state_failed_is_visible() {
        let state = UpgradeState::Failed("network error".into());
        assert!(state.is_visible());
    }

    #[test]
    fn trusted_url_accepts_github_releases() {
        assert!(is_trusted_download_url(
            "https://github.com/CarlosDanielDev/maestro/releases/download/v0.6.0/maestro-darwin-arm64"
        ));
    }

    #[test]
    fn trusted_url_accepts_githubusercontent() {
        assert!(is_trusted_download_url(
            "https://objects.githubusercontent.com/some/path/to/binary"
        ));
    }

    #[test]
    fn trusted_url_rejects_http() {
        assert!(!is_trusted_download_url(
            "http://github.com/CarlosDanielDev/maestro/releases/download/v0.6.0/binary"
        ));
    }

    #[test]
    fn trusted_url_rejects_arbitrary_domain() {
        assert!(!is_trusted_download_url("https://evil.com/maestro"));
    }

    #[test]
    fn trusted_url_rejects_empty_string() {
        assert!(!is_trusted_download_url(""));
    }
}
