use super::*;
use crate::session::types::SessionStatus;
use crate::state::progress::ProgressTracker;
use crate::tui::fullscreen::draw_fullscreen;
use crate::tui::theme::Theme;
use insta::assert_snapshot;

#[test]
fn fullscreen_view_markdown_last_message() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut session = make_session(SessionStatus::Running, Some(99));
    session.last_message =
        "# Status Update\n\nRunning **all tests** now.\n\n- unit tests\n- integration tests\n"
            .to_string();
    let tracker = ProgressTracker::new();

    terminal
        .draw(|f| {
            draw_fullscreen(f, &session, &tracker, f.area(), &theme, 0);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn fullscreen_view_empty_last_message_shows_placeholder() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut session = make_session(SessionStatus::Running, Some(99));
    session.last_message = String::new();
    let tracker = ProgressTracker::new();

    terminal
        .draw(|f| {
            draw_fullscreen(f, &session, &tracker, f.area(), &theme, 0);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn fullscreen_view_auto_scroll_to_bottom() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut session = make_session(SessionStatus::Running, Some(99));
    let mut long_msg = String::new();
    for i in 1..=25 {
        long_msg.push_str(&format!(
            "## Section {}\n\nContent for section {}.\n\n",
            i, i
        ));
    }
    session.last_message = long_msg;
    let tracker = ProgressTracker::new();

    terminal
        .draw(|f| {
            draw_fullscreen(f, &session, &tracker, f.area(), &theme, 0);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}

#[test]
fn fullscreen_view_plain_text_last_message() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    let mut session = make_session(SessionStatus::Running, Some(99));
    session.last_message = "Running tests, please wait...".to_string();
    let tracker = ProgressTracker::new();

    terminal
        .draw(|f| {
            draw_fullscreen(f, &session, &tracker, f.area(), &theme, 0);
        })
        .unwrap();

    assert_snapshot!(terminal.backend());
}
