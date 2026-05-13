//! Integration tests for the template render engine.
//!
//! End-to-end render path: tempdir manifest + command files, real filesystem,
//! provider-rules trait injected via a fake.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::path::Path;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::agent_provider::AgentProvider;
use crate::agent_provider::types::{
    AgentError, AgentHealthCheck, AgentOutputFormat, AgentProviderEvent, AgentProviderId,
    AgentProviderKind, AgentRequest, AgentRunResult, ParserBinding,
};
use crate::templates::test_fakes::FakeRules;
use crate::templates::{TemplateError, TemplateProviderRules, render_command_in};

const MANIFEST_TOML: &str = r#"
[meta]
version = 1
description = "integration test manifest"

[placeholders.SUBAGENT_LIST]
description = "list subagents"
required_args = []

[placeholders.INCLUDE]
description = "include fragment"
required_args = ["path"]

[providers.claude]
display_name = "Claude Code"
"#;

static FAKE_RULES: FakeRules = FakeRules;

struct FakeProvider;

#[async_trait]
impl AgentProvider for FakeProvider {
    fn id(&self) -> &str {
        "fake"
    }
    fn kind(&self) -> AgentProviderKind {
        AgentProviderKind::Http
    }
    fn parser_binding(&self) -> ParserBinding {
        ParserBinding {
            name: "fake".to_string(),
            output_format: AgentOutputFormat::StreamJson,
        }
    }
    async fn health_check(&self) -> Result<AgentHealthCheck, AgentError> {
        Ok(AgentHealthCheck {
            provider_id: AgentProviderId::new(self.id()),
            available: true,
            version: None,
            message: "ok".to_string(),
        })
    }
    async fn run(
        &self,
        _request: AgentRequest,
        _events: mpsc::UnboundedSender<AgentProviderEvent>,
        _cancel: CancellationToken,
    ) -> Result<AgentRunResult, AgentError> {
        Ok(AgentRunResult { exit_code: None })
    }
    fn template_rules(&self) -> &'static dyn TemplateProviderRules {
        &FAKE_RULES
    }
}

fn setup_fixture(dir: &Path) {
    std::fs::write(dir.join("manifest.toml"), MANIFEST_TOML).expect("write manifest");
    std::fs::create_dir_all(dir.join("commands")).expect("commands dir");
    std::fs::write(
        dir.join("commands").join("hello.md"),
        "Hello from {{SUBAGENT_LIST}}\n",
    )
    .expect("write hello");
    std::fs::write(
        dir.join("commands").join("with_include.md"),
        "Start\n{{INCLUDE path=\"core/fragment.md\"}}\nEnd\n",
    )
    .expect("write with_include");
}

#[test]
fn render_command_renders_subagent_list_through_fake_rules() {
    let dir = tempfile::tempdir().expect("tempdir");
    setup_fixture(dir.path());
    let out = render_command_in(dir.path(), &FakeProvider, "hello").expect("render ok");
    assert_eq!(out, "Hello from [SUBAGENT_LIST]\n");
}

#[test]
fn render_command_with_unknown_placeholder_returns_err() {
    let dir = tempfile::tempdir().expect("tempdir");
    setup_fixture(dir.path());
    std::fs::write(
        dir.path().join("commands").join("bad.md"),
        "Oops {{UNKNOWN_TOKEN}}\n",
    )
    .expect("write bad");
    let result = render_command_in(dir.path(), &FakeProvider, "bad");
    match result {
        Err(TemplateError::UnknownPlaceholder { name, .. }) => {
            assert_eq!(name, "UNKNOWN_TOKEN");
        }
        other => panic!("expected UnknownPlaceholder, got {other:?}"),
    }
}

#[test]
fn render_command_missing_returns_file_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    setup_fixture(dir.path());
    let result = render_command_in(dir.path(), &FakeProvider, "no_such_command");
    assert!(matches!(result, Err(TemplateError::FileMissing { .. })));
}

#[test]
fn render_command_missing_manifest_returns_manifest_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("commands")).expect("commands dir");
    std::fs::write(
        dir.path().join("commands").join("hello.md"),
        "no manifest here",
    )
    .expect("write hello");
    let result = render_command_in(dir.path(), &FakeProvider, "hello");
    assert!(matches!(result, Err(TemplateError::ManifestMissing { .. })));
}

#[test]
fn render_command_empty_placeholders_table_still_rejects_unknown_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("manifest.toml"),
        "[meta]\nversion = 1\n[placeholders]\n",
    )
    .expect("write manifest");
    std::fs::create_dir_all(dir.path().join("commands")).expect("commands dir");
    std::fs::write(
        dir.path().join("commands").join("bad.md"),
        "{{TOTALLY_UNKNOWN}}",
    )
    .expect("write bad");
    let result = render_command_in(dir.path(), &FakeProvider, "bad");
    match result {
        Err(TemplateError::UnknownPlaceholder { name, .. }) => {
            assert_eq!(name, "TOTALLY_UNKNOWN");
        }
        other => panic!("expected UnknownPlaceholder, got {other:?}"),
    }
}

#[test]
fn render_command_include_uses_fake_rules_not_filesystem() {
    let dir = tempfile::tempdir().expect("tempdir");
    setup_fixture(dir.path());
    let out = render_command_in(dir.path(), &FakeProvider, "with_include").expect("render ok");
    assert_eq!(out, "Start\n[INCLUDE path=core/fragment.md]\nEnd\n");
}

#[test]
fn render_command_rejects_path_traversal_in_command_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    setup_fixture(dir.path());
    let result = render_command_in(dir.path(), &FakeProvider, "../../../etc/passwd");
    assert!(matches!(result, Err(TemplateError::SandboxEscape { .. })));
}

#[test]
fn render_command_rejects_absolute_command_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    setup_fixture(dir.path());
    let result = render_command_in(dir.path(), &FakeProvider, "/etc/passwd");
    assert!(matches!(result, Err(TemplateError::SandboxEscape { .. })));
}

#[test]
fn render_command_rejects_path_separator_in_command_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    setup_fixture(dir.path());
    let result = render_command_in(dir.path(), &FakeProvider, "foo/bar");
    assert!(matches!(result, Err(TemplateError::SandboxEscape { .. })));
}

#[test]
fn render_command_rejects_empty_command_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    setup_fixture(dir.path());
    let result = render_command_in(dir.path(), &FakeProvider, "");
    assert!(matches!(result, Err(TemplateError::SandboxEscape { .. })));
}

#[test]
fn render_command_empty_template_file_renders_empty_string() {
    let dir = tempfile::tempdir().expect("tempdir");
    setup_fixture(dir.path());
    std::fs::write(dir.path().join("commands").join("empty.md"), "").expect("write empty");
    let out = render_command_in(dir.path(), &FakeProvider, "empty").expect("render ok");
    assert_eq!(out, "");
}
