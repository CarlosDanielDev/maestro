//! #1289 judge round: the Cost and Tokens views list no PR rows, so their
//! keys never act on one (an armed ⏎ ⏎ there would write to GitHub with no
//! armed bar on screen). And where the Dashboard lands when you come back.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::usage::{draw, empty_app, press, text, usage_app, view};
use crate::provider::github::types::awaiting_pending_pr;
use crate::tui::app::{App, TuiMode};
use crate::tui::input_handler::handle_key;
use crate::tui::screens::dashboard::rows::WorkKey;
use crate::tui::screens::dashboard::usage::View;

async fn key(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE)).await;
}

fn retries(app: &App) -> u32 {
    app.pending_prs[0].manual_retry_count
}

#[tokio::test]
async fn usage_views_never_arm_a_pr_row() {
    let mut app = usage_app();
    app.pending_prs.push(awaiting_pending_pr(545));
    draw(&mut app, 120, 36);
    // In Ops, walk down to the PR row (Sessions come first).
    for _ in 0..3 {
        press(&mut app, 'j').await;
    }
    assert_eq!(
        app.screen_state.dashboard.list.selected,
        Some(WorkKey::PendingPr(545))
    );
    for v in ['c', 't'] {
        press(&mut app, v).await;
        let sel = app.screen_state.dashboard.list.selected;
        assert!(matches!(sel, Some(WorkKey::Session(_))), "{v}: {sel:?}");
        // `j` past the last live session skips the hidden PR row.
        for _ in 0..6 {
            press(&mut app, 'j').await;
            let sel = app.screen_state.dashboard.list.selected;
            assert!(matches!(sel, Some(WorkKey::Session(_))), "{v} j: {sel:?}");
        }
        draw(&mut app, 120, 36);
        key(&mut app, KeyCode::Enter).await;
        key(&mut app, KeyCode::Enter).await;
        assert_eq!(retries(&app), 0, "{v}: ⏎ ⏎ queued a PR create");
        app.screen_state.dashboard.view = View::Ops;
        app.tui_mode = TuiMode::Dashboard;
        draw(&mut app, 120, 36);
        app.screen_state
            .dashboard
            .list
            .select(WorkKey::PendingPr(545));
    }
}

#[tokio::test]
async fn usage_view_with_only_a_pr_row_selects_nothing() {
    let mut app = empty_app();
    app.gh_auth_ok = true;
    app.pending_prs.push(awaiting_pending_pr(545));
    draw(&mut app, 120, 36);
    assert_eq!(
        app.screen_state.dashboard.list.selected,
        Some(WorkKey::PendingPr(545))
    );
    press(&mut app, 'c').await;
    let out = text(&draw(&mut app, 120, 36));
    assert_eq!(app.screen_state.dashboard.list.selected, None);
    assert!(out.contains("Nothing has run yet"), "{out}");
    key(&mut app, KeyCode::Enter).await;
    key(&mut app, KeyCode::Enter).await;
    assert_eq!(retries(&app), 0, "⏎ ⏎ queued a PR create from Cost");
    assert_eq!((app.tui_mode, view(&app)), (TuiMode::Dashboard, View::Cost));
}

/// Going to the Dashboard (tab `2`, ⌃K) opens Ops; going back (`Esc` from
/// a screen opened in a Usage view) returns to that view.
#[tokio::test]
async fn usage_view_on_return() {
    let mut app = usage_app();
    draw(&mut app, 120, 36);
    press(&mut app, 'c').await;
    app.navigate_to(TuiMode::IssueBrowser);
    app.navigate_to(TuiMode::Dashboard);
    assert_eq!((app.tui_mode, view(&app)), (TuiMode::Dashboard, View::Ops));

    press(&mut app, 't').await;
    draw(&mut app, 120, 36);
    key(&mut app, KeyCode::Enter).await;
    assert!(
        matches!(
            app.tui_mode,
            TuiMode::Session {
                view: crate::tui::app::SessionView::Detail,
                ..
            }
        ),
        "{:?}",
        app.tui_mode
    );
    key(&mut app, KeyCode::Esc).await;
    assert_eq!(
        (app.tui_mode, view(&app)),
        (TuiMode::Dashboard, View::Tokens)
    );
}

/// The TurboQuant tile uses the panel's word: `would save` while off.
#[tokio::test]
async fn turboquant_tile_and_panel_share_the_off_word() {
    let mut app = usage_app();
    assert!(!app.flags.is_enabled(crate::flags::Flag::TurboQuant));
    press(&mut app, 't').await;
    let out = text(&draw(&mut app, 160, 40));
    assert!(out.contains("TurboQuant · off"), "{out}");
    assert!(!out.contains("a ceiling"), "{out}");
    let tile = out
        .lines()
        .find(|l| l.contains("measured · est."))
        .unwrap_or_default();
    assert!(tile.contains("would save"), "{out}");
}

/// The average is over the sessions listed, numerator and count alike:
/// `D` dismisses sessions but leaves their spend in the run total.
#[tokio::test]
async fn usage_cost_average_counts_listed_sessions_only() {
    let mut app = empty_app();
    let k = || super::usage::tokens(1_000, 1_000, 0, 0);
    for issue in [1, 2] {
        let s = super::usage::session(
            issue,
            crate::session::types::SessionStatus::Running,
            "claude-opus-4-5",
            2.0,
            k(),
        );
        app.pool.enqueue(s);
    }
    app.total_cost = 6.0; // a third $2.00 session was dismissed
    press(&mut app, 'c').await;
    let out = text(&draw(&mut app, 160, 40));
    assert!(out.contains("avg $2.00"), "{out}");
}

/// No hint for a key that does nothing: `⏎` cannot arm a PR row in the
/// Usage views, so their footer says `open` only.
#[tokio::test]
async fn usage_footer_enter_says_open_only() {
    let mut app = usage_app();
    let ops = text(&draw(&mut app, 120, 36));
    assert!(ops.contains("Enter open · arm PR"), "{ops}");
    for v in ['c', 't'] {
        press(&mut app, v).await;
        let out = text(&draw(&mut app, 120, 36));
        let footer = out.lines().last().unwrap_or_default();
        assert!(footer.contains("Enter open"), "{v}: {footer}");
        assert!(!out.contains("arm PR"), "{v}: {footer}");
    }
}
