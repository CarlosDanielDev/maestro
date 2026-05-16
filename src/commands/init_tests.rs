use super::*;
use crate::init::{DetectedStack, FakeProjectDetector};
use tempfile::TempDir;

#[test]
fn init_creates_templates_manifest_toml() {
    // Verifies cmd_init_inner wires the scaffold step in after the
    // maestro.toml write succeeds.
    let dir = TempDir::new().expect("create tempdir");
    let detector = FakeProjectDetector::new(vec![DetectedStack::Rust]);
    let code = cmd_init_inner(false, dir.path(), &detector).expect("cmd_init_inner");
    assert_eq!(code, 0);
    let manifest = dir.path().join(".maestro/templates/manifest.toml");
    assert!(
        manifest.exists(),
        ".maestro/templates/manifest.toml must be created by init"
    );
}

#[test]
fn init_manifest_toml_matches_canonical_bytes() {
    let dir = TempDir::new().expect("create tempdir");
    let detector = FakeProjectDetector::new(vec![DetectedStack::Rust]);
    let code = cmd_init_inner(false, dir.path(), &detector).expect("cmd_init_inner");
    assert_eq!(code, 0);

    let written = std::fs::read(dir.path().join(".maestro/templates/manifest.toml"))
        .expect("manifest.toml must be readable after init");
    let canonical = include_bytes!("../../template/.maestro/templates/manifest.toml");
    assert_eq!(
        written.as_slice(),
        canonical.as_slice(),
        "scaffolded manifest.toml must be byte-equal to embedded source"
    );
}

#[test]
fn init_creates_all_template_files() {
    let dir = TempDir::new().expect("create tempdir");
    let detector = FakeProjectDetector::new(vec![DetectedStack::Rust]);
    let code = cmd_init_inner(false, dir.path(), &detector).expect("cmd_init_inner");
    assert_eq!(code, 0);

    for rel in crate::init::scaffold::template_relative_paths() {
        assert!(
            dir.path().join(&rel).exists(),
            "expected scaffolded file: {}",
            rel.display()
        );
    }
}

#[test]
fn first_origin_remote_url_prefers_fetch_url() {
    let remote = "upstream\thttps://example.com/upstream.git (fetch)\n\
                  origin\tgit@github.com:owner/repo.git (push)\n\
                  origin\thttps://github.com/owner/repo.git (fetch)\n";
    assert_eq!(
        first_origin_remote_url(remote).as_deref(),
        Some("https://github.com/owner/repo.git")
    );
}

#[test]
fn validate_azure_devops_org_accepts_supported_forms() {
    assert!(validate_azure_devops_organization_url(
        "https://dev.azure.com/MyOrg"
    ));
    assert!(validate_azure_devops_organization_url(
        "https://MyOrg.visualstudio.com"
    ));
}

#[test]
fn validate_azure_devops_org_rejects_invalid_forms() {
    for input in [
        "http://dev.azure.com/MyOrg",
        "https://dev.azure.com/",
        "https://dev.azure.com/MyOrg/Project",
        "https://MyOrg@dev.azure.com/MyOrg",
        "https://example.com/MyOrg",
        "dev.azure.com/MyOrg",
        "",
    ] {
        assert!(
            !validate_azure_devops_organization_url(input),
            "accepted {input:?}"
        );
    }
}
