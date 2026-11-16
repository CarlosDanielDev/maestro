use insta::assert_snapshot;
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier},
};

use super::*;

fn top_left(block: Block<'_>) -> (String, Color, bool) {
    let area = Rect::new(0, 0, 4, 3);
    let mut buf = Buffer::empty(area);
    ratatui::widgets::Widget::render(block, area, &mut buf);
    let cell = &buf[(0, 0)];
    (
        cell.symbol().to_string(),
        cell.fg,
        cell.modifier.contains(Modifier::BOLD),
    )
}

#[test]
fn panel_focus_border_types() {
    let theme = Theme::dark();
    let cases = [
        (
            Focus::Inactive,
            BorderType::Plain,
            "┌",
            theme.border_inactive,
        ),
        (Focus::Active, BorderType::Thick, "┏", theme.border_active),
        (Focus::Overlay, BorderType::Rounded, "╭", theme.accent_info),
        (Focus::Danger, BorderType::Double, "╔", theme.accent_error),
    ];
    for (focus, border_type, glyph, colour) in cases {
        assert_eq!(focus.border_type(), border_type, "{focus:?}");
        let (symbol, fg, _) = top_left(panel_plain(focus, &theme));
        assert_eq!(symbol, glyph, "{focus:?} corner");
        assert_eq!(fg, colour, "{focus:?} colour");
        let (titled, _, _) = top_left(panel("T", focus, &theme));
        assert_eq!(titled, glyph, "{focus:?} titled corner");
    }
}

#[test]
fn panel_active_border_is_bold() {
    let theme = Theme::dark();
    assert!(top_left(panel_plain(Focus::Active, &theme)).2);
    assert!(!top_left(panel_plain(Focus::Inactive, &theme)).2);
}

#[test]
fn styled_block_maps_focused_to_active_and_unfocused_to_inactive() {
    let theme = Theme::dark();
    assert_eq!(
        top_left(theme.styled_block("T", true)),
        top_left(panel("T", Focus::Active, &theme))
    );
    assert_eq!(
        top_left(theme.styled_block("T", false)),
        top_left(panel("T", Focus::Inactive, &theme))
    );
    assert_eq!(
        top_left(theme.styled_block_plain(true)),
        top_left(panel_plain(Focus::Active, &theme))
    );
}

/// NoColor: every colour is `Reset`, so the four focus states must still
/// differ by border shape (and Active by BOLD).
#[test]
fn panel_focus_no_color() {
    let theme = Theme::no_color();
    let mut terminal = Terminal::new(TestBackend::new(64, 4)).unwrap();
    terminal
        .draw(|f| {
            let cols = Layout::horizontal([Constraint::Fill(1); 4]).split(f.area());
            let focuses = [
                Focus::Inactive,
                Focus::Active,
                Focus::Overlay,
                Focus::Danger,
            ];
            for (focus, area) in focuses.into_iter().zip(cols.iter()) {
                f.render_widget(panel(&format!("{focus:?}"), focus, &theme), *area);
            }
        })
        .unwrap();
    let out = format!("{:?}", terminal.backend().buffer());
    for colour in ["Green", "Yellow", "Red", "Blue", "Cyan", "Rgb", "Indexed"] {
        assert!(!out.contains(colour), "NoColor drew {colour}: {out}");
    }
    assert_snapshot!(out);
}
