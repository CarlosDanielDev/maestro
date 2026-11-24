use crate::tui::app::TuiMode;
use crate::tui::keybinding_hints::keybinding_hints_spans;
use crate::tui::navigation::keymap::{fit_hints_to_width, mode_keymap};
use crate::tui::theme::Theme;
use insta::assert_debug_snapshot;
use ratatui::text::Span;

fn render_overview_hints(agent_graph_enabled: bool) -> Vec<Span<'static>> {
    let km = mode_keymap(TuiMode::Overview, None, &[], agent_graph_enabled);
    let theme = Theme::dark();
    let fitted = fit_hints_to_width(km.hints, 120);
    keybinding_hints_spans(&fitted, false, &theme)
}

/// #1292: the Overview `g` toggle is gone (the graph is Dashboard › Deps
/// › files), so its `[g] Graph` hint is gone with the flag on too.
#[test]
fn overview_hints_with_agent_graph_flag_on_have_no_g_entry() {
    let spans = render_overview_hints(true);
    let rendered: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(
        !rendered.contains("[g]"),
        "Overview hints must not include [g]; got: {rendered:?}"
    );
    assert_eq!(
        spans,
        render_overview_hints(false),
        "the flag changes nothing"
    );
}

#[test]
fn overview_hints_with_agent_graph_flag_off_excludes_g_entry() {
    let spans = render_overview_hints(false);
    let rendered: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(
        !rendered.contains("[g]"),
        "Overview hints must NOT include [g] when toggle is off; got: {rendered:?}"
    );
    assert_debug_snapshot!(spans);
}
