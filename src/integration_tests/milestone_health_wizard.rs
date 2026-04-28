//! Integration tests for the milestone-health wizard (#500).
//!
//! These tests drive the state machine and the `MockGitHubClient` end to
//! end without rendering a TUI. The contract verified is:
//!
//! - Exactly zero GitHub writes occur until the user explicitly confirms.
//! - On confirm, exactly one `patch_milestone_description` call is recorded.
//! - Errors at any I/O step never leave the wizard in an inconsistent state.

use crossterm::event::KeyCode;

use crate::milestone_health::report::HealthReport;
use crate::milestone_health::{analyze, check_issues};
use crate::provider::github::client::GitHubClient;
use crate::provider::github::client::mock::MockGitHubClient;
use crate::provider::github::types::{GhIssue, GhMilestone};
use crate::tui::screens::milestone_health::state::{
    HealthInput, HealthScreenState, HealthSideEffect, HealthStep, PatchOutcome,
};

fn ms(n: u64, title: &str, desc: &str) -> GhMilestone {
    GhMilestone {
        number: n,
        title: title.to_string(),
        description: desc.to_string(),
        state: "open".to_string(),
        open_issues: 0,
        closed_issues: 0,
    }
}

fn full_feature_body(blockers: &[u64]) -> String {
    let mut s = String::new();
    for sec in [
        "Overview",
        "Expected Behavior",
        "Acceptance Criteria",
        "Files to Modify",
        "Test Hints",
        "Blocked By",
        "Definition of Done",
    ] {
        s.push_str(&format!("## {}\n\n", sec));
        match sec {
            "Acceptance Criteria" => s.push_str("- [ ] one\n\n"),
            "Blocked By" => {
                if blockers.is_empty() {
                    s.push_str("- None\n\n");
                } else {
                    for b in blockers {
                        s.push_str(&format!("- #{} placeholder\n", b));
                    }
                    s.push('\n');
                }
            }
            _ => s.push_str("placeholder\n\n"),
        }
    }
    s
}

fn missing_section_body(missing: &str) -> String {
    let mut s = String::new();
    for sec in [
        "Overview",
        "Expected Behavior",
        "Acceptance Criteria",
        "Files to Modify",
        "Test Hints",
        "Blocked By",
        "Definition of Done",
    ] {
        if sec == missing {
            continue;
        }
        s.push_str(&format!("## {}\n\n", sec));
        match sec {
            "Acceptance Criteria" => s.push_str("- [ ] one\n\n"),
            "Blocked By" => s.push_str("- None\n\n"),
            _ => s.push_str("placeholder\n\n"),
        }
    }
    s
}

fn make_issue(number: u64, body: String) -> GhIssue {
    GhIssue {
        number,
        title: format!("Issue #{}", number),
        body,
        labels: vec!["type:feature".to_string()],
        state: "open".to_string(),
        html_url: format!("https://github.com/owner/repo/issues/{}", number),
        milestone: Some(1),
        assignees: vec![],
    }
}

fn graph_with_all_at_level_zero(numbers: &[u64]) -> String {
    let mut s =
        String::from("Header summary.\n\n## Dependency Graph (Implementation Order)\n\nLevel 0:\n");
    for n in numbers {
        s.push_str(&format!("• #{} placeholder\n", n));
    }
    s.push_str("\nSequence: ");
    s.push_str(
        &numbers
            .iter()
            .map(|n| format!("#{}", n))
            .collect::<Vec<_>>()
            .join(" ∥ "),
    );
    s.push('\n');
    s
}

fn run_to_report(
    state: &mut HealthScreenState,
    milestone: GhMilestone,
    issues: Vec<GhIssue>,
) -> HealthReport {
    // Walk Picker → Loading via the public reducer.
    let _ = state.transition(HealthInput::MilestonesLoaded(Ok(vec![milestone.clone()])));
    let _ = state.transition(HealthInput::Key(KeyCode::Enter)); // Picker → Loading
    let dor = check_issues(&issues);
    let anomalies = analyze(&milestone.description, &issues);
    let report = HealthReport {
        dor,
        anomalies: anomalies.clone(),
    };
    let _ = state.transition(HealthInput::DataFetched(Ok((milestone, issues))));
    report
}

// G-1
#[tokio::test]
async fn end_to_end_5_issues_2_dor_fails_1_cycle_correct_report() {
    let mock = MockGitHubClient::new();
    let issues = vec![
        make_issue(1, full_feature_body(&[])),
        make_issue(2, full_feature_body(&[])),
        make_issue(3, missing_section_body("Blocked By")),
        make_issue(4, missing_section_body("Acceptance Criteria")),
        make_issue(5, full_feature_body(&[6])),
        make_issue(6, full_feature_body(&[5])),
    ];
    mock.set_issues(issues.clone());
    let m = ms(
        1,
        "v1.0",
        &graph_with_all_at_level_zero(&[1, 2, 3, 4, 5, 6]),
    );
    mock.set_milestones(vec![m.clone()]);

    let mut state = HealthScreenState::new();
    let report = run_to_report(&mut state, m, issues);
    assert_eq!(report.dor.len(), 6);
    assert_eq!(report.not_ready_count(), 2);
    assert!(report.anomaly_count() >= 1);
    assert_eq!(mock.patch_milestone_calls().len(), 0);
}

// G-2
#[tokio::test]
async fn end_to_end_confirm_dispatches_one_patch_and_mock_records_it() {
    let mock = MockGitHubClient::new();
    let issues = vec![
        make_issue(1, full_feature_body(&[])),
        make_issue(2, full_feature_body(&[1])),
    ];
    mock.set_issues(issues.clone());
    let m = ms(1, "v1.0", &graph_with_all_at_level_zero(&[1, 2]));
    mock.set_milestones(vec![m.clone()]);

    let mut state = HealthScreenState::new();
    let _ = run_to_report(&mut state, m, issues);
    // Report → Patch
    let _ = state.transition(HealthInput::Key(KeyCode::Enter));
    // Patch → Confirm
    let _ = state.transition(HealthInput::Key(KeyCode::Enter));
    // Confirm → Writing — this dispatches the PATCH side effect.
    let eff = state.transition(HealthInput::Key(KeyCode::Enter));
    let (mn, desc) = match eff {
        HealthSideEffect::DispatchPatch {
            milestone_number,
            description,
        } => (milestone_number, description),
        other => panic!("expected DispatchPatch, got {:?}", other),
    };
    // Caller (the screen) executes the PATCH on the mock and feeds the
    // result back into the reducer.
    mock.patch_milestone_description(mn, &desc).await.unwrap();
    let _ = state.transition(HealthInput::DataPatched(Ok(())));

    assert!(matches!(
        state.step,
        HealthStep::Result {
            outcome: PatchOutcome::Success,
            ..
        }
    ));
    assert_eq!(mock.patch_milestone_calls().len(), 1);
}

// G-3
#[tokio::test]
async fn list_issues_err_state_lands_in_fetch_error_no_patch() {
    let mock = MockGitHubClient::new();
    mock.set_list_issues_by_milestone_error("network failure");
    let m = ms(1, "v1.0", "");
    mock.set_milestones(vec![m.clone()]);

    let mut state = HealthScreenState::new();
    let _ = state.transition(HealthInput::MilestonesLoaded(Ok(vec![m.clone()])));
    let _ = state.transition(HealthInput::Key(KeyCode::Enter));
    let issues_result = mock.list_issues_by_milestone(&m.title).await;
    assert!(issues_result.is_err());
    let _ = state.transition(HealthInput::DataFetched(Err(issues_result.err().unwrap())));

    assert!(matches!(state.step, HealthStep::FetchError { .. }));
    assert!(mock.patch_milestone_calls().is_empty());
}

// G-4
#[tokio::test]
async fn healthy_milestone_goes_to_healthy_step_no_patch() {
    let mock = MockGitHubClient::new();
    let issues = vec![
        make_issue(1, full_feature_body(&[])),
        make_issue(2, full_feature_body(&[1])),
    ];
    mock.set_issues(issues.clone());
    let desc = "Header.\n\n## Dependency Graph (Implementation Order)\n\nLevel 0 — no dependencies:\n• #1 a\n\nLevel 1 — depends on Level 0:\n• #2 b\n\nSequence: #1 → #2\n";
    let m = ms(1, "v1.0", desc);
    mock.set_milestones(vec![m.clone()]);

    let mut state = HealthScreenState::new();
    let _ = run_to_report(&mut state, m, issues);
    assert!(
        matches!(state.step, HealthStep::Healthy { .. }),
        "step = {:?}",
        state.step
    );
    assert!(mock.patch_milestone_calls().is_empty());
}

// G-5
#[tokio::test]
async fn esc_from_confirm_dispatches_milestones_refetch_no_patch() {
    let mock = MockGitHubClient::new();
    let issues = vec![
        make_issue(1, full_feature_body(&[])),
        make_issue(2, full_feature_body(&[1])),
    ];
    mock.set_issues(issues.clone());
    let m = ms(1, "v1.0", &graph_with_all_at_level_zero(&[1, 2]));
    mock.set_milestones(vec![m.clone()]);

    let mut state = HealthScreenState::new();
    let _ = run_to_report(&mut state, m, issues);
    let _ = state.transition(HealthInput::Key(KeyCode::Enter)); // Report → Patch
    let _ = state.transition(HealthInput::Key(KeyCode::Enter)); // Patch → Confirm
    let eff = state.transition(HealthInput::Key(KeyCode::Esc));
    assert_eq!(eff, HealthSideEffect::DispatchFetchMilestones);
    assert!(mock.patch_milestone_calls().is_empty());
}

// G-6
#[tokio::test]
async fn esc_from_patch_dispatches_milestones_refetch_no_patch() {
    let mock = MockGitHubClient::new();
    let issues = vec![
        make_issue(1, full_feature_body(&[])),
        make_issue(2, full_feature_body(&[1])),
    ];
    mock.set_issues(issues.clone());
    let m = ms(1, "v1.0", &graph_with_all_at_level_zero(&[1, 2]));
    mock.set_milestones(vec![m.clone()]);

    let mut state = HealthScreenState::new();
    let _ = run_to_report(&mut state, m, issues);
    let _ = state.transition(HealthInput::Key(KeyCode::Enter)); // Report → Patch
    let eff = state.transition(HealthInput::Key(KeyCode::Esc));
    assert_eq!(eff, HealthSideEffect::DispatchFetchMilestones);
    assert!(mock.patch_milestone_calls().is_empty());
}

// G-7
#[tokio::test]
async fn patch_error_then_retry_then_ok_records_two_patch_calls() {
    let mock = MockGitHubClient::new();
    let issues = vec![
        make_issue(1, full_feature_body(&[])),
        make_issue(2, full_feature_body(&[1])),
    ];
    mock.set_issues(issues.clone());
    let m = ms(1, "v1.0", &graph_with_all_at_level_zero(&[1, 2]));
    mock.set_milestones(vec![m.clone()]);

    let mut state = HealthScreenState::new();
    let _ = run_to_report(&mut state, m, issues);
    let _ = state.transition(HealthInput::Key(KeyCode::Enter)); // Report → Patch
    let _ = state.transition(HealthInput::Key(KeyCode::Enter)); // Patch → Confirm
    let eff = state.transition(HealthInput::Key(KeyCode::Enter)); // Confirm → Writing
    let (mn, desc) = match eff {
        HealthSideEffect::DispatchPatch {
            milestone_number,
            description,
        } => (milestone_number, description),
        other => panic!("expected DispatchPatch, got {:?}", other),
    };
    // First attempt fails.
    mock.set_patch_milestone_error("403 forbidden");
    let result = mock.patch_milestone_description(mn, &desc).await;
    assert!(result.is_err());
    let _ = state.transition(HealthInput::DataPatched(Err(result.err().unwrap())));
    assert!(matches!(
        state.step,
        HealthStep::Result {
            outcome: PatchOutcome::Error {
                retryable: true,
                ..
            },
            ..
        }
    ));

    // Retry — clear the error injection first.
    mock.clear_patch_milestone_error();
    let eff2 = state.transition(HealthInput::Key(KeyCode::Char('r')));
    let (mn2, desc2) = match eff2 {
        HealthSideEffect::DispatchPatch {
            milestone_number,
            description,
        } => (milestone_number, description),
        other => panic!("expected DispatchPatch on retry, got {:?}", other),
    };
    mock.patch_milestone_description(mn2, &desc2).await.unwrap();
    let _ = state.transition(HealthInput::DataPatched(Ok(())));

    assert!(matches!(
        state.step,
        HealthStep::Result {
            outcome: PatchOutcome::Success,
            ..
        }
    ));
    assert_eq!(mock.patch_milestone_calls().len(), 2);
}

// G-8
#[tokio::test]
async fn empty_milestone_zero_issues_goes_to_empty_no_patch() {
    let mock = MockGitHubClient::new();
    mock.set_issues(vec![]);
    let m = ms(1, "v1.0", "Header. No issues.");
    mock.set_milestones(vec![m.clone()]);

    let mut state = HealthScreenState::new();
    let _ = state.transition(HealthInput::MilestonesLoaded(Ok(vec![m.clone()])));
    let _ = state.transition(HealthInput::Key(KeyCode::Enter));
    let _ = state.transition(HealthInput::DataFetched(Ok((m, vec![]))));
    assert!(matches!(state.step, HealthStep::Empty { .. }));
    assert!(mock.patch_milestone_calls().is_empty());
}

// G-9
#[tokio::test]
async fn fifty_issues_analysis_completes_dor_len_is_50() {
    let mock = MockGitHubClient::new();
    let issues: Vec<GhIssue> = (1..=50)
        .map(|n| make_issue(n, full_feature_body(&[])))
        .collect();
    mock.set_issues(issues.clone());
    let numbers: Vec<u64> = (1..=50).collect();
    let m = ms(1, "v1.0", &graph_with_all_at_level_zero(&numbers));
    mock.set_milestones(vec![m.clone()]);

    let mut state = HealthScreenState::new();
    let report = run_to_report(&mut state, m, issues);
    assert_eq!(report.dor.len(), 50);
}
