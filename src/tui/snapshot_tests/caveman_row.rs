use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use crate::settings::CavemanModeState;
use crate::tui::screens::settings::caveman_row::render_caveman_row;
use crate::tui::theme::Theme;

fn render_row(state: &CavemanModeState, focused: bool) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(80, 1)).unwrap();
    let theme = Theme::default();
    terminal
        .draw(|f| {
            render_caveman_row(f, f.area(), state, focused, &theme);
        })
        .unwrap();
    terminal
}

#[test]
fn caveman_row_renders_explicit_true() {
    let t = render_row(&CavemanModeState::ExplicitTrue, false);
    assert_snapshot!(t.backend());
}

#[test]
fn caveman_row_renders_explicit_false() {
    let t = render_row(&CavemanModeState::ExplicitFalse, false);
    assert_snapshot!(t.backend());
}

#[test]
fn caveman_row_renders_default() {
    let t = render_row(&CavemanModeState::Default, false);
    assert_snapshot!(t.backend());
}

#[test]
fn caveman_row_renders_error() {
    let t = render_row(
        &CavemanModeState::Error("simulated load failure".into()),
        false,
    );
    assert_snapshot!(t.backend());
}

#[test]
fn caveman_row_renders_focused_explicit_true() {
    let t = render_row(&CavemanModeState::ExplicitTrue, true);
    assert_snapshot!(t.backend());
}
