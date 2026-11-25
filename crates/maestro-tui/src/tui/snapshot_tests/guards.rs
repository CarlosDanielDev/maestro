//! §02.24 Guards, release notes and small modals (#1276): the bypass
//! warning, the budget gate on Block, Release notes, and Code health from
//! the ⌃K palette. Every screen goes through `handle_key` + `ui::draw` at
//! 80x24 or larger.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use insta::assert_snapshot;

use super::log_drawer::{draw, log_app};
use crate::tui::app::{App, TuiCommand, TuiMode};
use crate::tui::input_handler::handle_key;

fn screen(app: &mut App, w: u16, h: u16) -> String {
    format!("{}", draw(app, w, h).backend())
}

async fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    handle_key(app, KeyEvent::new(code, mods)).await;
}

/// ⌃B on the Dashboard: the warning names both effects (with the flag),
/// what stays, and when it turns off.
#[tokio::test]
async fn bypass_warning() {
    let mut app = log_app(TuiMode::Dashboard);
    press(&mut app, KeyCode::Char('b'), KeyModifiers::CONTROL).await;
    assert_eq!(app.tui_mode, TuiMode::BypassWarning);
    let t = draw(&mut app, 80, 24);
    let out = format!("{}", t.backend());
    for want in [
        "What it turns off",
        "bypassPermissions",
        "auto-accepted",
        "What stays",
        "0 sessions will get bypass",
        "Turns off when",
    ] {
        assert!(out.contains(want), "missing {want:?}:\n{out}");
    }
    assert_snapshot!("bypass_warning", t.backend());
}

/// The gate on Block over the Dashboard: chip, SPEND rows, cap effect,
/// `S` locked.
#[test]
fn budget_gate_block() {
    let mut app = super::budget_prespawn::gated_app(1.2);
    let t = draw(&mut app, 80, 24);
    let out = format!("{}", t.backend());
    for want in [
        "BLOCK",
        "#1382 would pass the budget cap",
        "SPEND",
        "$1.20 of $1.00",
        "all agents pause",
        "S spawn (locked: over the cap)",
    ] {
        assert!(out.contains(want), "missing {want:?}:\n{out}");
    }
    assert_snapshot!("budget_gate_block", t.backend());
}

/// A changelog that never changes, so a release does not move the
/// snapshot (the compiled-in CHANGELOG.md does).
const NOTES: &str = "# Changelog\n\n## [1.2.0] - 2026-11-20\n\n### Added\n\n\
- A thing you can now do (#1)\n\n### Fixed\n\n- A crash on start (#2)\n\n\
## [1.1.0] - 2026-11-01\n\n### Changed\n\n- Faster scans (#3)\n";

/// Release notes from Home's `n`: a crumb screen with the shared footer.
#[tokio::test]
async fn release_notes() {
    let mut app = log_app(TuiMode::Landing);
    press(&mut app, KeyCode::Char('n'), KeyModifiers::NONE).await;
    assert_eq!(app.tui_mode, TuiMode::ReleaseNotes, "Home `n` opens it");
    app.screen_state.release_notes_screen =
        Some(crate::tui::screens::ReleaseNotesScreen::with_source(NOTES));
    let t = draw(&mut app, 80, 24);
    let out = format!("{}", t.backend());
    assert!(out.contains("› Release Notes"), "crumb:\n{out}");
    assert!(out.contains("A thing you can now do"), "notes:\n{out}");
    assert!(
        out.contains("Esc") && out.contains("Back"),
        "footer:\n{out}"
    );
    assert_snapshot!("release_notes", t.backend());
}

/// ⌃K, `code health`, ⏎: Code health opens on a scanning screen and the
/// scan is queued (it runs in the event loop, not in this test).
#[tokio::test]
async fn palette_opens_code_health() {
    for (w, h) in [(80, 24), (80, 27)] {
        let mut app = log_app(TuiMode::Dashboard);
        press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL).await;
        for c in "code health".chars() {
            press(&mut app, KeyCode::Char(c), KeyModifiers::NONE).await;
        }
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE).await;
        assert_eq!(app.tui_mode, TuiMode::Sanitize, "at {w}x{h}");
        assert!(
            (app.pending_commands.iter()).any(|c| matches!(c, TuiCommand::ScanCodeHealth)),
            "the scan is queued"
        );
        let out = screen(&mut app, w, h);
        assert!(out.contains("CODE HEALTH"), "drawn at {w}x{h}:\n{out}");
        assert!(out.contains("Scanning .rs files"), "says it scans:\n{out}");
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE).await;
        assert_eq!(app.tui_mode, TuiMode::Dashboard, "Esc comes back");
        assert!(app.screen_state.sanitize_screen.is_none());
    }
}
