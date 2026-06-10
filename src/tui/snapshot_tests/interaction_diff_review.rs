//! Snapshot tests for the read-only diff reviewer overlay (#918): empty
//! diff, single-file, 16-file list, and a scrolled state.

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use crate::tui::navigation::InputMode;
use crate::tui::screens::test_helpers::{key_event, key_event_with_modifiers};
use crate::tui::screens::{InteractionScreen, Screen};
use crate::tui::theme::Theme;
use crossterm::event::{KeyCode, KeyModifiers};

const W: u16 = 120;
const H: u16 = 36;

fn single_file_diff() -> String {
    "diff --git a/src/lib.rs b/src/lib.rs\n\
     index 111..222 100644\n\
     --- a/src/lib.rs\n\
     +++ b/src/lib.rs\n\
     @@ -1,4 +1,5 @@\n\
      fn keep() {}\n\
     -fn removed() {}\n\
     +fn added() {}\n\
     +fn also_added() {}\n\
      fn tail() {}\n"
        .to_string()
}

fn many_files_diff(count: usize) -> String {
    let mut out = String::new();
    for i in 0..count {
        out.push_str(&format!(
            "diff --git a/src/file_{i:02}.rs b/src/file_{i:02}.rs\n\
             index 111..222 100644\n\
             --- a/src/file_{i:02}.rs\n\
             +++ b/src/file_{i:02}.rs\n\
             @@ -1,2 +1,2 @@\n\
             -old line {i}\n\
             +new line {i}\n"
        ));
    }
    out
}

fn screen_with_diff(diff: &str) -> InteractionScreen {
    let mut screen = InteractionScreen::new();
    screen.open_diff_review(diff);
    screen
}

fn render(screen: &mut InteractionScreen) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(W, H)).unwrap();
    let theme = Theme::dark();
    terminal.draw(|f| screen.draw(f, f.area(), &theme)).unwrap();
    terminal
}

#[test]
fn diff_review_empty_diff() {
    let mut screen = screen_with_diff("");
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains("no changes vs base"),
        "empty state message expected:\n{rendered}"
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn diff_review_single_file() {
    let mut screen = screen_with_diff(&single_file_diff());
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(rendered.contains("src/lib.rs"), "{rendered}");
    assert!(rendered.contains("fn added()"), "{rendered}");
    assert_snapshot!(terminal.backend());
}

#[test]
fn diff_review_sixteen_files() {
    let mut screen = screen_with_diff(&many_files_diff(16));
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(rendered.contains("file_00.rs"), "{rendered}");
    assert!(rendered.contains("file_15.rs"), "{rendered}");
    assert_snapshot!(terminal.backend());
}

#[test]
fn diff_review_scrolled_state() {
    let mut screen = screen_with_diff(&single_file_diff());
    // Render once so the viewport is known, then scroll down two lines.
    let _ = render(&mut screen);
    screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Insert);
    screen.handle_input(&key_event(KeyCode::Char('j')), InputMode::Insert);
    let terminal = render(&mut screen);
    assert_snapshot!(terminal.backend());
}

#[test]
fn diff_review_close_returns_to_chat_with_state_intact() {
    let mut screen = screen_with_diff(&single_file_diff());
    assert!(screen.diff_review_open());
    screen.handle_input(&key_event(KeyCode::Char('q')), InputMode::Insert);
    assert!(!screen.diff_review_open(), "q closes the overlay");
    // Ctrl+D is greyed without a worktree: the empty-root screen logs.
    let action = screen.handle_input(
        &key_event_with_modifiers(KeyCode::Char('d'), KeyModifiers::CONTROL),
        InputMode::Insert,
    );
    match action {
        crate::tui::screens::ScreenAction::LogActivity { message, .. } => {
            assert!(message.contains("no isolated worktree"), "{message}");
        }
        other => panic!("expected greyed log line, got {other:?}"),
    }
}
