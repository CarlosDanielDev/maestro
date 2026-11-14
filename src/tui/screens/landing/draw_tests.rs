use super::{
    ConsoleTier, console_row_heights, console_tier, should_emit_styles_from_env, welcome_layout,
    whats_new_highlights,
};
use crate::tui::screens::landing::LandingScreen;
use crate::tui::theme::Theme;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

#[test]
fn styles_are_disabled_when_no_color_is_present() {
    assert!(!should_emit_styles_from_env(|_| None, true));
}

#[test]
fn styles_are_disabled_for_dumb_term() {
    assert!(!should_emit_styles_from_env(
        |key| (key == "TERM").then(|| "dumb".to_string()),
        false
    ));
}

#[test]
fn styles_are_enabled_for_normal_term() {
    assert!(should_emit_styles_from_env(
        |key| (key == "TERM").then(|| "xterm-256color".to_string()),
        false
    ));
}

/// The footer carries the quit and notes hints. At 60x20 the grown menu
/// leaves no room for the blank line above the footer; the footer must still
/// land inside the area.
#[test]
fn footer_stays_inside_the_area_at_60x20_when_the_menu_grows() {
    let area = Rect::new(0, 0, 60, 20);
    let layout = welcome_layout(area);

    assert!(
        layout.footer_y < area.y + area.height,
        "footer at row {} is off the 20-row area",
        layout.footer_y
    );
}

// ---------------------------------------------------------------------------
// #1218: one landing. The Release Console renders at every size the slot
// shows, dropping panels by tier.
// ---------------------------------------------------------------------------

/// Draws the landing (ASCII header, styled) and returns the buffer as text.
fn render_text(width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let theme = Theme::dark();
    let screen = LandingScreen::new();
    terminal
        .draw(|f| screen.draw_impl_for_test(f, f.area(), &theme, false, true))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// True when a sub-panel with this title is drawn (`┌Title` border corner).
fn has_panel(text: &str, title: &str) -> bool {
    text.contains(&format!("┌{title}"))
}

/// Every highlight of the current version shows as `  [<Category>] `.
/// Derived from `whats_new_highlights()`: never hard-code a category, the
/// next patch release may have no `[Added]` items.
fn assert_all_highlight_prefixes_present(text: &str) {
    let items = whats_new_highlights();
    assert!(!items.is_empty(), "current version must have highlights");
    for (cat, _) in &items {
        let prefix = format!("  [{}] ", cat.label());
        assert!(text.contains(&prefix), "missing {prefix:?}:\n{text}");
    }
}

#[test]
fn console_tier_edges() {
    let cases: &[(u16, u16, Option<ConsoleTier>)] = &[
        (0, 0, None),
        (60, 15, None),
        (79, 24, None),
        (80, 23, None),
        (200, 23, None),
        (79, 60, None),
        (80, 24, Some(ConsoleTier::Small)),
        (99, 60, Some(ConsoleTier::Small)),
        (100, 29, Some(ConsoleTier::Small)),
        (100, 30, Some(ConsoleTier::Medium)),
        (110, 32, Some(ConsoleTier::Medium)),
        (139, 60, Some(ConsoleTier::Medium)),
        (140, 33, Some(ConsoleTier::Medium)),
        (140, 34, Some(ConsoleTier::Full)),
        (150, 40, Some(ConsoleTier::Full)),
        (200, 60, Some(ConsoleTier::Full)),
    ];
    for &(w, h, want) in cases {
        assert_eq!(console_tier(Rect::new(0, 0, w, h)), want, "{w}x{h}");
    }
}

#[test]
fn console_tier_height_bounds() {
    assert_eq!(ConsoleTier::Small.height_bounds(), (7, 7));
    assert_eq!(ConsoleTier::Medium.height_bounds(), (13, 18));
    assert_eq!(ConsoleTier::Full.height_bounds(), (16, 30));
}

#[test]
fn console_row_heights_full_keeps_the_old_formula() {
    assert_eq!(console_row_heights(ConsoleTier::Full, 28), [2, 10, 9]);
    assert_eq!(console_row_heights(ConsoleTier::Full, 14), [2, 6, 5]);
}

#[test]
fn console_row_heights_medium_drops_the_network_row() {
    assert_eq!(console_row_heights(ConsoleTier::Medium, 11), [1, 0, 5]);
    assert_eq!(console_row_heights(ConsoleTier::Medium, 16), [1, 0, 9]);
    assert_eq!(console_row_heights(ConsoleTier::Medium, 30), [1, 0, 9]);
}

#[test]
fn console_row_heights_small_is_summary_only() {
    assert_eq!(console_row_heights(ConsoleTier::Small, 5), [1, 0, 0]);
    assert_eq!(console_row_heights(ConsoleTier::Small, 20), [1, 0, 0]);
}

#[test]
fn layout_80x24_console_sits_between_menu_and_footer() {
    let area = Rect::new(0, 0, 80, 24);
    let l = welcome_layout(area);
    assert_eq!(l.console_tier, Some(ConsoleTier::Small));
    assert!(l.console_y > l.menu_y, "console must be below the menu");
    assert!(
        l.console_y + l.console_height <= l.footer_y,
        "console rows {}..{} overlap footer row {}",
        l.console_y,
        l.console_y + l.console_height,
        l.footer_y
    );
    assert!(
        l.footer_y < area.height,
        "footer off screen: {}",
        l.footer_y
    );
    assert!(l.console_x + l.console_width <= area.width);
}

#[test]
fn layout_console_fits_at_100x30_and_140x34() {
    for (w, h, tier) in [
        (100u16, 30u16, ConsoleTier::Medium),
        (140, 34, ConsoleTier::Full),
    ] {
        let l = welcome_layout(Rect::new(0, 0, w, h));
        assert_eq!(l.console_tier, Some(tier), "{w}x{h}");
        assert!(
            l.console_y + l.console_height <= l.footer_y,
            "{w}x{h}: console {}..{} vs footer {}",
            l.console_y,
            l.console_y + l.console_height,
            l.footer_y
        );
        assert!(l.footer_y < h, "{w}x{h}: footer off screen");
        assert!(l.console_x + l.console_width <= w, "{w}x{h}: too wide");
    }
}

/// The Full tier keeps today's geometry (wide snapshot stays byte-identical).
#[test]
fn layout_full_tier_matches_the_old_wide_deck() {
    assert_eq!(welcome_layout(Rect::new(0, 0, 150, 40)).console_height, 23);
    assert_eq!(welcome_layout(Rect::new(0, 0, 180, 48)).console_height, 30);
    assert_eq!(welcome_layout(Rect::new(0, 0, 180, 48)).console_width, 180);
}

#[test]
fn layout_below_80x24_has_no_console_tier() {
    assert_eq!(welcome_layout(Rect::new(0, 0, 79, 40)).console_tier, None);
    assert_eq!(welcome_layout(Rect::new(0, 0, 120, 23)).console_tier, None);
}

#[test]
fn console_title_shows_at_80x24_100x30_and_150x40() {
    for (w, h) in [(80, 24), (100, 30), (150, 40)] {
        let text = render_text(w, h);
        assert!(text.contains("Release Console v"), "{w}x{h}:\n{text}");
    }
}

#[test]
fn full_150x40_draws_every_panel_and_the_net_hint() {
    let text = render_text(150, 40);
    for title in ["Internet", "Link", "Mix", "Ref Trend", "Highlights"] {
        assert!(has_panel(&text, title), "missing panel {title}:\n{text}");
    }
    assert!(
        text.contains("<w> net"),
        "Full footer needs <w> net:\n{text}"
    );
}

#[test]
fn medium_110x32_drops_internet_and_link_and_the_net_hint() {
    let text = render_text(110, 32);
    for title in ["Mix", "Ref Trend", "Highlights"] {
        assert!(has_panel(&text, title), "missing panel {title}:\n{text}");
    }
    for title in ["Internet", "Link"] {
        assert!(
            !has_panel(&text, title),
            "unexpected panel {title}:\n{text}"
        );
    }
    assert!(
        !text.contains("<w> net"),
        "Medium footer has <w> net:\n{text}"
    );
    assert!(
        text.contains("<q> quit"),
        "footer must stay visible:\n{text}"
    );
}

#[test]
fn small_80x24_shows_only_summary_and_highlights() {
    let text = render_text(80, 24);
    assert!(text.contains("Release Console v"), "{text}");
    assert_all_highlight_prefixes_present(&text);
    for title in ["Internet", "Link", "Mix", "Ref Trend"] {
        assert!(
            !has_panel(&text, title),
            "unexpected panel {title}:\n{text}"
        );
    }
    assert!(
        !text.contains("<w> net"),
        "Small footer has <w> net:\n{text}"
    );
    assert!(
        text.contains("<q> quit"),
        "footer must stay visible:\n{text}"
    );
}

#[test]
fn below_80_columns_or_24_rows_has_no_console() {
    for (w, h) in [(79, 40), (120, 23), (60, 20)] {
        let text = render_text(w, h);
        assert!(!text.contains("Release Console v"), "{w}x{h}:\n{text}");
    }
}

/// No size may draw the old "What's New in v" box, and none may panic.
#[test]
fn no_size_renders_the_old_whats_new_box() {
    let edge_w = [20u16, 60, 79, 80, 99, 100, 139, 140, 200];
    let edge_h = [12u16, 15, 23, 24, 29, 30, 33, 34, 60];
    // Degenerate sizes first: they must not panic either.
    let mut sizes: Vec<(u16, u16)> = vec![(0, 0), (1, 1), (1, 24), (80, 1)];
    for &w in &edge_w {
        for &h in &edge_h {
            sizes.push((w, h));
        }
    }
    for w in (60..=200).step_by(20) {
        for h in (15..=60).step_by(9) {
            sizes.push((w, h));
        }
    }
    for (w, h) in sizes {
        let text = render_text(w, h);
        assert!(!text.contains("What's New in v"), "{w}x{h}:\n{text}");
    }
}
