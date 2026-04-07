#![allow(dead_code)]
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;

/// Parsed semantic version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// Parse a version string, stripping leading 'v' and pre-release suffixes.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim_start_matches('v');
        let s = s.split('-').next().unwrap_or(s);
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(anyhow!("Invalid semver: expected X.Y.Z, got {:?}", s));
        }
        Ok(Self {
            major: parts[0].parse().context("Invalid major version")?,
            minor: parts[1].parse().context("Invalid minor version")?,
            patch: parts[2].parse().context("Invalid patch version")?,
        })
    }

    /// Returns true if `candidate` is strictly newer than `current`.
    pub fn is_newer_than(candidate: &str, current: &str) -> Result<bool> {
        let candidate = Self::parse(candidate)?;
        let current = Self::parse(current)?;
        Ok(candidate > current)
    }
}

/// Trait for checking available updates. Mockable.
#[async_trait]
pub trait UpdateChecker: Send + Sync {
    /// Check for a newer release. Returns `Some(ReleaseInfo)` if an upgrade is available.
    async fn check_for_update(&self) -> Result<Option<crate::updater::ReleaseInfo>>;
}

/// Parse the GitHub Releases API JSON response and return the latest stable tag name.
pub fn parse_releases_response(json: &str) -> Result<Option<String>> {
    let releases: Vec<serde_json::Value> =
        serde_json::from_str(json).context("Failed to parse releases JSON")?;
    let latest = releases
        .iter()
        .find(|r| {
            !r.get("prerelease")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .and_then(|r| r.get("tag_name"))
        .and_then(|t| t.as_str())
        .map(|s| s.trim_start_matches('v').to_string());
    Ok(latest)
}

/// Production implementation that checks GitHub Releases API.
pub struct GitHubReleaseChecker {
    repo: String,
}

impl GitHubReleaseChecker {
    pub fn new(repo: String) -> Self {
        Self { repo }
    }

    fn platform_asset_name() -> &'static str {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            "maestro-darwin-arm64"
        }
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        {
            "maestro-darwin-x86_64"
        }
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            "maestro-linux-x86_64"
        }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        {
            "maestro-linux-arm64"
        }
    }

    fn extract_download_url(resp: &serde_json::Value) -> Option<String> {
        let asset_name = Self::platform_asset_name();
        resp["assets"].as_array().and_then(|assets| {
            assets.iter().find_map(|a| {
                let name = a["name"].as_str()?;
                if name.contains(asset_name) {
                    a["browser_download_url"].as_str().map(|s| s.to_string())
                } else {
                    None
                }
            })
        })
    }
}

#[async_trait]
impl UpdateChecker for GitHubReleaseChecker {
    async fn check_for_update(&self) -> Result<Option<crate::updater::ReleaseInfo>> {
        let url = format!("https://api.github.com/repos/{}/releases/latest", self.repo);
        let client = reqwest::Client::new();
        let resp: serde_json::Value = client
            .get(&url)
            .header("User-Agent", concat!("maestro/", env!("CARGO_PKG_VERSION")))
            .header("Accept", "application/vnd.github+json")
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await?
            .json()
            .await?;

        let tag = resp["tag_name"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing tag_name in release response"))?;
        let version = tag.trim_start_matches('v').to_string();

        let current = env!("CARGO_PKG_VERSION");
        if !Version::is_newer_than(&version, current)? {
            return Ok(None);
        }

        let download_url = Self::extract_download_url(&resp).unwrap_or_default();

        Ok(Some(crate::updater::ReleaseInfo {
            tag: tag.to_string(),
            version,
            download_url,
        }))
    }
}

#[cfg(test)]
pub mod test_support {
    use super::*;
    use anyhow::anyhow;

    pub enum MockCheckerResponse {
        NewVersion(String),
        SameVersion,
        NetworkError,
        MalformedResponse,
    }

    pub struct MockUpdateChecker {
        pub response: MockCheckerResponse,
    }

    #[async_trait]
    impl UpdateChecker for MockUpdateChecker {
        async fn check_for_update(&self) -> Result<Option<crate::updater::ReleaseInfo>> {
            match &self.response {
                MockCheckerResponse::NewVersion(v) => Ok(Some(crate::updater::ReleaseInfo {
                    tag: format!("v{}", v),
                    version: v.clone(),
                    download_url: String::new(),
                })),
                MockCheckerResponse::SameVersion => Ok(None),
                MockCheckerResponse::NetworkError => Err(anyhow!(
                    "connection refused: failed to reach api.github.com"
                )),
                MockCheckerResponse::MalformedResponse => Err(anyhow!(
                    "Invalid semver: expected X.Y.Z, got \"not-a-version\""
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::*;

    #[test]
    fn version_comparison_newer_is_detected() {
        assert!(Version::is_newer_than("0.6.0", "0.5.0").unwrap());
    }

    #[test]
    fn version_comparison_same_version_is_not_an_upgrade() {
        assert!(!Version::is_newer_than("0.5.0", "0.5.0").unwrap());
    }

    #[test]
    fn version_comparison_older_remote_is_not_an_upgrade() {
        assert!(!Version::is_newer_than("0.5.9", "0.6.0").unwrap());
    }

    #[test]
    fn version_comparison_patch_increment_is_detected() {
        assert!(Version::is_newer_than("0.5.1", "0.5.0").unwrap());
    }

    #[test]
    fn version_comparison_major_increment_is_detected() {
        assert!(Version::is_newer_than("1.0.0", "0.5.0").unwrap());
    }

    #[test]
    fn version_parse_strips_leading_v_prefix() {
        let v = Version::parse("v0.6.0").unwrap();
        assert_eq!(
            v,
            Version {
                major: 0,
                minor: 6,
                patch: 0
            }
        );
    }

    #[test]
    fn version_parse_rejects_empty_string() {
        assert!(Version::parse("").is_err());
    }

    #[test]
    fn version_parse_rejects_plain_text() {
        assert!(Version::parse("not-a-version").is_err());
    }

    #[test]
    fn version_parse_rejects_two_part_version() {
        assert!(Version::parse("1.2").is_err());
    }

    #[test]
    fn version_parse_rejects_four_part_version() {
        assert!(Version::parse("1.2.3.4").is_err());
    }

    #[test]
    fn version_parse_handles_pre_release_suffix_without_panic() {
        let result = Version::parse("0.6.0-rc.1");
        match result {
            Ok(v) => assert_eq!(
                v,
                Version {
                    major: 0,
                    minor: 6,
                    patch: 0
                }
            ),
            Err(_) => { /* acceptable */ }
        }
    }

    #[tokio::test]
    async fn check_for_update_returns_some_when_newer_version_available() {
        let checker = MockUpdateChecker {
            response: MockCheckerResponse::NewVersion("0.6.0".into()),
        };
        let result = checker.check_for_update().await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().version, "0.6.0");
    }

    #[tokio::test]
    async fn check_for_update_returns_none_when_same_version() {
        let checker = MockUpdateChecker {
            response: MockCheckerResponse::SameVersion,
        };
        let result = checker.check_for_update().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn check_for_update_propagates_network_error() {
        let checker = MockUpdateChecker {
            response: MockCheckerResponse::NetworkError,
        };
        let result = checker.check_for_update().await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("connection refused"));
    }

    #[tokio::test]
    async fn check_for_update_returns_error_on_malformed_version() {
        let checker = MockUpdateChecker {
            response: MockCheckerResponse::MalformedResponse,
        };
        let result = checker.check_for_update().await;
        assert!(result.is_err());
    }

    #[test]
    fn github_releases_json_parse_extracts_tag_name() {
        let json = r#"[{"tag_name":"v0.6.0","prerelease":false}]"#;
        let result = parse_releases_response(json).unwrap();
        assert_eq!(result, Some("0.6.0".to_string()));
    }

    #[test]
    fn github_releases_json_parse_skips_prerelease() {
        let json = r#"[
            {"tag_name":"v0.6.0-rc.1","prerelease":true},
            {"tag_name":"v0.5.9","prerelease":false}
        ]"#;
        let result = parse_releases_response(json).unwrap();
        assert_eq!(result, Some("0.5.9".to_string()));
    }

    #[test]
    fn github_releases_json_parse_empty_list_returns_none() {
        let json = r#"[]"#;
        let result = parse_releases_response(json).unwrap();
        assert_eq!(result, None);
    }
}
