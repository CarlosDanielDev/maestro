//! Snapshot test for the F6 dependency-graph empty state (#1160).
//!
//! `--prompt` and `--issue` runs both start without a work assigner; the
//! pane must not call an `--issue` run "prompt-only".

use super::*;
use crate::tui::dep_graph::draw_dep_graph;
use crate::tui::theme::Theme;
use insta::assert_snapshot;

#[test]
fn dep_graph_without_assigner() {
    let mut terminal = test_terminal();
    let theme = Theme::dark();
    terminal
        .draw(|f| draw_dep_graph(f, None, f.area(), &theme))
        .unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(!text.contains("prompt-only"), "{text}");
    assert_snapshot!(terminal.backend());
}
