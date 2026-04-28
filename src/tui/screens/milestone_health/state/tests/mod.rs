//! Tests for the milestone health-check wizard state machine (#500).

use super::*;

fn ms(n: u64, title: &str) -> GhMilestone {
    GhMilestone {
        number: n,
        title: title.to_string(),
        description: String::new(),
        state: "open".to_string(),
        open_issues: 0,
        closed_issues: 0,
    }
}

fn issue(number: u64, body: &str) -> GhIssue {
    GhIssue {
        number,
        title: format!("Issue #{}", number),
        body: body.to_string(),
        labels: vec!["type:feature".to_string()],
        state: "open".to_string(),
        html_url: format!("https://github.com/owner/repo/issues/{}", number),
        milestone: Some(1),
        assignees: vec![],
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

fn picker_with(milestones: Vec<GhMilestone>) -> HealthScreenState {
    HealthScreenState {
        step: HealthStep::Picker {
            milestones,
            selected: 0,
        },
        report: None,
    }
}

fn loading() -> HealthScreenState {
    HealthScreenState {
        step: HealthStep::Loading {
            label: "Fetching…".to_string(),
            milestone: None,
        },
        report: None,
    }
}

#[test]
fn loading_milestones_loaded_ok_goes_to_picker() {
    let mut s = loading();
    let eff = s.transition(HealthInput::MilestonesLoaded(Ok(vec![ms(1, "v1.0")])));
    assert!(matches!(s.step, HealthStep::Picker { .. }));
    assert_eq!(eff, HealthSideEffect::None);
}

#[test]
fn loading_milestones_loaded_err_goes_to_fetch_error() {
    let mut s = loading();
    let _ = s.transition(HealthInput::MilestonesLoaded(Err(anyhow::anyhow!(
        "network failure"
    ))));
    assert!(matches!(s.step, HealthStep::FetchError { .. }));
}

#[test]
fn picker_enter_dispatches_fetch_issues() {
    let mut s = picker_with(vec![ms(5, "v2.0")]);
    let eff = s.transition(HealthInput::Key(KeyCode::Enter));
    assert!(matches!(s.step, HealthStep::Loading { .. }));
    assert_eq!(
        eff,
        HealthSideEffect::DispatchFetchIssues {
            milestone_number: 5,
            milestone_title: "v2.0".to_string(),
        }
    );
}

#[test]
fn picker_esc_pops() {
    let mut s = picker_with(vec![]);
    let eff = s.transition(HealthInput::Key(KeyCode::Esc));
    assert_eq!(eff, HealthSideEffect::Pop);
}

#[test]
fn loading_data_fetched_with_issues_and_anomalies_goes_to_report() {
    let mut s = loading();
    let m = ms(1, "v1.0");
    let issues = vec![issue(1, &full_feature_body(&[]))];
    let _ = s.transition(HealthInput::DataFetched(Ok((m, issues))));
    assert!(matches!(
        s.step,
        HealthStep::Report { .. } | HealthStep::Healthy { .. }
    ));
}

#[test]
fn loading_data_fetched_zero_issues_goes_to_empty() {
    let mut s = loading();
    let _ = s.transition(HealthInput::DataFetched(Ok((ms(1, "v1.0"), vec![]))));
    assert!(matches!(s.step, HealthStep::Empty { .. }));
}

#[test]
fn loading_data_fetched_healthy_goes_to_healthy() {
    let mut s = loading();
    let m = GhMilestone {
        number: 1,
        title: "v1.0".to_string(),
        description: "Header.\n\n## Dependency Graph (Implementation Order)\n\nLevel 0 — no dependencies:\n• #1 first\n\nLevel 1 — depends on Level 0:\n• #2 second\n\nSequence: #1 → #2\n".to_string(),
        state: "open".to_string(),
        open_issues: 2,
        closed_issues: 0,
    };
    let issues = vec![
        issue(1, &full_feature_body(&[])),
        issue(2, &full_feature_body(&[1])),
    ];
    let _ = s.transition(HealthInput::DataFetched(Ok((m, issues))));
    assert!(
        matches!(s.step, HealthStep::Healthy { .. }),
        "step = {:?}",
        s.step
    );
}

#[test]
fn loading_data_fetched_err_goes_to_fetch_error() {
    let mut s = loading();
    let _ = s.transition(HealthInput::DataFetched(Err(anyhow::anyhow!(
        "gh cli error"
    ))));
    assert!(matches!(s.step, HealthStep::FetchError { .. }));
}

#[test]
fn report_enter_goes_to_patch() {
    let mut s = HealthScreenState {
        step: HealthStep::Report {
            milestone: ms(1, "v1.0"),
            issues: vec![issue(1, &full_feature_body(&[]))],
        },
        report: Some(HealthReport::default()),
    };
    let _ = s.transition(HealthInput::Key(KeyCode::Enter));
    assert!(matches!(s.step, HealthStep::Patch { .. }));
}

#[test]
fn healthy_esc_dispatches_milestones_refetch() {
    let mut s = HealthScreenState {
        step: HealthStep::Healthy {
            milestone: ms(1, "v1.0"),
        },
        report: None,
    };
    let eff = s.transition(HealthInput::Key(KeyCode::Esc));
    assert_eq!(eff, HealthSideEffect::DispatchFetchMilestones);
}

#[test]
fn patch_enter_goes_to_confirm() {
    let mut s = HealthScreenState {
        step: HealthStep::Patch {
            milestone: ms(1, "v1.0"),
            proposed: "proposed".to_string(),
            diff: vec![],
        },
        report: None,
    };
    let _ = s.transition(HealthInput::Key(KeyCode::Enter));
    assert!(matches!(s.step, HealthStep::Confirm { .. }));
}

#[test]
fn patch_esc_dispatches_milestones_refetch() {
    let mut s = HealthScreenState {
        step: HealthStep::Patch {
            milestone: ms(1, "v1.0"),
            proposed: "proposed".to_string(),
            diff: vec![],
        },
        report: None,
    };
    let eff = s.transition(HealthInput::Key(KeyCode::Esc));
    assert_eq!(eff, HealthSideEffect::DispatchFetchMilestones);
}

#[test]
fn confirm_enter_dispatches_patch_and_goes_to_writing() {
    let mut s = HealthScreenState {
        step: HealthStep::Confirm {
            milestone: ms(10, "v1.0"),
            proposed: "new description".to_string(),
        },
        report: None,
    };
    let eff = s.transition(HealthInput::Key(KeyCode::Enter));
    assert_eq!(
        eff,
        HealthSideEffect::DispatchPatch {
            milestone_number: 10,
            description: "new description".to_string(),
        }
    );
    assert!(matches!(s.step, HealthStep::Writing { .. }));
}

#[test]
fn confirm_esc_dispatches_milestones_refetch_no_patch() {
    let mut s = HealthScreenState {
        step: HealthStep::Confirm {
            milestone: ms(1, "v1.0"),
            proposed: "x".to_string(),
        },
        report: None,
    };
    let eff = s.transition(HealthInput::Key(KeyCode::Esc));
    assert_eq!(eff, HealthSideEffect::DispatchFetchMilestones);
    assert!(!matches!(eff, HealthSideEffect::DispatchPatch { .. }));
}

#[test]
fn writing_data_patched_ok_goes_to_result_success() {
    let mut s = HealthScreenState {
        step: HealthStep::Writing {
            milestone: ms(5, "v1.0"),
            last_proposed: "x".to_string(),
        },
        report: None,
    };
    let _ = s.transition(HealthInput::DataPatched(Ok(())));
    assert!(matches!(
        s.step,
        HealthStep::Result {
            outcome: PatchOutcome::Success,
            ..
        }
    ));
}

#[test]
fn writing_data_patched_err_goes_to_result_error_retryable() {
    let mut s = HealthScreenState {
        step: HealthStep::Writing {
            milestone: ms(5, "v1.0"),
            last_proposed: "x".to_string(),
        },
        report: None,
    };
    let _ = s.transition(HealthInput::DataPatched(Err(anyhow::anyhow!(
        "403 forbidden"
    ))));
    if let HealthStep::Result {
        outcome: PatchOutcome::Error {
            retryable, message, ..
        },
        ..
    } = &s.step
    {
        assert!(retryable);
        assert!(message.contains("403"));
    } else {
        panic!("step = {:?}", s.step);
    }
}

#[test]
fn writing_esc_is_ignored_stays_writing() {
    let mut s = HealthScreenState {
        step: HealthStep::Writing {
            milestone: ms(5, "v1.0"),
            last_proposed: "x".to_string(),
        },
        report: None,
    };
    let eff = s.transition(HealthInput::Key(KeyCode::Esc));
    assert_eq!(eff, HealthSideEffect::None);
    assert!(matches!(s.step, HealthStep::Writing { .. }));
}

#[test]
fn result_error_retry_key_dispatches_patch_again() {
    let mut s = HealthScreenState {
        step: HealthStep::Result {
            milestone: ms(3, "v1.0"),
            outcome: PatchOutcome::Error {
                message: "403".to_string(),
                retryable: true,
                last_proposed: "last proposed desc".to_string(),
            },
        },
        report: None,
    };
    let eff = s.transition(HealthInput::Key(KeyCode::Char('r')));
    assert_eq!(
        eff,
        HealthSideEffect::DispatchPatch {
            milestone_number: 3,
            description: "last proposed desc".to_string(),
        }
    );
    assert!(matches!(s.step, HealthStep::Writing { .. }));
}

#[test]
fn result_esc_dispatches_milestones_refetch() {
    let mut s = HealthScreenState {
        step: HealthStep::Result {
            milestone: ms(1, "v1.0"),
            outcome: PatchOutcome::Success,
        },
        report: None,
    };
    let eff = s.transition(HealthInput::Key(KeyCode::Esc));
    assert_eq!(eff, HealthSideEffect::DispatchFetchMilestones);
}

#[test]
fn empty_esc_dispatches_milestones_refetch() {
    let mut s = HealthScreenState {
        step: HealthStep::Empty {
            milestone: ms(1, "v1.0"),
        },
        report: None,
    };
    let eff = s.transition(HealthInput::Key(KeyCode::Esc));
    assert_eq!(eff, HealthSideEffect::DispatchFetchMilestones);
}

#[test]
fn full_happy_path_dispatches_patch_exactly_once() {
    let m = ms(1, "v1.0");
    let issues = vec![
        issue(1, &full_feature_body(&[])),
        issue(2, &full_feature_body(&[1])),
    ];
    let mut s = picker_with(vec![m.clone()]);
    let mut effects: Vec<HealthSideEffect> = Vec::new();
    effects.push(s.transition(HealthInput::Key(KeyCode::Enter)));
    effects.push(s.transition(HealthInput::DataFetched(Ok((m, issues)))));
    assert!(
        matches!(s.step, HealthStep::Report { .. }),
        "step = {:?}",
        s.step
    );
    effects.push(s.transition(HealthInput::Key(KeyCode::Enter)));
    effects.push(s.transition(HealthInput::Key(KeyCode::Enter)));
    effects.push(s.transition(HealthInput::Key(KeyCode::Enter)));

    let patches: Vec<&HealthSideEffect> = effects
        .iter()
        .filter(|e| matches!(e, HealthSideEffect::DispatchPatch { .. }))
        .collect();
    assert_eq!(patches.len(), 1);
}

#[test]
fn healthy_path_never_reaches_patch_or_confirm() {
    let mut s = loading();
    let m = GhMilestone {
        number: 1,
        title: "v1.0".to_string(),
        description: "Header.\n\n## Dependency Graph (Implementation Order)\n\nLevel 0 — no dependencies:\n• #1 a\n\nSequence: #1\n".to_string(),
        state: "open".to_string(),
        open_issues: 1,
        closed_issues: 0,
    };
    let issues = vec![issue(1, &full_feature_body(&[]))];
    let _ = s.transition(HealthInput::DataFetched(Ok((m, issues))));
    assert!(matches!(s.step, HealthStep::Healthy { .. }));
}

#[test]
fn esc_from_loading_dispatches_milestones_refetch() {
    let mut s = loading();
    let eff = s.transition(HealthInput::Key(KeyCode::Esc));
    assert_eq!(eff, HealthSideEffect::DispatchFetchMilestones);
}

#[test]
fn fetch_error_retry_key_does_not_dispatch_patch() {
    // Regression for security finding M-2: a fetch failure must never
    // bypass the Confirm gate. The dedicated FetchError step makes this
    // structurally impossible — any key returns `DispatchFetchMilestones`.
    let mut s = loading();
    let _ = s.transition(HealthInput::MilestonesLoaded(Err(anyhow::anyhow!(
        "network failure"
    ))));
    let eff = s.transition(HealthInput::Key(KeyCode::Char('r')));
    assert!(!matches!(eff, HealthSideEffect::DispatchPatch { .. }));
    assert_eq!(eff, HealthSideEffect::DispatchFetchMilestones);
}
