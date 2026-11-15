use super::*;
use crate::session::types::SessionStatus;
use crate::state::progress::ProgressTracker;
use crate::tui::detail::draw_detail_with_claims;
use crate::tui::theme::Theme;
use insta::assert_snapshot;

#[test]
fn detail_view_basic() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let session = make_session(SessionStatus::Running, Some(42));
    let tracker = ProgressTracker::new();

    terminal
        .draw(|f| {
            draw_detail_with_claims(f, &session, &tracker, None, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn detail_view_with_progress() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let session = make_session(SessionStatus::Running, Some(42));
    let mut tracker = ProgressTracker::new();
    let progress = tracker.get_or_create(session.id);
    progress.on_tool_use("Write", Some("src/main.rs"));
    progress.on_tool_use("Write", Some("src/lib.rs"));

    terminal
        .draw(|f| {
            draw_detail_with_claims(f, &session, &tracker, None, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn detail_view_with_activity_log() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut session = make_session(SessionStatus::Running, Some(42));
    session.activity_log = vec![
        make_activity(5, "Reading src/main.rs"),
        make_activity(15, "Writing tests"),
        make_activity(30, "Running cargo test"),
    ];
    let tracker = ProgressTracker::new();

    terminal
        .draw(|f| {
            draw_detail_with_claims(f, &session, &tracker, None, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn detail_view_no_files_touched() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut session = make_session(SessionStatus::Running, Some(42));
    session.files_touched = vec![];
    let tracker = ProgressTracker::new();

    terminal
        .draw(|f| {
            draw_detail_with_claims(f, &session, &tracker, None, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn detail_view_with_files_and_retries() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut session = make_session(SessionStatus::Errored, Some(42));
    session.files_touched = vec![
        "src/main.rs".to_string(),
        "src/lib.rs".to_string(),
        "tests/test.rs".to_string(),
    ];
    session.retry_count = 3;
    let tracker = ProgressTracker::new();

    terminal
        .draw(|f| {
            draw_detail_with_claims(f, &session, &tracker, None, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn detail_view_activity_log_does_not_use_markdown() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut session = make_session(SessionStatus::Running, Some(42));
    session.activity_log = vec![
        make_activity(5, "**bold** should appear literally"),
        make_activity(10, "Running `cargo test`"),
    ];
    let tracker = ProgressTracker::new();

    terminal
        .draw(|f| {
            draw_detail_with_claims(f, &session, &tracker, None, f.area(), &theme);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}
