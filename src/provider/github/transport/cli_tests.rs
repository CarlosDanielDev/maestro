use super::*;
use crate::provider::github::gh_argv;

// ── with_repo argv-injection guard ────────────────────────────────

#[test]
fn with_repo_accepts_owner_slash_repo() {
    let c = GhCliClient::new().with_repo("CarlosDanielDev/maestro".into());
    assert!(c.is_ok());
}

#[test]
fn from_config_repo_injects_repo_into_list_open_prs_argv() {
    let c = GhCliClient::from_config_repo(Some("CarlosDanielDev/maestro".into()));
    let argv = gh_argv::build_list_open_prs_argv("number", c.repo_arg());
    assert!(
        argv.windows(2)
            .any(|w| w == ["--repo", "CarlosDanielDev/maestro"])
    );
}

#[test]
fn from_config_repo_falls_back_for_missing_or_invalid_repo() {
    assert_eq!(GhCliClient::from_config_repo(None).repo_arg(), None);
    assert_eq!(
        GhCliClient::from_config_repo(Some("not-owner-repo-shape".into())).repo_arg(),
        None
    );
}

#[test]
fn with_repo_rejects_dash_prefixed_value() {
    let c = GhCliClient::new().with_repo("--evil-flag=value".into());
    assert!(c.is_err(), "must reject argv-injection-shaped repo");
}

#[test]
fn with_repo_rejects_missing_slash() {
    let c = GhCliClient::new().with_repo("not-owner-repo-shape".into());
    assert!(c.is_err());
}

#[test]
fn with_repo_rejects_empty_owner() {
    let c = GhCliClient::new().with_repo("/repo".into());
    assert!(c.is_err());
}

#[test]
fn with_repo_rejects_empty_repo() {
    let c = GhCliClient::new().with_repo("owner/".into());
    assert!(c.is_err());
}
