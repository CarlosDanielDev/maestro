//! Snapshot tests for the free-form prompt screen's launch options (#919).
//!
//! Pins the two checkbox rows (`Produce PR`, `Interaction`) added below the
//! attachments list, in the default and toggled states.

use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};

use crate::tui::screens::{PromptInputScreen, Screen};
use crate::tui::theme::Theme;

const W: u16 = 100;
const H: u16 = 28;

fn render(screen: &mut PromptInputScreen) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(W, H)).unwrap();
    let theme = Theme::dark();
    terminal.draw(|f| screen.draw(f, f.area(), &theme)).unwrap();
    terminal
}

#[test]
fn prompt_input_launch_options_default() {
    let mut screen = PromptInputScreen::new().with_launch_defaults((true, false));
    let terminal = render(&mut screen);
    let rendered = format!("{:?}", terminal.backend());
    assert!(rendered.contains("Produce PR"), "missing row:\n{rendered}");
    assert!(rendered.contains("Interaction"), "missing row:\n{rendered}");
    assert_snapshot!(terminal.backend());
}

#[test]
fn prompt_input_launch_options_toggled() {
    let mut screen = PromptInputScreen::new().with_launch_defaults((false, true));
    // Focus the Interaction stop so the `>` marker renders.
    screen.focus_ring.next(); // images
    screen.focus_ring.next(); // produce_pr
    screen.focus_ring.next(); // interaction
    let terminal = render(&mut screen);
    assert_snapshot!(terminal.backend());
}
