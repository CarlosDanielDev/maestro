//! Keys of the run loop. `QueueExecution` and `ContinuousPause` take every
//! key here (`input_handler::handle_mode_keys`).
//!
//! Queue: `h` hold ↔ resume, `S` stop (typed confirm: drain or kill), and
//! on a failed item the decision card (`↑↓` pick, `⏎` confirm, `v` log,
//! `Esc` later). Continuous: the decision card. `Esc` goes back; the run
//! keeps going.

use crossterm::event::{KeyCode, KeyEvent};

use super::{Phase, RunLoopState, RunView, StopDialog};
use crate::tui::activity_log::LogLevel;
use crate::tui::app::{App, SessionView, TuiMode};
use crate::tui::ds::confirm::ConfirmOutcome;
use crate::tui::ds::toast::{Severity, Toast};
use crate::tui::widgets::decision_card::DecisionAction;
use crate::work::executor::{ExecutorPhase, FailureAction};

/// Handle `key` on the run loop. Always consumed.
pub async fn handle(app: &mut App, key: &KeyEvent) {
    if app.screen_state.run_loop.stop.is_some() {
        return handle_stop(app, key).await;
    }
    let Some((issue, mut card)) = RunView::from_app(app).and_then(|v| match (v.phase, v.card) {
        (Phase::Waiting(n), Some(card)) => Some((n, card)),
        _ => None,
    }) else {
        return handle_run_keys(app, key);
    };
    if matches!(key.code, KeyCode::Char('r' | 's' | 'a')) {
        let toast = Toast::new(Severity::Info, "pick with ↑↓, ⏎ confirms");
        app.push_toast(toast, std::time::Instant::now());
        return;
    }
    match card.handle_key(key) {
        DecisionAction::Pick(i) => app.screen_state.run_loop.choice = Some((issue, i)),
        DecisionAction::Confirm(i) => {
            app.screen_state.run_loop.choice = None;
            match app.tui_mode {
                TuiMode::ContinuousPause => decide_continuous(app, i == 1),
                _ => decide_queue(app, i == 1),
            }
        }
        DecisionAction::Evidence => {
            let id = app
                .queue_executor
                .as_ref()
                .and_then(|e| e.deciding_session_id());
            if let Some(id) = id {
                app.navigate_to(TuiMode::Session {
                    id,
                    view: SessionView::Detail,
                });
            }
        }
        // Later never keeps a paid pick: the card reopens on the default.
        DecisionAction::Later => {
            app.screen_state.run_loop.choice = None;
            app.navigate_back_or_dashboard();
        }
        DecisionAction::None => handle_run_keys(app, key),
    }
}

/// Keys outside the card: `h`, `S` and `Esc` (queue only). A continuous
/// pause waits on its card: `Esc` does nothing there.
fn handle_run_keys(app: &mut App, key: &KeyEvent) {
    if app.tui_mode != TuiMode::QueueExecution {
        return;
    }
    let open = (app.queue_executor.as_ref()).is_some_and(|e| !e.is_finished() && !e.is_stopped());
    match key.code {
        KeyCode::Esc => app.navigate_back_or_dashboard(),
        KeyCode::Char('h') if open => toggle_hold(app),
        KeyCode::Char('S') if open => open_stop(app),
        _ => {}
    }
}

/// `h`: hold the queue (nothing new starts), or resume it and start the
/// next item if none runs.
fn toggle_hold(app: &mut App) {
    let Some(exec) = app.queue_executor.as_mut() else {
        return;
    };
    let held = !exec.is_held();
    exec.set_held(held);
    let idle = exec.phase() == ExecutorPhase::Idle;
    let message = if held {
        "Queue held: nothing new starts, running sessions finish"
    } else {
        "Queue resumed"
    };
    app.activity_log
        .push_simple("Queue".into(), message.into(), LogLevel::Info);
    if !held && idle {
        app.advance_queue_and_launch();
    }
}

fn open_stop(app: &mut App) {
    let Some(view) = RunView::from_app(app) else {
        return;
    };
    let running_spent: f64 = (app.queue_executor.as_ref())
        .and_then(|e| e.running_session_id())
        .and_then(|id| app.pool.get_session(id))
        .map_or(0.0, |s| s.cost_usd);
    let most = (view.cap > 0.0).then(|| (view.running as f64 * view.cap - running_spent).max(0.0));
    let running_id = app
        .queue_executor
        .as_ref()
        .and_then(|e| e.running_session_id());
    let starting = (app.queue_executor.as_ref()).is_some_and(|e| e.running_issue().is_some())
        && running_id.is_none();
    // Counted once, here: git never runs per frame.
    let kill_rows = running_id.map_or_else(Vec::new, |id| {
        app.kill_teardown_rows(id, app.uncommitted_files(id), app.unpushed_commits(id))
    });
    let dialog = StopDialog::new(view.running, starting, view.not_started, most);
    app.screen_state.run_loop.stop = Some(dialog.with_kill_rows(kill_rows));
}

/// The open "Stop the run?" dialog: `↑↓` switches drain ↔ kill, typed
/// text goes to the field, `⏎` confirms once it matches, `Esc` cancels.
async fn handle_stop(app: &mut App, key: &KeyEvent) {
    let state: &mut RunLoopState = &mut app.screen_state.run_loop;
    let Some(dialog) = state.stop.as_mut() else {
        return;
    };
    if matches!(key.code, KeyCode::Up | KeyCode::Down) {
        dialog.toggle();
        return;
    }
    match dialog.confirm.handle_key(*key) {
        ConfirmOutcome::Pending => {}
        ConfirmOutcome::Cancelled => state.stop = None,
        ConfirmOutcome::Confirmed => {
            let kill = dialog.kill;
            state.stop = None;
            stop_run(app, kill).await;
        }
    }
}

/// Stop the run: nothing new starts. `kill` also kills the running session
/// the way `X` does; otherwise it finishes (drain).
async fn stop_run(app: &mut App, kill: bool) {
    let Some(exec) = app.queue_executor.as_mut() else {
        return;
    };
    let running = exec.running_session_id();
    let starting = exec.running_issue().filter(|_| running.is_none());
    exec.stop();
    // A paid pick never outlives the run it was made on.
    app.screen_state.run_loop.choice = None;
    if kill {
        // No session yet: it is dropped when it arrives (`add_session`).
        app.screen_state.run_loop.kill_unstarted = starting;
    }
    let finished = exec.is_finished();
    let message = if kill {
        "Run stopped: running session killed, nothing new starts"
    } else {
        "Run stopped: the running session finishes, nothing new starts"
    };
    app.activity_log
        .push_simple("Queue".into(), message.into(), LogLevel::Warn);
    if kill && let Some(id) = running {
        app.kill_selected_session(id).await;
    }
    if finished {
        app.open_completion_summary();
    }
}

/// The queue card: Skip (0, `$0.00`) or Retry (1, a paid run).
fn decide_queue(app: &mut App, retry: bool) {
    let Some(exec) = app.queue_executor.as_mut() else {
        return;
    };
    let action = if retry {
        FailureAction::Retry
    } else {
        FailureAction::Skip
    };
    exec.apply_decision(action);
    if exec.is_finished() {
        app.open_completion_summary();
    } else {
        app.advance_queue_and_launch();
    }
}

/// The continuous card: skip and go on (0) or retry (1).
fn decide_continuous(app: &mut App, retry: bool) {
    let Some(cont) = app.continuous_mode.as_mut() else {
        return;
    };
    if retry {
        if let Some(issue_number) = cont.on_retry() {
            if let Some(ref mut service) = app.work_assignment_service {
                service.inner_mut().mark_pending_undo_cascade(issue_number);
            }
            app.activity_log.push_simple(
                "CONTINUOUS".into(),
                format!("Retrying #{issue_number}..."),
                LogLevel::Info,
            );
        }
    } else {
        let skipped = cont.current_failure().map(|f| f.issue_number);
        cont.on_skip();
        if let Some(num) = skipped {
            app.activity_log.push_simple(
                "CONTINUOUS".into(),
                format!("Skipped #{num}, advancing..."),
                LogLevel::Warn,
            );
        }
    }
    app.tui_mode = TuiMode::Dashboard;
}
