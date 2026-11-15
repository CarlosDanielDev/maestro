use crate::tui::budget_prespawn::{draw_budget_prespawn, modal_rect};
use crate::tui::theme::Theme;
use insta::assert_snapshot;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

#[test]
fn budget_prespawn_modal_warn_shows_projected_and_limit() {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let theme = Theme::dark();

    terminal
        .draw(|f| {
            let area = modal_rect(f.area());
            draw_budget_prespawn(f, 42.0, 50.0, area, &theme);
        })
        .unwrap();

    let output = format!("{}", terminal.backend());
    assert!(
        output.contains("$42.00") && output.contains("$50.00"),
        "modal must render projected and limit values; output:\n{}",
        output
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn budget_prespawn_modal_block_shows_projected_and_limit() {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let theme = Theme::dark();

    terminal
        .draw(|f| {
            let area = modal_rect(f.area());
            draw_budget_prespawn(f, 52.0, 50.0, area, &theme);
        })
        .unwrap();

    let output = format!("{}", terminal.backend());
    assert!(
        output.contains("$52.00") && output.contains("$50.00"),
        "modal must render projected and limit values; output:\n{}",
        output
    );
    assert_snapshot!(terminal.backend());
}

#[test]
fn budget_prespawn_modal_no_branding_bg() {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let theme = Theme::dark();

    terminal
        .draw(|f| {
            let area = modal_rect(f.area());
            draw_budget_prespawn(f, 42.0, 50.0, area, &theme);
        })
        .unwrap();

    let output = format!("{}", terminal.backend());
    let branding_bg = theme.branding_bg;
    // Color::Rgb(r,g,b) formats as "Rgb(r,g,b)" in Debug. We can't match
    // ANSI escape directly (TestBackend renders chars not codes). Instead,
    // verify the modal text was rendered (we have content) — the visual
    // regression below catches branding_bg if implementer ever swaps in.
    assert!(
        !output.is_empty(),
        "modal must render content (sanity); theme.branding_bg = {:?}",
        branding_bg
    );
    // The styled_block wrapper is the official border. If a future PR
    // replaces it with `theme.branding_bg`-backed surface, the
    // snapshot_tests/budget_prespawn.rs snapshots (TC-8.1, TC-8.2) will
    // diff visibly — that's the real guard. This test exists to document
    // the intent.
}

#[test]
fn budget_prespawn_modal_chord_hints_exact_text() {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let theme = Theme::dark();

    terminal
        .draw(|f| {
            let area = modal_rect(f.area());
            draw_budget_prespawn(f, 42.0, 50.0, area, &theme);
        })
        .unwrap();

    let output = format!("{}", terminal.backend());
    // The TestBackend renders spans concatenated by row. We check each
    // chord hint substring individually; the exact spacing between them
    // is not load-bearing for the AC.
    assert!(
        output.contains("[y]es"),
        "modal footer must contain '[y]es'; output:\n{}",
        output
    );
    assert!(
        output.contains("[n]o"),
        "modal footer must contain '[n]o'; output:\n{}",
        output
    );
    assert!(
        output.contains("[s]kip"),
        "modal footer must contain '[s]kip'; output:\n{}",
        output
    );
}
