use crate::tui::app::TuiMode;
use crate::tui::keybinding_hints::keybinding_hints_spans;
use crate::tui::navigation::keymap::{fit_hints_to_width, mode_keymap};
use crate::tui::theme::Theme;
use ratatui::text::Span;

fn render_dashboard_hints(agent_graph_enabled: bool) -> Vec<Span<'static>> {
    let km = mode_keymap(TuiMode::Dashboard, None, &[], agent_graph_enabled);
    let theme = Theme::dark();
    let fitted = fit_hints_to_width(km.hints, 120);
    keybinding_hints_spans(&fitted, false, &theme)
}

/// #1292: the old `g` toggle is gone (the graph is Dashboard › Deps ›
/// files, #1294 removed the old mode), so no `[g] Graph` hint comes back
/// with the flag on or off.
#[test]
fn dashboard_hints_have_no_g_entry_with_the_agent_graph_flag_on_or_off() {
    let spans = render_dashboard_hints(true);
    let rendered: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(
        !rendered.contains("[g]"),
        "Dashboard hints must not include [g]; got: {rendered:?}"
    );
    assert_eq!(
        spans,
        render_dashboard_hints(false),
        "the flag changes nothing"
    );
}
