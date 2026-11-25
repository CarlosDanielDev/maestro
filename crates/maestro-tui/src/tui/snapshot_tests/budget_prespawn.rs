use crate::tui::budget_prespawn::{GateView, draw, modal_rect};
use crate::tui::theme::Theme;
use insta::assert_snapshot;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// The Usage "What happens" words for the cap.
const CAP_WHAT: &str = "all agents pause";

fn view(spent: f64, blocked: bool) -> GateView {
    GateView {
        session: "#415".into(),
        spent,
        next: 5.0,
        per_session: 10.0,
        cap: 50.0,
        blocked,
        fits: false,
    }
}

fn render(v: &GateView) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let theme = Theme::dark();
    terminal
        .draw(|f| draw(f, v, CAP_WHAT, modal_rect(f.area()), &theme))
        .unwrap();
    terminal
}

/// Warn: the session, every SPEND row, the worst case that passes the cap
/// and the cap effect.
#[test]
fn budget_prespawn_modal_warn_shows_projected_and_limit() {
    let terminal = render(&view(37.0, false));
    let output = format!("{}", terminal.backend());
    for want in [
        "WARN",
        "#415 would use 84% of the budget",
        "$37.00 of $50.00",
        "$13.00 left",
        "$5.00 est. for the next turn",
        "$42.00",
        "$47.00",
        "under the cap",
        "all agents pause; new spawns wait",
        "S spawn now",
    ] {
        assert!(output.contains(want), "missing {want:?}:\n{output}");
    }
    assert_snapshot!(terminal.backend());
}

#[test]
fn budget_prespawn_modal_block_shows_projected_and_limit() {
    let terminal = render(&view(46.0, true));
    let output = format!("{}", terminal.backend());
    for want in [
        "BLOCK",
        "#415 would pass the budget cap",
        "$46.00 of $50.00",
        "$51.00",
        "$56.00",
        "! passes the cap",
        "S spawn (locked: over the cap)",
    ] {
        assert!(output.contains(want), "missing {want:?}:\n{output}");
    }
    assert_snapshot!(terminal.backend());
}

/// After `b` raised the cap the gate says the session now fits: no WARN
/// chip for a warning that no longer applies.
#[test]
fn budget_prespawn_modal_after_cap_raise_says_it_fits() {
    let mut v = view(10.0, false);
    v.fits = true;
    let output = format!("{}", render(&v).backend());
    assert!(
        output.contains(" OK ") && output.contains("now fits the budget"),
        "{output}"
    );
    assert!(!output.contains("WARN"), "{output}");
}

/// The frame is `ds::panel` (no `branding_bg` surface): Double (danger)
/// on Block, Rounded (overlay) on Warn.
#[test]
fn budget_prespawn_modal_no_branding_bg() {
    for (blocked, corner) in [(true, "╔"), (false, "╭")] {
        let t = render(&view(40.0, blocked));
        let area = modal_rect(t.backend().buffer().area);
        let cell = &t.backend().buffer()[(area.x, area.y)];
        assert_eq!(cell.symbol(), corner, "blocked={blocked}");
        assert_ne!(cell.bg, Theme::dark().branding_bg, "blocked={blocked}");
    }
}

/// The gate's keys are drawn in the dialog: `S` (locked on Block), `b`
/// and `n`; the old `[y]es [n]o [s]kip` row is gone.
#[test]
fn budget_prespawn_modal_chord_hints_exact_text() {
    for blocked in [false, true] {
        let output = format!("{}", render(&view(40.0, blocked)).backend());
        assert!(
            output.contains("b raise cap") && output.contains("n drop"),
            "{output}"
        );
        assert!(
            !output.contains("[y]es") && !output.contains("[s]kip"),
            "{output}"
        );
    }
}

/// A full app parked in the gate for one queued session, `total_cost`
/// already spent against a $1 cap.
pub(super) fn gated_app(total_cost: f64) -> crate::tui::app::App {
    use crate::session::types::Session;
    use crate::tui::app::TuiMode;
    let session = Session::new(
        "probe".into(),
        "opus".into(),
        "orchestrator".into(),
        Some(1382),
        None,
    );
    let id = session.id;
    let mut app = super::log_drawer::log_app(TuiMode::BudgetPreSpawn { session_id: id });
    app.pool.enqueue(session);
    app.install_budget(&crate::config::BudgetConfig {
        per_session_usd: 0.5,
        total_usd: 1.0,
        alert_threshold_pct: 80,
    });
    app.total_cost = total_cost;
    app
}

fn footer(t: &Terminal<TestBackend>) -> String {
    let buf = t.backend().buffer();
    let y = buf.area.height - 1;
    (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
}

/// #1382: on Block `S` does nothing (#1243), so the footer offers only
/// the keys that act: `b` and `n`.
#[test]
fn budget_prespawn_block_footer_hints() {
    let mut app = gated_app(1.2);
    let t = super::log_drawer::draw(&mut app, 80, 24);
    let row = footer(&t);
    assert!(
        row.contains("raise cap") && row.contains("drop"),
        "b and n are offered: {row}"
    );
    assert!(!row.contains("spawn"), "Block must not offer S: {row}");
    assert_snapshot!(t.backend());
}

/// Under the cap (Warn) `S` acts, so the footer offers it.
#[test]
fn budget_prespawn_warn_footer_keeps_proceed() {
    let mut app = gated_app(0.2);
    let row = footer(&super::log_drawer::draw(&mut app, 80, 24));
    assert!(row.contains("S") && row.contains("spawn"), "{row}");
}
