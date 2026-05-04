//! Integration tests for `cmd_init_inner`. All tests use a
//! `FakeProjectDetector` plus a `tempfile::TempDir`; nothing reads from
//! the real project root.

use std::fs;
use tempfile::tempdir;

use crate::commands::{InitOptions, InitPrompter, cmd_init_inner, cmd_init_inner_with_options};
use crate::init::{DetectedStack, FakeProjectDetector};
use crate::provider::types::ProviderKind;

#[test]
fn cmd_init_writes_file_on_fresh_init() {
    let dir = tempdir().unwrap();
    let detector = FakeProjectDetector::new(vec![DetectedStack::Rust]);
    let code = cmd_init_inner(false, dir.path(), &detector).expect("fresh init ok");
    assert_eq!(code, 0);
    let target = dir.path().join("maestro.toml");
    assert!(target.exists(), "maestro.toml must be written");
    let body = fs::read_to_string(&target).unwrap();
    assert!(body.contains("language = \"rust\""), "{body}");
    assert!(body.contains("build_command = \"cargo build\""), "{body}");
}

#[test]
fn cmd_init_exits_nonzero_when_file_exists_without_reset() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("maestro.toml"), "[project]\n").unwrap();
    let detector = FakeProjectDetector::new(vec![DetectedStack::Rust]);
    let code = cmd_init_inner(false, dir.path(), &detector).expect("returns code");
    assert_eq!(
        code, 2,
        "must exit non-zero when file exists without --reset"
    );
}

#[test]
fn cmd_init_reset_preserves_custom_key() {
    let dir = tempdir().unwrap();
    let original = "[project]\n\
                    repo = \"owner/repo\"\n\
                    custom_key = \"my-secret\"\n\
                    [sessions]\n\
                    [budget]\n\
                    per_session_usd = 5.0\n\
                    total_usd = 50.0\n\
                    alert_threshold_pct = 80\n\
                    [notifications]\n";
    fs::write(dir.path().join("maestro.toml"), original).unwrap();

    let detector = FakeProjectDetector::new(vec![DetectedStack::Node]);
    let code = cmd_init_inner(true, dir.path(), &detector).expect("reset ok");
    assert_eq!(code, 0);

    let body = fs::read_to_string(dir.path().join("maestro.toml")).unwrap();
    assert!(body.contains("custom_key = \"my-secret\""), "{body}");
    assert!(body.contains("language = \"node\""), "{body}");
    assert!(body.contains("test_command = \"npm test\""), "{body}");
    assert!(!body.contains("\"cargo build\""), "{body}");
}

#[test]
fn cmd_init_empty_markers_writes_generic_template_exit_zero() {
    let dir = tempdir().unwrap();
    let detector = FakeProjectDetector::new(vec![]);
    let code = cmd_init_inner(false, dir.path(), &detector).expect("fresh ok");
    assert_eq!(code, 0);
    let body = fs::read_to_string(dir.path().join("maestro.toml")).unwrap();
    assert!(!body.contains("\"cargo build\""), "{body}");
    assert!(!body.contains("\"npm run build\""), "{body}");
    assert!(!body.contains("\"go build ./...\""), "{body}");
    assert!(!body.contains("\"pytest\""), "{body}");
}

#[test]
fn cmd_init_reset_adds_missing_keys_to_existing_file() {
    let dir = tempdir().unwrap();
    let original = "[project]\n\
                    repo = \"owner/repo\"\n\
                    [sessions]\n\
                    [budget]\n\
                    per_session_usd = 5.0\n\
                    total_usd = 50.0\n\
                    alert_threshold_pct = 80\n\
                    [notifications]\n";
    fs::write(dir.path().join("maestro.toml"), original).unwrap();

    let detector = FakeProjectDetector::new(vec![DetectedStack::Go]);
    let code = cmd_init_inner(true, dir.path(), &detector).expect("reset ok");
    assert_eq!(code, 0);

    let body = fs::read_to_string(dir.path().join("maestro.toml")).unwrap();
    assert!(
        body.contains("build_command = \"go build ./...\""),
        "{body}"
    );
    assert!(body.contains("test_command = \"go test ./...\""), "{body}");
}

struct ScriptedPrompter {
    provider: ProviderKind,
    organizations: Vec<String>,
    projects: Vec<String>,
}

impl ScriptedPrompter {
    fn new(provider: ProviderKind, organizations: &[&str], projects: &[&str]) -> Self {
        Self {
            provider,
            organizations: organizations
                .iter()
                .map(|s| (*s).to_string())
                .rev()
                .collect(),
            projects: projects.iter().map(|s| (*s).to_string()).rev().collect(),
        }
    }
}

impl InitPrompter for ScriptedPrompter {
    fn choose_provider(&mut self, _detected: ProviderKind) -> anyhow::Result<ProviderKind> {
        Ok(self.provider)
    }

    fn prompt_azdo_organization(&mut self) -> anyhow::Result<String> {
        self.organizations
            .pop()
            .ok_or_else(|| anyhow::anyhow!("no scripted organization"))
    }

    fn prompt_azdo_project(&mut self) -> anyhow::Result<String> {
        self.projects
            .pop()
            .ok_or_else(|| anyhow::anyhow!("no scripted project"))
    }
}

#[test]
fn cmd_init_non_interactive_writes_github_provider_default() {
    let dir = tempdir().unwrap();
    let detector = FakeProjectDetector::new(vec![DetectedStack::Rust]);

    let code =
        cmd_init_inner_with_options(false, dir.path(), &detector, InitOptions::non_interactive())
            .expect("non-interactive init ok");

    assert_eq!(code, 0);
    let body = fs::read_to_string(dir.path().join("maestro.toml")).unwrap();
    assert!(body.contains("[provider]"), "{body}");
    assert!(body.contains("kind = \"github\""), "{body}");
    assert!(!body.contains("[experimental]"), "{body}");
}

#[test]
fn cmd_init_detects_azdo_remote_and_reprompts_invalid_organization() {
    let dir = tempdir().unwrap();
    let detector = FakeProjectDetector::new(vec![DetectedStack::Rust]);
    let mut prompter = ScriptedPrompter::new(
        ProviderKind::AzureDevops,
        &["https://example.com/org", "https://dev.azure.com/MyOrg"],
        &["", "MyProject"],
    );

    let code = cmd_init_inner_with_options(
        false,
        dir.path(),
        &detector,
        InitOptions::interactive(
            Some("git@ssh.dev.azure.com:v3/MyOrg/MyProject/MyRepo"),
            &mut prompter,
        ),
    )
    .expect("interactive azdo init ok");

    assert_eq!(code, 0);
    let body = fs::read_to_string(dir.path().join("maestro.toml")).unwrap();
    assert!(body.contains("[provider]"), "{body}");
    assert!(body.contains("kind = \"azure_devops\""), "{body}");
    assert!(
        body.contains("organization = \"https://dev.azure.com/MyOrg\""),
        "{body}"
    );
    assert!(body.contains("az_project = \"MyProject\""), "{body}");
    assert!(body.contains("[experimental]"), "{body}");
    assert!(body.contains("azure_devops = true"), "{body}");

    let loaded = crate::config::Config::load(&dir.path().join("maestro.toml")).unwrap();
    assert_eq!(loaded.provider.kind, ProviderKind::AzureDevops);
    assert!(loaded.experimental.azure_devops);
}
