//! #1281 judge round: the PRD write lives only in the Scaffold arm, the
//! cache follows the Project field, `x` is a letter on text fields, every
//! run gets a fresh cancel token, and a failed create keeps its list.

use super::run_tests::{profile, screen_in};
use super::*;
use crate::adapt::types::*;
use crate::tui::screens::test_helpers::key_event;

/// The body of one `TuiCommand` arm in `tui/mod.rs`, up to the next arm.
fn command_arm<'a>(src: &'a str, head: &str) -> &'a str {
    let start = src.find(head).expect("arm present");
    let rest = &src[start + head.len()..];
    let end = rest.find("app::TuiCommand::").unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn only_the_scaffold_arm_writes_the_prd() {
    // The event-loop arms spawn real Claude calls, so they cannot run in a
    // test; this pins where the PRD write lives. A dry run never queues
    // `RunAdaptScaffold` (plan_dry_run_writes_nothing), so it never writes.
    let src = include_str!("../../mod.rs");
    let consolidate = command_arm(src, "app::TuiCommand::RunAdaptConsolidate(");
    let plan = command_arm(src, "app::TuiCommand::RunAdaptPlan(");
    let scaffold = command_arm(src, "app::TuiCommand::RunAdaptScaffold(");
    assert!(
        !consolidate.contains("save_consolidated_prd("),
        "Consolidate must not write"
    );
    assert!(
        !plan.contains("save_consolidated_prd("),
        "Plan must not write"
    );
    assert!(
        scaffold.contains("save_consolidated_prd("),
        "Scaffold writes the PRD"
    );
    let code = src
        .split("mod save_consolidated_prd_tests")
        .next()
        .unwrap_or(src);
    assert_eq!(
        code.matches("save_consolidated_prd(").count(),
        2,
        "one def, one call"
    );
}

#[test]
fn cache_follows_the_project_field() {
    // #1540: the project folder ends in `j`, so this runs the same every time
    // and proves `j` types into Project (it used to move focus instead).
    let tmp = tempfile::TempDir::new().unwrap();
    let with_cache = tmp.path().join("proj");
    std::fs::create_dir(&with_cache).unwrap();
    AdaptResults {
        profile: Some(profile(&with_cache)),
        ..Default::default()
    }
    .save_cache(&with_cache);
    let mut s = screen_in(&with_cache);
    // Type the last character of the path: the cache of that project loads.
    assert!(s.config.path.ends_with('j'), "setup: the path ends in j");
    s.config.path.pop();
    s.handle_input(&key_event(KeyCode::Char('j')), InputMode::Normal);
    assert!(s.loaded_from_cache, "the chosen project's cache is found");
    assert!(s.results.profile.is_some());

    // Edit the path to another folder: that cache must not stay loaded.
    s.handle_input(&key_event(KeyCode::Backspace), InputMode::Normal);
    assert!(!s.loaded_from_cache, "no cache for another folder");
    assert!(
        s.results.profile.is_none(),
        "results from the old project are gone"
    );
}

#[test]
fn x_on_the_path_field_types_and_never_arms_the_clear() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut s = screen_in(dir.path());
    s.loaded_from_cache = true;
    let before = s.config.path.clone();
    s.handle_input(&key_event(KeyCode::Char('x')), InputMode::Normal);
    assert!(!s.pending_clear, "x on Project is a letter");
    assert_eq!(s.config.path, format!("{before}x"));
    s.selected_field = 1;
    s.loaded_from_cache = true;
    s.handle_input(&key_event(KeyCode::Char('x')), InputMode::Normal);
    assert!(s.pending_clear, "on Stop after x arms the clear");
}

#[test]
fn retry_after_esc_runs_with_a_fresh_token() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut s = screen_in(dir.path());
    s.step = AdaptStep::Materializing;
    s.handle_input(&key_event(KeyCode::Esc), InputMode::Normal);
    assert!(s.cancel.is_cancelled(), "setup: Esc stopped the run");
    s.set_error(AdaptStep::Materializing, "HTTP 502".into());

    let action = s.handle_input(&key_event(KeyCode::Char('r')), InputMode::Normal);

    assert!(matches!(action, ScreenAction::StartAdaptPipeline(_)));
    assert!(
        !s.cancel.is_cancelled(),
        "the retry is not stopped by the old Esc"
    );
}

#[test]
fn leaving_the_screen_mid_write_cancels() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut s = screen_in(dir.path());
    s.step = AdaptStep::Materializing;
    let token = s.cancel.clone();
    drop(s);
    assert!(
        token.is_cancelled(),
        "a dropped screen stops the materializer"
    );
}

#[test]
fn failed_materialize_keeps_what_was_written() {
    use crate::adapt::materializer::PartialMaterialize;
    let dir = tempfile::TempDir::new().unwrap();
    let mut s = screen_in(dir.path());
    s.step = AdaptStep::Materializing;
    let written = MaterializeResult {
        milestones_created: vec![CreatedMilestone {
            number: 7,
            title: "M0: Foundation".into(),
            reused: false,
        }],
        issues_created: vec![CreatedIssue {
            number: 101,
            title: "feat: setup".into(),
            milestone_number: Some(7),
        }],
        issues_skipped: vec![],
        tech_debt_issue: None,
        dry_run: false,
    };
    let err = anyhow::Error::new(PartialMaterialize {
        written,
        source: anyhow::anyhow!("HTTP 502"),
    });

    s.fail_materialize(err);

    assert_eq!(s.step, AdaptStep::Failed);
    let kept = s.results.materialize.as_ref().expect("partial result kept");
    assert_eq!(kept.issues_created[0].number, 101);
    assert!(s.error.as_ref().unwrap().message.contains("HTTP 502"));

    let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    let theme = crate::tui::theme::Theme::dark();
    t.draw(|f| s.draw(f, f.area(), &theme)).unwrap();
    let shown = format!("{}", t.backend());
    assert!(shown.contains("Written before the failure"), "{shown}");
    assert!(shown.contains("#101 feat: setup"), "{shown}");
}
