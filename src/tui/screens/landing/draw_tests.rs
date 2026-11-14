use super::header::uses_compact_header;
use super::{
    ConsoleTier, console_row_heights, console_tier, menu_rows, release_category_counts,
    release_mix_text, should_emit_styles_from_env, welcome_layout, whats_new_highlights,
};
use crate::tui::screens::landing::LandingScreen;
use crate::tui::theme::Theme;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect, style::Color};

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

/// Draws the landing and returns the raw buffer.
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

/// Draws the landing (ASCII header, styled) and returns the buffer as text.
fn render_text(width: u16, height: u16) -> String {
    buffer_text(&render_buffer(width, height, false, true))
}

/// True when a sub-panel with this title is drawn (`┌Title` border corner).
fn has_panel(text: &str, title: &str) -> bool {
    text.contains(&format!("┌{title}"))
}

/// Row right under a panel's top border, from the panel's left corner on.
/// Returns (char column of the corner, rest of that row).
fn row_below_panel(text: &str, title: &str) -> (usize, String) {
    let lines: Vec<&str> = text.lines().collect();
    let needle = format!("┌{title}");
    let (i, line) = lines
        .iter()
        .enumerate()
        .find(|(_, l)| l.contains(&needle))
        .unwrap_or_else(|| panic!("missing panel {title}:\n{text}"));
    let byte = line.find(&needle).unwrap();
    let col = line[..byte].chars().count();
    let below: String = lines[i + 1].chars().skip(col).collect();
    (col, below)
}

#[test]
fn console_tier_edges() {
    let cases: &[(u16, u16, Option<ConsoleTier>)] = &[
        (0, 0, None),
        (60, 15, None),
        (79, 24, None),
        (79, 26, None),
        (79, 60, None),
        (80, 19, None),
        (200, 19, None),
        (80, 20, Some(ConsoleTier::Small)),
        (80, 23, Some(ConsoleTier::Small)),
        (80, 24, Some(ConsoleTier::Small)),
        (80, 25, Some(ConsoleTier::Small)),
        (200, 23, Some(ConsoleTier::Small)),
        (200, 25, Some(ConsoleTier::Small)),
        (80, 26, Some(ConsoleTier::Small)),
        (99, 40, Some(ConsoleTier::Small)),
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
    assert_eq!(ConsoleTier::Small.height_bounds(), (9, 9));
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
fn console_row_heights_small_has_summary_and_a_mini_chart_row() {
    assert_eq!(console_row_heights(ConsoleTier::Small, 7), [1, 0, 3]);
    assert_eq!(console_row_heights(ConsoleTier::Small, 20), [1, 0, 3]);
}

#[test]
fn layout_80x26_console_sits_between_menu_and_footer() {
    let area = Rect::new(0, 0, 80, 26);
    let l = welcome_layout(area);
    assert_eq!(l.console_tier, Some(ConsoleTier::Small));
    assert_eq!(l.console_height, 9);
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
fn layout_below_80x20_or_79_columns_has_no_console_tier() {
    for (w, h) in [(79, 40), (80, 19), (120, 19), (200, 19), (79, 22)] {
        assert_eq!(
            welcome_layout(Rect::new(0, 0, w, h)).console_tier,
            None,
            "{w}x{h}"
        );
    }
}

#[test]
fn console_title_shows_at_80x26_100x30_and_150x40() {
    for (w, h) in [(80, 26), (100, 30), (150, 40)] {
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

/// #1224: the Small tier keeps the console look (thin-bordered titled
/// panels), not a loose bullet list inside the console border.
#[test]
fn small_tier_draws_mini_panels_not_a_bullet_list() {
    for (w, h) in [(80, 26), (120, 28)] {
        let text = render_text(w, h);
        assert!(text.contains("Release Console v"), "{w}x{h}:\n{text}");
        for title in ["Mix", "Ref Trend", "Highlights"] {
            assert!(has_panel(&text, title), "{w}x{h}: missing {title}:\n{text}");
        }
        for title in ["Internet", "Link"] {
            assert!(
                !has_panel(&text, title),
                "{w}x{h}: unexpected {title}:\n{text}"
            );
        }
        assert!(!text.contains("<w> net"), "{w}x{h}: <w> net:\n{text}");
        assert!(text.contains("<q> quit"), "{w}x{h}: footer:\n{text}");

        // As many highlights as fit: one at the minimum height.
        let (cat, _) = &whats_new_highlights()[0];
        let prefix = format!("  [{}] ", cat.label());
        assert!(
            text.contains(&prefix),
            "{w}x{h}: missing {prefix:?}:\n{text}"
        );
    }
}

#[test]
fn small_mix_panel_shows_category_counts_as_text() {
    for (w, h) in [(80, 26), (120, 28)] {
        let text = render_text(w, h);
        let (label, n) = release_category_counts()[1];
        let want = format!("{label} {n}");
        let (mix_col, below) = row_below_panel(&text, "Mix");
        let (ref_col, _) = row_below_panel(&text, "Ref Trend");
        let mix_row: String = below.chars().take(ref_col - mix_col).collect();
        assert!(
            mix_row.contains(&want),
            "{w}x{h}: Mix row {mix_row:?} lacks {want:?}:\n{text}"
        );
    }
}

#[test]
fn small_ref_trend_panel_draws_a_sparkline() {
    for (w, h) in [(80, 26), (120, 28)] {
        let text = render_text(w, h);
        let (_, row) = row_below_panel(&text, "Ref Trend");
        assert!(
            row.chars().any(|c| "▁▂▃▄▅▆▇█".contains(c)),
            "{w}x{h}: no sparkline glyph in {row:?}:\n{text}"
        );
    }
}

#[test]
fn small_highlights_are_boxed() {
    let text = render_text(80, 26);
    let (_, row) = row_below_panel(&text, "Highlights");
    assert!(
        row.starts_with('│'),
        "highlight row must sit inside the box: {row:?}"
    );
}

#[test]
fn release_mix_text_lists_all_five_counts_when_wide() {
    let text = release_mix_text(200);
    for (label, n) in release_category_counts() {
        let want = format!("{label} {n}");
        assert!(text.contains(&want), "{text:?} lacks {want:?}");
    }
    assert!(!text.contains('…'), "wide text must not be cut: {text:?}");
}

#[test]
fn release_mix_text_is_ellipsized_when_narrow() {
    let text = release_mix_text(10);
    assert!(text.ends_with('…'), "{text:?}");
    assert!(text.chars().count() <= 10, "{text:?}");
}

#[test]
fn below_80_columns_or_20_rows_has_no_console() {
    for (w, h) in [(79, 40), (60, 20), (80, 19), (120, 19), (79, 24)] {
        let text = render_text(w, h);
        assert!(!text.contains("Release Console v"), "{w}x{h}:\n{text}");
    }
}

/// No size may draw the old "What's New in v" box, and none may panic.
#[test]
fn no_size_renders_the_old_whats_new_box() {
    let edge_w = [20u16, 60, 79, 80, 99, 100, 139, 140, 200];
    let edge_h = [12u16, 15, 19, 20, 23, 24, 25, 26, 28, 29, 30, 33, 34, 60];
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

// ---------------------------------------------------------------------------
// #1226: compact one-line header for 80+ columns and 20..26 rows.
// ---------------------------------------------------------------------------

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
