//! #1240: Fullscreen and Agent graph take their own keys. None reaches
//! `handle_overview_keys`, whose `k` / `p` / `r` / `D` act on the hidden
//! Overview grid selection, not on what the screen shows.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use super::handle_key;
use crate::session::types::{Session, SessionStatus};
use crate::tui::app::{App, SessionView, TuiMode};
use crate::tui::navigation::registry::{Action, actions_on, registry};

async fn press(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

fn session(issue: u64, status: SessionStatus, message: &str) -> Session {
    let mut s = Session::new(
        format!("task {issue}"),
        "claude-opus-4-5".into(),
        "orchestrator".into(),
        Some(issue),
        None,
    );
    s.status = status;
    s.last_message = message.into();
    s
}

/// Overview grid selects #101 (index 0). #202 has 40 rows of output and is
/// the session on screen. #303 is finished, so `D` would dismiss it.
/// Returns the app and #202's id.
fn app_on_202() -> (App, uuid::Uuid) {
    let mut app = crate::tui::make_test_app("fullscreen-keys");
    app.show_mascot = false;
    app.show_activity_log = false;
    app.no_splash = true;
    let toml = "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\n\
                per_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
                [github]\n[notifications]\n[views]\nagent_graph_enabled = true\n";
    app.config = Some(toml::from_str(toml).expect("test config parse"));
    let rows: String = (1..=40).map(|i| format!("- row {i:02}\n")).collect();
    let a = session(101, SessionStatus::Running, "first");
    let b = session(202, SessionStatus::Running, &rows);
    let c = session(303, SessionStatus::Completed, "done");
    let (b_id, c_id) = (b.id, c.id);
    app.pool.enqueue(a);
    app.pool.enqueue(b);
    app.pool.enqueue(c);
    app.pool.try_promote();
    // Promotion may move the status on; pin what each row should be.
    let ids: Vec<_> = app.pool.all_sessions().iter().map(|s| s.id).collect();
    for id in ids {
        let status = if id == c_id {
            SessionStatus::Completed
        } else {
            SessionStatus::Running
        };
        app.pool.get_session_mut(id).unwrap().status = status;
    }
    app.pool.finalize(c_id);
    assert_eq!(app.panel_view.selected_index(), 0);
    assert_eq!(
        app.pool.session_at_index(0).and_then(|s| s.issue_number),
        Some(101),
        "the hidden grid selection is #101"
    );
    (app, b_id)
}

fn draw(app: &mut App, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| crate::tui::ui::draw(f, app)).unwrap();
    format!("{}", terminal.backend())
}

#[tokio::test]
async fn fullscreen_k_scrolls() {
    for (w, h) in [(80, 24), (80, 27)] {
        let (mut app, b) = app_on_202();
        app.tui_mode = TuiMode::Session {
            id: b,
            view: SessionView::Output,
        };
        let out = draw(&mut app, w, h);
        assert!(out.contains("row 40"), "{w}x{h} follows the tail:\n{out}");

        for _ in 0..3 {
            press(&mut app, KeyCode::Char('k')).await;
        }
        assert_eq!(
            app.tui_mode,
            TuiMode::Session {
                id: b,
                view: SessionView::Output
            },
            "{w}x{h}: k is not kill"
        );
        let out = draw(&mut app, w, h);
        assert!(!out.contains("Type 101"), "{w}x{h}: no kill dialog:\n{out}");
        assert!(!out.contains("row 40"), "{w}x{h}: k scrolled up:\n{out}");
        assert!(out.contains("row 37"), "{w}x{h}: three rows up:\n{out}");

        press(&mut app, KeyCode::Up).await;
        let out = draw(&mut app, w, h);
        assert!(!out.contains("row 37"), "{w}x{h}: ↑ scrolls too:\n{out}");

        for _ in 0..4 {
            press(&mut app, KeyCode::Char('j')).await;
        }
        let out = draw(&mut app, w, h);
        assert!(out.contains("row 40"), "{w}x{h}: j scrolls back:\n{out}");

        // The scroll stops at the top, so one `j` moves at once.
        for _ in 0..100 {
            press(&mut app, KeyCode::Char('k')).await;
        }
        let out = draw(&mut app, w, h);
        assert!(out.contains("row 01"), "{w}x{h}: at the top:\n{out}");
        press(&mut app, KeyCode::Char('j')).await;
        let out = draw(&mut app, w, h);
        assert!(!out.contains("row 01"), "{w}x{h}: one j moves down:\n{out}");

        press(&mut app, KeyCode::Esc).await;
        assert_ne!(
            app.tui_mode,
            TuiMode::Session {
                id: b,
                view: SessionView::Output
            },
            "{w}x{h}: Esc returns"
        );
    }
}

/// #1292: the Agent graph half of this guard moved to the Files lens of
/// Dashboard Deps (`screens/dashboard/deps_tests.rs`,
/// `deps_files_lens_keys_do_not_fall_through`).
#[tokio::test]
async fn fullscreen_and_graph_keys_do_not_fall_through() {
    let (mut app, b) = app_on_202();
    let mode = TuiMode::Session {
        id: b,
        view: SessionView::Output,
    };
    app.tui_mode = mode;
    let before = app.pool.all_sessions().len();
    // Enter is an Overview key too: it proves the fall-through is closed
    // even where `p` / `r` have no visible effect in a test.
    // #1288: on Output `d` is the strip's Detail key, checked below.
    for code in ['p', 'k', 'r', 'D']
        .iter()
        .map(|&c| KeyCode::Char(c))
        .chain([KeyCode::Enter])
    {
        press(&mut app, code).await;
        assert_eq!(app.tui_mode, mode, "{code:?} left {mode:?}");
        let out = draw(&mut app, 80, 24);
        assert!(!out.contains("Type 101"), "{code:?}: kill dialog:\n{out}");
    }
    assert_eq!(
        app.pool.all_sessions().len(),
        before,
        "{mode:?}: D dismissed"
    );
    assert!(!app.show_activity_log, "{mode:?}: d toggled the log");
    press(&mut app, KeyCode::Char('d')).await;
    assert_eq!(
        app.tui_mode,
        TuiMode::Session {
            id: b,
            view: SessionView::Detail
        },
        "d opens Detail"
    );
    assert!(!app.show_activity_log, "d toggled the log");
    let actions = actions_on(registry(), mode);
    assert!(
        actions.iter().all(|b| b.action != Action::Kill),
        "{mode:?}: the palette offers kill"
    );
}

/// The whole Fullscreen screen at 80x24: the in-screen footer and the
/// bottom hint row list only keys Fullscreen handles (`j k ↑ ↓ Esc`).
#[tokio::test]
async fn fullscreen_screen_footer() {
    let (mut app, b) = app_on_202();
    app.tui_mode = TuiMode::Session {
        id: b,
        view: SessionView::Output,
    };
    let out = draw(&mut app, 80, 24);
    for hint in ["Esc Back", "j/k Scroll", "[Esc] back"] {
        assert!(out.contains(hint), "{hint:?} missing:\n{out}");
    }
    assert!(!out.contains("Kill"), "Fullscreen has no kill key:\n{out}");
    insta::assert_snapshot!(out);
}
