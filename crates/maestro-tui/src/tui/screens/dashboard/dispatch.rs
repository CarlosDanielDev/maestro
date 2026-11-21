//! Dashboard keys (#1280). Taken before the old launcher letters
//! (`HomeScreen`), which still answer every key this module leaves.
//! `j k ↑ ↓` move, `⏎` opens a session or arms a pending PR, `X` kills
//! through the typed confirm, `l` opens the log. Any key but `⏎`
//! disarms an armed PR; `Esc` then only disarms.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::rows::WorkKey;
use super::{Dashboard, DashboardView, pr_lock_reason};
use crate::tui::app::{App, TuiMode};
use crate::tui::ds::armed::{ArmedState, EnterOutcome};
use crate::tui::screens::ScreenAction;

/// The data the Dashboard reads and its own state, borrowed together.
pub fn split(app: &mut App) -> (DashboardView<'_>, &mut Dashboard) {
    let view = DashboardView {
        sessions: app.pool.all_sessions(),
        pending: &app.pending_prs,
        ci: &app.ci_poller.ci_check_details,
        gh_auth_ok: app.gh_auth_ok,
        now: chrono::Utc::now(),
    };
    (view, &mut app.screen_state.dashboard)
}

/// Rebuild the rows. A Home `⏎` jump (`PanelView::reveal`) selects its
/// session here, as it did on the Overview grid.
pub fn sync(app: &mut App) {
    let reveal = app
        .panel_view
        .pending_reveal
        .take()
        .and_then(|i| app.pool.session_id_at_index(i));
    let (view, dash) = split(app);
    dash.rebuild(&view);
    if let Some(id) = reveal {
        dash.list.select(WorkKey::Session(id));
    }
}

/// `Some` when the Dashboard took the key; `None` lets the launcher
/// letters have it.
pub fn dispatch_input(app: &mut App, event: &Event) -> Option<ScreenAction> {
    let Event::Key(key) = event else {
        return None;
    };
    if key.kind != KeyEventKind::Press || !(key.modifiers - KeyModifiers::SHIFT).is_empty() {
        return None;
    }
    sync(app);
    if key.code == KeyCode::Enter {
        return Some(enter(app));
    }
    let dash = &mut app.screen_state.dashboard;
    let was_armed = matches!(dash.armed.state(), ArmedState::Armed(_));
    dash.disarm();
    handle_other(app, key, was_armed)
}

fn handle_other(app: &mut App, key: &KeyEvent, was_armed: bool) -> Option<ScreenAction> {
    let list = &mut app.screen_state.dashboard.list;
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => list.move_down(),
        KeyCode::Char('k') | KeyCode::Up => list.move_up(),
        KeyCode::Char('X') => {
            let live = list.selected_session().filter(|id| {
                app.pool
                    .get_session(*id)
                    .is_some_and(|s| !s.status.is_terminal())
            });
            if let Some(id) = live {
                app.open_kill_confirm(id);
            }
        }
        KeyCode::Char('l') => {
            if let Some(id) = list.selected_session() {
                app.log_viewer_scroll = 0;
                app.navigate_to(TuiMode::LogViewer(id));
            }
        }
        KeyCode::Esc if was_armed => {}
        _ => return None,
    }
    Some(ScreenAction::None)
}

/// `⏎`: a session opens its Detail, an open PR the PR list, a pending PR
/// arms and then (second `⏎` within `ARM_WINDOW`) queues the create.
fn enter(app: &mut App) -> ScreenAction {
    let selected = app.screen_state.dashboard.list.selected;
    match selected {
        Some(WorkKey::Session(id)) => app.navigate_to(TuiMode::Detail(id)),
        Some(WorkKey::OpenPr(_)) => return ScreenAction::Push(TuiMode::PrReview),
        Some(WorkKey::PendingPr(issue)) if arm_or_commit(app, issue) => {
            app.trigger_manual_pr_retry(issue);
        }
        _ => {}
    }
    ScreenAction::None
}

/// True when this `⏎` commits the write. A locked row never arms.
fn arm_or_commit(app: &mut App, issue: u64) -> bool {
    let locked = app
        .pending_prs
        .iter()
        .find(|p| p.issue_number == issue)
        .is_none_or(|p| pr_lock_reason(p).is_some());
    let dash = &mut app.screen_state.dashboard;
    if locked {
        dash.disarm();
        return false;
    }
    if dash.armed_for != Some(issue) {
        dash.disarm();
        dash.armed_for = Some(issue);
    }
    let now = dash.now();
    match dash.armed.on_enter(now) {
        EnterOutcome::Commit => {
            dash.disarm();
            true
        }
        EnterOutcome::Armed | EnterOutcome::Ignored => false,
    }
}
