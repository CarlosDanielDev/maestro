use super::*;
use std::time::Instant;

fn make_app() -> crate::tui::app::App {
    crate::tui::make_test_app("maestro-tui-app-test")
}

fn make_app_with_flags(flags: crate::flags::store::FeatureFlags) -> crate::tui::app::App {
    let mut app = crate::tui::make_test_app("maestro-tui-app-test");
    app.flags = flags;
    app
}

#[test]
fn poll_ci_status_skips_fix_when_ci_auto_fix_flag_disabled() {
    use crate::provider::github::ci::PendingPrCheck;
    let flags = crate::flags::store::FeatureFlags::new(
        std::collections::HashMap::new(),
        vec![],
        vec!["ci_auto_fix".to_string()],
    );
    let mut app = make_app_with_flags(flags);
    app.ci_poller.add_check(PendingPrCheck {
        pr_number: 99,
        issue_number: 42,
        branch: "feat/test".to_string(),
        fix_attempt: 0,
        check_count: 0,
        awaiting_fix_ci: false,
        created_at: Instant::now()
            .checked_sub(Duration::from_secs(120))
            .unwrap_or_else(Instant::now),
    });
    app.ci_poller.last_ci_poll = Instant::now()
        .checked_sub(Duration::from_secs(120))
        .unwrap_or_else(Instant::now);
    // poll_ci_status with Flag::CiAutoFix disabled — no fix sessions spawned.
    // The checker will fail (no gh in tests), but auto_fix_enabled is false so
    // CiPollAction::Abandon is chosen — no fix session enqueued.
    app.poll_ci_status();
    assert!(
        app.pending_session_launches.is_empty(),
        "poll_ci_status must not spawn fix session when Flag::CiAutoFix is disabled"
    );
}

// --- Issue #125: CI check details field ---

#[test]
fn app_ci_check_details_field_defaults_to_empty() {
    let app = make_app();
    assert!(app.ci_poller.ci_check_details.is_empty());
}

#[test]
fn ci_check_details_can_be_populated_and_read() {
    let mut app = make_app();
    let detail = crate::provider::github::ci::CheckRunDetail {
        name: "build".into(),
        status: crate::provider::github::ci::CheckStatus::Completed,
        conclusion: crate::provider::github::ci::CheckConclusion::Success,
        started_at: None,
        elapsed_secs: Some(42),
    };
    app.ci_poller.ci_check_details.insert(99, vec![detail]);
    assert_eq!(app.ci_poller.ci_check_details.len(), 1);
    assert_eq!(app.ci_poller.ci_check_details[&99][0].name, "build");
}

#[test]
fn ci_check_details_keyed_by_pr_number() {
    let mut app = make_app();
    let detail = crate::provider::github::ci::CheckRunDetail {
        name: "test".into(),
        status: crate::provider::github::ci::CheckStatus::InProgress,
        conclusion: crate::provider::github::ci::CheckConclusion::None,
        started_at: None,
        elapsed_secs: None,
    };
    app.ci_poller.ci_check_details.insert(55, vec![detail]);
    assert!(app.ci_poller.ci_check_details.contains_key(&55));
    assert!(!app.ci_poller.ci_check_details.contains_key(&10));
}
