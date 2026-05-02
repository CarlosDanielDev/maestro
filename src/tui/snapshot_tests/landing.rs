use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend, style::Color};

use crate::tui::screens::landing::LandingScreen;
use crate::tui::screens::landing::draw::welcome_layout;
use crate::tui::theme::Theme;

fn render_landing(
    width: u16,
    height: u16,
    use_nerd_font: bool,
    styled: bool,
) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let theme = Theme::dark();
    let screen = LandingScreen::new();

    terminal
        .draw(|f| {
            screen.draw_impl_for_test(f, f.area(), &theme, use_nerd_font, styled);
        })
        .unwrap();

    terminal
}

#[test]
fn landing_welcome_80x24_ascii() {
    let terminal = render_landing(80, 24, false, true);
    assert_snapshot!(terminal.backend());
}

#[test]
fn landing_welcome_80x24_nerd_font() {
    let terminal = render_landing(80, 24, true, true);
    assert_snapshot!(terminal.backend());
}

#[test]
fn landing_welcome_120x40_ascii() {
    let terminal = render_landing(120, 40, false, true);
    assert_snapshot!(terminal.backend());
}

#[test]
fn landing_welcome_120x40_nerd_font() {
    let terminal = render_landing(120, 40, true, true);
    assert_snapshot!(terminal.backend());
}

#[test]
fn landing_welcome_60x20_ascii() {
    let terminal = render_landing(60, 20, false, true);
    assert_snapshot!(terminal.backend());
}

#[test]
fn landing_welcome_60x20_nerd_font() {
    let terminal = render_landing(60, 20, true, true);
    assert_snapshot!(terminal.backend());
}

#[test]
fn landing_welcome_below_60_hides_header() {
    let terminal = render_landing(50, 16, false, true);
    assert_snapshot!(terminal.backend());
}

#[test]
fn landing_welcome_very_narrow_truncates_with_ellipsis() {
    let terminal = render_landing(20, 12, false, true);
    let rendered = format!("{:?}", terminal.backend());
    assert!(
        rendered.contains('…'),
        "narrow welcome menu must truncate with ellipsis:\n{rendered}"
    );
}

#[test]
fn landing_layout_keeps_symmetric_menu_padding() {
    for width in [20, 50, 60, 80, 120] {
        let layout = welcome_layout(ratatui::layout::Rect::new(0, 0, width, 24));
        let delta = layout.left_padding.abs_diff(layout.right_padding);
        assert!(
            delta <= 1,
            "width {width}: left={} right={}",
            layout.left_padding,
            layout.right_padding
        );
        if width >= 4 {
            assert!(
                layout.left_padding >= 2 && layout.right_padding >= 2,
                "width {width}: padding should be at least 2 columns"
            );
        }
    }
}

#[test]
fn landing_resize_rerenders_at_new_dimensions() {
    let theme = Theme::dark();
    let screen = LandingScreen::new();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();

    terminal
        .draw(|f| {
            screen.draw_impl_for_test(f, f.area(), &theme, false, true);
        })
        .unwrap();
    terminal.backend_mut().resize(80, 24);
    terminal
        .resize(ratatui::layout::Rect::new(0, 0, 80, 24))
        .unwrap();
    terminal
        .draw(|f| {
            screen.draw_impl_for_test(f, f.area(), &theme, false, true);
        })
        .unwrap();

    let expected = render_landing(80, 24, false, true);
    assert_eq!(
        format!("{:?}", terminal.backend()),
        format!("{:?}", expected.backend())
    );
}

#[test]
fn landing_plain_mode_emits_no_styles() {
    let terminal = render_landing(80, 24, true, false);
    let buffer = terminal.backend().buffer();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let style = buffer[(x, y)].style();
            let fg_plain = style.fg.is_none_or(|color| color == Color::Reset);
            let bg_plain = style.bg.is_none_or(|color| color == Color::Reset);
            let underline_plain = style
                .underline_color
                .is_none_or(|color| color == Color::Reset);
            assert!(
                fg_plain
                    && bg_plain
                    && underline_plain
                    && style.add_modifier.is_empty()
                    && style.sub_modifier.is_empty(),
                "style at ({x},{y}) was {style:?}"
            );
        }
    }
}
