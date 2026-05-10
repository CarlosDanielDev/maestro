use super::*;
use crate::orchestration::types::TeamOutput;
use crate::state::types::IssueNumber;
use std::fs;
use tempfile::tempdir;

fn make_loader_with_tempdirs() -> (tempfile::TempDir, tempfile::TempDir, Loader) {
    let user = tempdir().unwrap();
    let project = tempdir().unwrap();
    let loader = Loader::new(
        Some(user.path().to_path_buf()),
        Some(project.path().to_path_buf()),
    );
    (user, project, loader)
}

fn write_user_preset_file(user: &Path, name: &str, contents: &str) {
    fs::write(user.join(format!("{name}.toml")), contents).unwrap();
}

// -- list_teams ------------------------------------------------------

#[test]
fn list_teams_with_only_builtins_returns_five_entries() {
    let loader = Loader::new(None, None);
    let summaries = list_teams(&loader).unwrap();
    assert_eq!(summaries.len(), 5);
    for name in [
        "default-coder",
        "default-researcher",
        "default-triager",
        "default-reviewer",
        "default-docs",
    ] {
        assert!(
            summaries.iter().any(|s| s.name == name),
            "missing built-in {name}"
        );
    }
    for s in &summaries {
        assert_eq!(s.source_tier, TierLabel::BuiltIn);
    }
}

#[test]
fn list_teams_includes_user_tier_preset() {
    let (user, _project, loader) = make_loader_with_tempdirs();
    write_user_preset_file(
        user.path(),
        "cheap-coder",
        r#"extends = "default-coder"
implementer = "ollama"
"#,
    );
    let summaries = list_teams(&loader).unwrap();
    let entry = summaries
        .iter()
        .find(|s| s.name == "cheap-coder")
        .expect("cheap-coder must be present");
    assert_eq!(entry.source_tier, TierLabel::User);
}

#[test]
fn list_teams_json_is_valid_json_array() {
    let loader = Loader::new(None, None);
    let s = list_teams_json(&loader).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
    let arr = parsed.as_array().expect("top level must be array");
    assert_eq!(arr.len(), 5);
    for entry in arr {
        assert!(entry.get("name").is_some());
        assert!(entry.get("primitive").is_some());
        assert!(entry.get("source_tier").is_some());
    }
}

// -- explain ---------------------------------------------------------

#[test]
fn explain_returns_resolved_bindings_for_builtin() {
    let loader = Loader::new(None, None);
    let exp = explain(&loader, "default-coder").unwrap();
    assert_eq!(exp.name, "default-coder");
    assert_eq!(exp.source_tier, TierLabel::BuiltIn);
    assert!(!exp.bindings.is_empty());
}

#[test]
fn explain_unknown_team_returns_err() {
    let loader = Loader::new(None, None);
    let err = explain(&loader, "no-such-team").unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("not found"));
    assert!(msg.contains("no-such-team"));
}

#[test]
fn explain_json_round_trips_through_serde_json() {
    let loader = Loader::new(None, None);
    let s = explain_json(&loader, "default-coder").unwrap();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["name"], "default-coder");
    assert!(v["bindings"].is_array());
}

#[test]
fn explain_overridden_field_traces_to_child_preset() {
    let (user, _project, loader) = make_loader_with_tempdirs();
    write_user_preset_file(
        user.path(),
        "child",
        r#"extends = "default-coder"
implementer = "opencode"
"#,
    );
    let exp = explain(&loader, "child").unwrap();
    let imp = exp
        .bindings
        .iter()
        .find(|b| b.role == "implementer")
        .expect("implementer binding must exist");
    assert_eq!(imp.agent, "opencode");
}

// -- write_new_preset ------------------------------------------------

#[test]
fn write_new_preset_user_tier_writes_toml_under_override_dir() {
    let (user, _project, _loader) = make_loader_with_tempdirs();
    let opts = NewPresetOpts {
        name: "my-team".into(),
        extends: "default-coder".into(),
        tier: TeamTier::User,
        implementer: Some("opencode".into()),
        reviewer: None,
        docs: None,
    };
    let path = write_new_preset(&opts, Some(user.path()), None).unwrap();
    assert!(path.exists());
    assert!(path.ends_with("my-team.toml"));
    let body = fs::read_to_string(&path).unwrap();
    assert!(body.contains("extends = \"default-coder\""));
    assert!(body.contains("implementer = \"opencode\""));
}

#[test]
fn write_new_preset_project_tier_writes_under_dot_maestro_teams() {
    let (_user, project, _loader) = make_loader_with_tempdirs();
    let opts = NewPresetOpts {
        name: "proj-team".into(),
        extends: "default-coder".into(),
        tier: TeamTier::Project,
        implementer: None,
        reviewer: None,
        docs: None,
    };
    let path = write_new_preset(&opts, None, Some(project.path())).unwrap();
    assert!(path.exists());
    assert!(path.to_string_lossy().contains(".maestro/teams"));
}

#[test]
fn write_new_preset_rejects_path_traversal_in_name() {
    let (user, _project, _loader) = make_loader_with_tempdirs();
    let opts = NewPresetOpts {
        name: "../etc/passwd".into(),
        extends: "default-coder".into(),
        tier: TeamTier::User,
        implementer: None,
        reviewer: None,
        docs: None,
    };
    let err = write_new_preset(&opts, Some(user.path()), None).unwrap_err();
    let msg = format!("{err:#}");
    // Loader's validate runs leading-dot check before path-char check;
    // either rejection is acceptable — both close the path-traversal vector.
    assert!(
        msg.contains("illegal path characters") || msg.contains("cannot start with '.'"),
        "unexpected error: {msg}"
    );
}

#[test]
fn write_new_preset_rejects_empty_name() {
    let (user, _project, _loader) = make_loader_with_tempdirs();
    let opts = NewPresetOpts {
        name: "".into(),
        extends: "default-coder".into(),
        tier: TeamTier::User,
        implementer: None,
        reviewer: None,
        docs: None,
    };
    let err = write_new_preset(&opts, Some(user.path()), None).unwrap_err();
    assert!(format!("{err:#}").contains("empty"));
}

// -- manage_list -----------------------------------------------------

#[test]
fn manage_list_excludes_builtin_tier() {
    let loader = Loader::new(None, None);
    let entries = manage_list(&loader).unwrap();
    assert!(
        entries.is_empty(),
        "manage_list must not include built-in presets"
    );
}

#[test]
fn manage_list_includes_user_tier_with_path() {
    let (user, _project, loader) = make_loader_with_tempdirs();
    write_user_preset_file(
        user.path(),
        "u1",
        r#"extends = "default-coder"
implementer = "ollama"
"#,
    );
    let entries = manage_list(&loader).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "u1");
    assert!(entries[0].path.ends_with("u1.toml"));
}

// -- launch_headless (with mock SchedulerRunner) --------------------

struct MockSchedulerRunner {
    responses: std::sync::Mutex<std::collections::VecDeque<Result<TeamOutput, String>>>,
}

impl MockSchedulerRunner {
    fn new(responses: Vec<Result<TeamOutput, String>>) -> Self {
        Self {
            responses: std::sync::Mutex::new(responses.into()),
        }
    }
}

#[async_trait::async_trait]
impl SchedulerRunner for MockSchedulerRunner {
    async fn run_issue(
        &self,
        _issue: IssueNumber,
        _team: &ResolvedTeam,
    ) -> Result<TeamOutput, String> {
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| "mock runner exhausted".to_string())
            .and_then(|r| r)
    }
}

fn pr_output(n: u64) -> TeamOutput {
    TeamOutput::Pr {
        number: n,
        branch: format!("feat/{n}"),
    }
}

#[tokio::test]
async fn launch_headless_all_succeed_returns_no_failures() {
    let loader = Loader::new(None, None);
    let runner = std::sync::Arc::new(MockSchedulerRunner::new(vec![Ok(pr_output(1))]));
    let outcome = launch_headless(
        &loader,
        LaunchOpts {
            preset: "default-coder".into(),
            issue: Some(1),
            issues: vec![],
            max_parallel: 1,
        },
        runner,
    )
    .await
    .unwrap();
    assert_eq!(outcome.succeeded, vec![1]);
    assert!(outcome.failed.is_empty());
    assert_eq!(outcome.plan_levels, 1);
}

#[tokio::test]
async fn launch_headless_records_per_issue_failure() {
    let loader = Loader::new(None, None);
    let runner = std::sync::Arc::new(MockSchedulerRunner::new(vec![Err("boom".into())]));
    let outcome = launch_headless(
        &loader,
        LaunchOpts {
            preset: "default-coder".into(),
            issue: Some(99),
            issues: vec![],
            max_parallel: 1,
        },
        runner,
    )
    .await
    .unwrap();
    assert!(outcome.succeeded.is_empty());
    assert_eq!(outcome.failed.len(), 1);
    assert_eq!(outcome.failed[0].0, 99);
    assert!(outcome.failed[0].1.contains("boom"));
}

#[tokio::test]
async fn launch_headless_unknown_preset_returns_err() {
    let loader = Loader::new(None, None);
    let runner = std::sync::Arc::new(MockSchedulerRunner::new(vec![]));
    let err = launch_headless(
        &loader,
        LaunchOpts {
            preset: "no-such-preset".into(),
            issue: Some(1),
            issues: vec![],
            max_parallel: 1,
        },
        runner,
    )
    .await
    .unwrap_err();
    assert!(format!("{err:#}").contains("not found"));
}

#[tokio::test]
async fn launch_headless_requires_issue_or_issues() {
    let loader = Loader::new(None, None);
    let runner = std::sync::Arc::new(MockSchedulerRunner::new(vec![]));
    let err = launch_headless(
        &loader,
        LaunchOpts {
            preset: "default-coder".into(),
            issue: None,
            issues: vec![],
            max_parallel: 1,
        },
        runner,
    )
    .await
    .unwrap_err();
    assert!(format!("{err:#}").contains("required"));
}
