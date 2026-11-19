//! #1267: `u` with live work queues the upgrade instead of downloading;
//! the queued upgrade starts once the fleet is idle. Keys go through the
//! real key path at 80x24 and the status bar is read from a drawn screen.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

use crate::session::types::SessionStatus;
use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::{handle_key, handle_key_under_size_guard};
use crate::tui::snapshot_tests::make_session;
use crate::tui::ui;
use crate::updater::{ReleaseInfo, UpgradeState};

/// The smallest size that is laid out: keys reach the screens.
const AREA: Rect = Rect::new(0, 0, 80, 24);

/// A non-GitHub URL fails the installer's trust check before any network
/// call or file write, so a started download is harmless.
fn available() -> UpgradeState {
    UpgradeState::Available(ReleaseInfo {
        tag: "v99.0.0".into(),
        version: "99.0.0".into(),
        download_url: "http://127.0.0.1/untrusted-1267".into(),
    })
}

/// Overview with an update available and `live` queued running sessions.
fn app_with_live(live: usize) -> App {
    let mut app = crate::tui::make_test_app("upgrade-queue-1267");
    app.tui_mode = TuiMode::Overview;
    app.upgrade_state = available();
    for _ in 0..live {
        app.pool.enqueue(make_session(SessionStatus::Running, None));
    }
    app
}

async fn press_u(app: &mut App) {
    let key = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE);
    if handle_key_under_size_guard(app, key, AREA).await.is_none() {
        handle_key(app, key).await;
    }
}

/// The top row at 120x40: at 80 columns the update chip is the first
/// segment to drop (#1257), so it is read where it fits.
fn status_row(app: &mut App) -> String {
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    t.draw(|f| ui::draw(f, app)).unwrap();
    format!("{}", t.backend())
        .lines()
        .next()
        .unwrap_or_default()
        .to_string()
}

fn is_queued(state: &UpgradeState) -> bool {
    matches!(state, UpgradeState::Queued(info) if info.version == "99.0.0")
}

#[tokio::test]
async fn u_queues_when_fleet_busy() {
    let mut app = app_with_live(2);
    press_u(&mut app).await;
    assert!(
        is_queued(&app.upgrade_state),
        "u with running sessions must queue, not download: {:?}",
        app.upgrade_state
    );
    let row = status_row(&mut app);
    assert!(row.contains("↑ queued · after 2 agents"), "{row}");

    // `u` again unqueues: back to the plain chip, still no download.
    press_u(&mut app).await;
    assert!(
        matches!(app.upgrade_state, UpgradeState::Available(_)),
        "{:?}",
        app.upgrade_state
    );
    let row = status_row(&mut app);
    assert!(row.contains("↑ 99.0.0 u"), "{row}");
}

#[tokio::test]
async fn queued_upgrade_runs_when_idle() {
    let mut app = app_with_live(1);
    press_u(&mut app).await;
    assert!(is_queued(&app.upgrade_state), "{:?}", app.upgrade_state);

    // Still busy: the tick keeps it queued.
    app.start_queued_upgrade_if_idle();
    assert!(is_queued(&app.upgrade_state), "{:?}", app.upgrade_state);

    // The last session finishes (promoted by hand so the tick spawns nothing).
    assert_eq!(app.pool.try_promote().len(), 1);
    let id = Uuid::nil();
    app.pool.get_active_mut(id).expect("active").session.status = SessionStatus::Completed;
    assert!(app.pool.finalize(id));

    app.check_completions().await.expect("tick");
    assert!(
        matches!(
            app.upgrade_state,
            UpgradeState::Downloading { ref version } if version == "99.0.0"
        ),
        "the idle tick must start the queued upgrade: {:?}",
        app.upgrade_state
    );
    let toast = app
        .toasts
        .visible(Instant::now())
        .expect("a toast announces it");
    assert!(toast.message.contains("99.0.0"), "{}", toast.message);
}
