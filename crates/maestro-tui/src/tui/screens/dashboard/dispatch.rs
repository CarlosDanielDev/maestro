//! Dashboard keys (#1280). Taken before the old launcher letters
//! (`HomeScreen`), which still answer every key this module leaves.
//! `j k ↑ ↓` move, `⏎` opens a session or arms a pending PR, `X` kills
//! through the typed confirm, `l` opens the log, `D` dismisses finished
//! sessions, `o c t` switch the Ops, Cost and Tokens views (#1289), `g`
//! opens Deps (#1292, its keys are `deps::handle_key`). Any
//! key but `⏎` disarms an armed PR, also keys that never
//! reach this module (`note_key`); `Esc` then only disarms.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::rows::WorkKey;
use super::usage::{Usage, View};
use super::{Dashboard, DashboardView, pr_lock_reason};
use crate::tui::app::{App, TuiMode};
use crate::tui::ds::armed::{ArmedState, EnterOutcome};
use crate::tui::screens::ScreenAction;

/// The data the Dashboard reads and its own state, borrowed together.
pub fn split(app: &mut App) -> (DashboardView<'_>, &mut Dashboard) {
    let usage = usage(app);
    let files_enabled = app.is_agent_graph_enabled();
    let files = if files_enabled && app.screen_state.dashboard.view == View::Deps {
        let sessions = app.pool.all_sessions();
        crate::tui::agent_graph::model::build_graph(&sessions, &app.pool.file_claims)
    } else {
        Default::default()
    };
    let view = DashboardView {
        sessions: app.pool.all_sessions(),
        pending: &app.pending_prs,
        ci: &app.ci_poller.ci_check_details,
        gh_auth_ok: app.gh_auth_ok,
        now: app
            .screen_state
            .dashboard
            .wall
            .unwrap_or_else(chrono::Utc::now),
        usage,
        files,
        files_enabled,
        assigner: app.work_assignment_service.as_ref().map(|s| s.inner()),
    };
    (view, &mut app.screen_state.dashboard)
}

/// The numbers the Cost and Tokens views read (#1289), from state and
/// config only: nothing here is made up.
fn usage(app: &App) -> Usage {
    Usage {
        total_cost: app.total_cost,
        budget: app.config.as_ref().map(|c| c.budget.clone()),
        since: app.start_time,
        tq_enabled: app.flags.is_enabled(crate::flags::Flag::TurboQuant),
        tq_bit_width: app.config.as_ref().map_or(4, |c| c.turboquant.bit_width),
        badges: app.quota_badges(),
        quota: app.minimax_quota.clone(),
    }
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
    // #1289: Cost and Tokens list no PR rows, so their selection never
    // rests on one (a hidden armed ⏎ ⏎ would write to GitHub).
    if dash.view != View::Ops {
        dash.list.keep_on_session();
    }
    mirror_selection(app);
}

/// The Overview-era keys that still run from Detail (`X` / F10 kill, `f` /
/// F3 full, `c` copy) read `panel_view.selected`: keep it on the marked
/// session, or on no session for a PR row (`usize::MAX` selects nothing).
fn mirror_selection(app: &mut App) {
    let marked = app.screen_state.dashboard.list.selected_session();
    let index = marked.and_then(|id| app.pool.all_sessions().iter().position(|s| s.id == id));
    app.panel_view.selected = Some(index.unwrap_or(usize::MAX));
}

/// Called for every key before any handler: a key that will not reach the
/// Dashboard's own `⏎` / `Esc` disarms an armed PR row (a tab switch, an
/// overlay, a chord, a modal).
pub fn note_key(app: &mut App, key: &KeyEvent) {
    if key.kind != KeyEventKind::Press {
        return;
    }
    let plain = (key.modifiers - KeyModifiers::SHIFT).is_empty();
    let own = app.tui_mode == TuiMode::Dashboard
        && plain
        && matches!(key.code, KeyCode::Enter | KeyCode::Esc)
        && app.help_state.is_none()
        && app.palette.is_none()
        && !crate::tui::ui::log_drawer_shown(app);
    if !own {
        app.screen_state.dashboard.disarm();
    }
}

/// Select the first pending PR row (the ⌃K `retry PR` entry, #1280).
pub fn select_first_pending_pr(app: &mut App) {
    sync(app);
    let list = &mut app.screen_state.dashboard.list;
    if let Some(key) = list
        .rows
        .iter()
        .copied()
        .find(|k| matches!(k, WorkKey::PendingPr(_)))
    {
        list.select(key);
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
    // #1292: Deps takes every key it gets.
    if app.screen_state.dashboard.view == View::Deps {
        app.screen_state.dashboard.disarm();
        let graph = split(app).0.files;
        let action = super::deps::handle_key(app, key, &graph);
        mirror_selection(app);
        return Some(action);
    }
    if key.code == KeyCode::Enter {
        return Some(enter(app));
    }
    let dash = &mut app.screen_state.dashboard;
    let was_armed = matches!(dash.armed.state(), ArmedState::Armed(_));
    dash.disarm();
    let action = handle_other(app, key, was_armed);
    mirror_selection(app);
    action
}

fn handle_other(app: &mut App, key: &KeyEvent, was_armed: bool) -> Option<ScreenAction> {
    let usage = app.screen_state.dashboard.view != View::Ops;
    let list = &mut app.screen_state.dashboard.list;
    match key.code {
        KeyCode::Char('j') | KeyCode::Down if usage => list.move_session(true),
        KeyCode::Char('k') | KeyCode::Up if usage => list.move_session(false),
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
        KeyCode::Char('D') => app.dismiss_all_completed(),
        KeyCode::Esc if was_armed => {}
        // #1289: the views share the selection; Esc leaves a Usage view
        // for Ops before it leaves the Dashboard.
        KeyCode::Char('o') => app.screen_state.dashboard.view = View::Ops,
        KeyCode::Char('c') => show_usage(app, View::Cost),
        KeyCode::Char('t') => show_usage(app, View::Tokens),
        KeyCode::Char('g') => super::deps::press_g(app),
        KeyCode::Esc if app.screen_state.dashboard.view != View::Ops => {
            app.screen_state.dashboard.view = View::Ops;
        }
        _ => return None,
    }
    Some(ScreenAction::None)
}

/// The footer in Cost and Tokens: `⏎` only opens a session there (no PR
/// row to arm), so its hint drops `arm PR` (#1289; `ui::draw_footer`).
/// On Deps (#1292) the footer lists only the keys of the drawn lens.
pub fn usage_footer(app: &App, hints: &mut Vec<crate::tui::navigation::keymap::InlineHint>) {
    let dash = &app.screen_state.dashboard;
    if app.tui_mode != TuiMode::Dashboard || dash.view == View::Ops {
        return;
    }
    if dash.view == View::Deps {
        return super::deps::footer(app, hints);
    }
    for h in hints.iter_mut().filter(|h| h.key == "Enter") {
        h.action = "open";
    }
}

/// Cost or Tokens, with the selection moved off a PR row (#1289).
pub(super) fn show_usage(app: &mut App, view: View) {
    let dash = &mut app.screen_state.dashboard;
    dash.view = view;
    dash.list.keep_on_session();
}

/// `⏎`: a session opens its Detail, an open PR the PR list, a pending PR
/// arms and then (second `⏎` within `ARM_WINDOW`) queues the create. In
/// Cost and Tokens `sync` (run first) keeps the selection off PR rows.
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
