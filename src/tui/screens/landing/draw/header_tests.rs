//! Tests for the compact landing header (#1226).

use super::super::{ConsoleTier, menu_rows, welcome_layout};
use super::uses_compact_header;
use crate::tui::screens::landing::LandingScreen;
use crate::tui::theme::Theme;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect, style::Color};

fn render_buffer(width: u16, height: u16, nerd: bool, styled: bool) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let theme = Theme::dark();
    let screen = LandingScreen::new();
    terminal
        .draw(|f| screen.draw_impl_for_test(f, f.area(), &theme, nerd, styled))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn buffer_text(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_text(width: u16, height: u16) -> String {
    buffer_text(&render_buffer(width, height, false, true))
}

fn has_panel(text: &str, title: &str) -> bool {
    text.contains(&format!("┌{title}"))
}

#[test]
fn uses_compact_header_edges() {
    let cases: &[(u16, u16, bool)] = &[
        (0, 0, false),
        (80, 0, false),
        (80, 19, false),
        (80, 20, true),
        (80, 25, true),
        (80, 26, false),
        (79, 22, false),
        (79, 20, false),
        (120, 22, true),
        (200, 25, true),
        (200, 26, false),
        (200, 60, false),
    ];
    for &(w, h, want) in cases {
        assert_eq!(uses_compact_header(Rect::new(0, 0, w, h)), want, "{w}x{h}");
    }
}

#[test]
fn layout_80x20_to_80x25_uses_compact_header_and_small_console() {
    for h in 20u16..=25 {
        let area = Rect::new(0, 0, 80, h);
        let l = welcome_layout(area);
        assert!(l.compact_header, "80x{h}: compact flag");
        assert!(!l.show_header, "80x{h}: big logo must be off");
        assert_eq!(l.console_tier, Some(ConsoleTier::Small), "80x{h}");
        assert_eq!(l.console_height, 9, "80x{h}");
        assert_eq!(l.menu_y, 2 + (h - 20) / 2, "80x{h}: menu_y");
        assert_eq!(
            l.console_y,
            l.menu_y + menu_rows(l.columns) + 1,
            "80x{h}: console_y"
        );
        assert!(
            l.console_y + l.console_height <= l.footer_y,
            "80x{h}: console {}..{} vs footer {}",
            l.console_y,
            l.console_y + l.console_height,
            l.footer_y
        );
        assert!(l.footer_y < h, "80x{h}: footer off screen");
        assert!(l.console_x + l.console_width <= 80, "80x{h}: too wide");
    }
}

/// Exact budget at the minimum: 2 header + 6 menu + 1 gap + 9 console + 2 = 20.
#[test]
fn layout_80x20_exact_positions() {
    let l = welcome_layout(Rect::new(0, 0, 80, 20));
    assert_eq!(menu_rows(l.columns), 6);
    assert_eq!(l.menu_y, 2);
    assert_eq!(l.console_y, 9);
    assert_eq!(l.console_height, 9);
    assert_eq!(l.footer_y, 19);
}

#[test]
fn layout_big_header_cases_are_unchanged() {
    let cases: &[(u16, u16, bool)] = &[
        (80, 26, true),
        (120, 40, true),
        (180, 48, true),
        (80, 19, true),
        (79, 22, true),
        (79, 25, true),
        (60, 20, true),
        (59, 24, false),
    ];
    for &(w, h, show) in cases {
        let l = welcome_layout(Rect::new(0, 0, w, h));
        assert!(!l.compact_header, "{w}x{h}: compact must be off");
        assert_eq!(l.show_header, show, "{w}x{h}: show_header");
    }
}

#[test]
fn layout_120x22_uses_compact_header_and_small_console() {
    let l = welcome_layout(Rect::new(0, 0, 120, 22));
    assert!(l.compact_header && !l.show_header);
    assert_eq!(l.console_tier, Some(ConsoleTier::Small));
    assert_eq!(l.console_width, 116);
    assert_eq!(l.console_x, 2);
    assert!(l.console_y + l.console_height <= l.footer_y);
    assert!(l.footer_y < 22);
}

#[test]
fn compact_header_row0_has_brand_and_version_at_80x20() {
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    for (nerd, styled) in [(false, true), (true, true), (false, false), (true, false)] {
        let text = buffer_text(&render_buffer(80, 20, nerd, styled));
        let row0 = text.lines().next().unwrap();
        assert!(
            row0.contains("MAESTRO"),
            "nerd={nerd} styled={styled}: {row0:?}"
        );
        assert!(
            row0.contains(&version),
            "nerd={nerd} styled={styled}: {row0:?}"
        );
        assert!(
            !text.contains("|  \\/  |"),
            "nerd={nerd}: big ASCII logo:\n{text}"
        );
        assert!(
            !text.contains("███╗"),
            "nerd={nerd}: big block logo:\n{text}"
        );
    }
}

#[test]
fn compact_header_row_follows_the_centred_layout() {
    for h in 20u16..=25 {
        let text = render_text(80, h);
        let row = text.lines().nth(((h - 20) / 2) as usize).unwrap();
        assert!(row.contains("MAESTRO"), "80x{h}: {row:?}\n{text}");
    }
}

#[test]
fn compact_header_80x20_keeps_the_console_menu_and_footer() {
    for (nerd, styled) in [(false, true), (true, true), (false, false)] {
        let text = buffer_text(&render_buffer(80, 20, nerd, styled));
        assert!(text.contains("Release Console v"), "{text}");
        for title in ["Mix", "Ref Trend", "Highlights"] {
            assert!(has_panel(&text, title), "missing {title}:\n{text}");
        }
        assert!(text.contains("<q> quit"), "footer:\n{text}");
    }
}

#[test]
fn compact_header_is_styled_with_theme_tokens() {
    let theme = Theme::dark();
    let buf = render_buffer(80, 20, false, true);
    let row0: String = (0..80).map(|x| buf[(x, 0)].symbol()).collect();
    let brand_x = row0.find("MAESTRO").unwrap() as u16;
    let brand = &buf[(brand_x, 0)];
    assert_eq!(brand.fg, theme.branding_fg);
    assert_eq!(brand.bg, theme.branding_bg);
    let ver_x = row0
        .find(&format!("v{}", env!("CARGO_PKG_VERSION")))
        .unwrap() as u16;
    assert_eq!(buf[(ver_x, 0)].fg, theme.text_secondary);
}

#[test]
fn compact_header_plain_mode_emits_no_colors() {
    let buf = render_buffer(80, 20, false, false);
    for x in 0..80 {
        let cell = &buf[(x, 0)];
        assert_eq!(cell.fg, Color::Reset, "x={x}");
        assert_eq!(cell.bg, Color::Reset, "x={x}");
    }
}
