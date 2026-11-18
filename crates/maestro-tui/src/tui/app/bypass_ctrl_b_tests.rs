//! #1395: a bypass from config (or `--bypass-review`) does not count as
//! accepting the warning. After it is turned off, the next ⌃B opens the
//! typed-CONFIRM warning. Every key goes through the real `handle_key`
//! path, and the screen is drawn at 80x24 and 80x27 (the SizeGuard
//! minimum and one taller size).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use crate::review::bypass::BypassSource;
use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::handle_key;
use crate::tui::{make_test_app, ui};

/// The ⌃B warning title.
const WARNING_MARK: &str = "enable bypass";

/// What `setup.rs` does when `maestro.toml` sets bypass.
fn config_bypass_app() -> App {
    let mut app = make_test_app("ctrl-b-bypass");
    app.bypass_active = false;
    app.bypass_warning_acknowledged = false;
    app.activate_bypass(BypassSource::Config);
    app
}

async fn ctrl_b(app: &mut App) {
    handle_key(
        app,
        KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
    )
    .await;
}

/// Types CONFIRM (Shift letters, as a terminal sends them), then ⏎.
async fn type_confirm(app: &mut App) {
    for c in "CONFIRM".chars() {
        handle_key(app, KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT)).await;
    }
    handle_key(app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).await;
}

fn drawn_at(app: &mut App, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).expect("terminal");
    t.draw(|f| ui::draw(f, app)).expect("draw");
    let buf = t.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[tokio::test]
async fn ctrl_b_after_config_bypass_off_requires_typed_confirm() {
    for (w, h) in [(80, 24), (80, 27)] {
        let mut app = config_bypass_app();
        assert!(app.bypass_active, "setup: config bypass is on");

        ctrl_b(&mut app).await;
        assert!(!app.bypass_active, "first ⌃B turns bypass off");

        ctrl_b(&mut app).await;
        assert_eq!(app.tui_mode, TuiMode::BypassWarning, "at {w}x{h}");
        assert!(
            drawn_at(&mut app, w, h).contains(WARNING_MARK),
            "the typed-CONFIRM warning is drawn at {w}x{h}"
        );
        assert!(!app.bypass_active, "bypass stays off before CONFIRM");

        type_confirm(&mut app).await;
        assert!(app.bypass_active, "CONFIRM turns bypass on at {w}x{h}");
        assert!(!drawn_at(&mut app, w, h).contains(WARNING_MARK));
    }
}

#[tokio::test]
async fn ctrl_b_after_typed_confirm_toggles_without_warning() {
    let mut app = config_bypass_app();
    ctrl_b(&mut app).await;
    ctrl_b(&mut app).await;
    type_confirm(&mut app).await;
    assert!(app.bypass_active, "setup: CONFIRM accepted the warning");

    ctrl_b(&mut app).await;
    assert!(!app.bypass_active);
    ctrl_b(&mut app).await;
    assert!(app.bypass_active, "acknowledged this session: no warning");
    assert_ne!(app.tui_mode, TuiMode::BypassWarning);
    assert!(!drawn_at(&mut app, 80, 24).contains(WARNING_MARK));
}
