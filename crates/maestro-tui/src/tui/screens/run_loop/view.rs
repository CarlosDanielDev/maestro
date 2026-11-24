//! [`RunView`]: what the run loop draws, read from the app every frame.

use super::RunLoopState;
use crate::session::types::SessionStatus;
use crate::tui::app::{App, TuiMode};
use crate::tui::ds::money::money;
use crate::tui::ds::status_chip::status_word;
use crate::tui::widgets::decision_card::{Cost, DecisionCard, DecisionOption, Section};
use crate::work::executor::{ExecutorPhase, QueueExecutor, QueueItemState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavour {
    /// A queue of issues launched together (`QueueExecution`).
    Queue,
    /// `--continuous`: one issue at a time (`ContinuousPause`).
    Continuous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Live,
    /// `h`: nothing new starts, the running session finishes.
    Held,
    /// `S` drain: the running session finishes, then the run ends.
    Stopping,
    /// The issue the loop waits on you for.
    Waiting(u64),
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    /// A session runs it (its STATUS is drawn).
    Session(SessionStatus),
    /// Not started; it starts when a slot is free.
    Next,
    /// Not started and the run will not start it (stopped).
    NotRun,
    Done,
    Failed,
    Skipped,
    /// Waits on your decision.
    NeedsYou,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub issue: u64,
    pub title: String,
    pub state: RowState,
    /// Right-hand note: the session's cost once it exists.
    pub note: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunView {
    pub flavour: Flavour,
    pub phase: Phase,
    pub rows: Vec<Row>,
    pub done: usize,
    pub running: usize,
    pub not_started: usize,
    pub spent: f64,
    /// Per-session cap (`[budget] per_session_usd`); 0 = no cap.
    pub cap: f64,
    /// Finished rows at their real cost, the rest at the cap.
    pub worst: f64,
    /// The worst case approved at launch: every row at the cap.
    pub approved: f64,
    pub card: Option<DecisionCard>,
    /// Continuous only: `n done · n skipped · n failed`.
    pub tally: Option<String>,
}

impl RunView {
    /// The run loop of the run the mode shows: `ContinuousPause` reads the
    /// continuous state only, any other mode the queue executor only (a
    /// finished queue stays in the app after its run). `None`: no such run.
    pub fn from_app(app: &App) -> Option<Self> {
        let budget = crate::tui::budget_prespawn::launch_budget(app);
        let retry = Cost::retry(&budget);
        let state = &app.screen_state.run_loop;
        if app.tui_mode != TuiMode::ContinuousPause {
            let exec = app.queue_executor.as_ref()?;
            return Some(Self::queue(app, exec, budget.per_session_usd, retry, state));
        }
        let cont = app.continuous_mode.as_ref()?;
        let failure = cont.current_failure();
        let rows: Vec<Row> = failure
            .map(|f| Row {
                issue: f.issue_number,
                title: f.issue_title.clone(),
                state: RowState::NeedsYou,
                note: String::new(),
            })
            .into_iter()
            .collect();
        let tally = format!(
            "{} done · {} skipped · {} failed",
            cont.completed_count,
            cont.skipped_count,
            cont.failures.len()
        );
        let card = failure.map(|f| DecisionCard {
            title: format!("#{} failed · {}", f.issue_number, f.issue_title),
            tag: "needs you".into(),
            sections: vec![Section {
                title: "WHAT HAPPENED".into(),
                rows: vec![
                    ("error".into(), f.error_summary.clone()),
                    ("so far".into(), tally.clone()),
                ],
            }],
            options: vec![
                DecisionOption {
                    label: format!("Skip #{} and go on", f.issue_number),
                    note: "it stays open; the next ready issue starts".into(),
                    cost: Cost::Free,
                },
                DecisionOption {
                    label: format!("Retry #{}", f.issue_number),
                    note: "a new session: a working run is a paid run".into(),
                    cost: retry,
                },
            ],
            selected: state.choice_for(f.issue_number).min(1),
            // Continuous mode waits on this card alone: no "later".
            later: false,
            // The failure record has no session id to show.
            evidence: false,
        });
        Some(Self {
            flavour: Flavour::Continuous,
            phase: failure.map_or(Phase::Live, |f| Phase::Waiting(f.issue_number)),
            done: cont.completed_count,
            running: usize::from(failure.is_none() && cont.current_issue.is_some()),
            not_started: 0,
            spent: app.total_cost,
            cap: budget.per_session_usd,
            worst: 0.0,
            approved: 0.0,
            rows,
            card,
            tally: Some(tally),
        })
    }

    fn queue(app: &App, exec: &QueueExecutor, cap: f64, retry: Cost, state: &RunLoopState) -> Self {
        let waiting = match exec.phase() {
            ExecutorPhase::AwaitingDecision { failed_index } => Some(failed_index),
            _ => None,
        };
        let finished = exec.is_finished();
        let title = |n: u64| {
            (app.queue_launch_configs.iter().flatten())
                .find(|c| c.issue_number == Some(n))
                .map_or_else(|| format!("Issue #{n}"), |c| c.title.clone())
        };
        let mut spent = 0.0;
        let mut worst = 0.0;
        let (mut done, mut running, mut not_started) = (0, 0, 0);
        let mut rows = Vec::new();
        for (i, item) in exec.items().iter().enumerate() {
            let session = item.session_id.and_then(|id| app.pool.get_session(id));
            let cost = session.map_or(0.0, |s| s.cost_usd);
            spent += cost;
            let state = match item.state {
                QueueItemState::Pending if finished || exec.is_stopped() => RowState::NotRun,
                QueueItemState::Pending => RowState::Next,
                QueueItemState::Running => match session {
                    Some(s) => RowState::Session(s.status),
                    None => RowState::Session(SessionStatus::Spawning),
                },
                QueueItemState::Succeeded => RowState::Done,
                QueueItemState::Failed if waiting == Some(i) => RowState::NeedsYou,
                QueueItemState::Failed => RowState::Failed,
                QueueItemState::Skipped => RowState::Skipped,
            };
            match state {
                RowState::Next => not_started += 1,
                RowState::Session(_) => running += 1,
                RowState::Done => done += 1,
                _ => {}
            }
            let live = matches!(state, RowState::Next | RowState::Session(_));
            worst += if live { cap.max(cost) } else { cost };
            rows.push(Row {
                issue: item.queued.issue_number,
                title: title(item.queued.issue_number),
                state,
                note: session.map_or(String::new(), |_| money(Some(cost))),
            });
        }
        let approved = cap * exec.total_count() as f64;
        let card = waiting.map(|i| {
            let item = &exec.items()[i];
            let n = item.queued.issue_number;
            let session = item.session_id.and_then(|id| app.pool.get_session(id));
            let what = match session {
                Some(s) if s.is_hollow_completion => "hollow: finished without doing any work",
                Some(s) => status_word(s.status),
                None => "no session started (the issue fetch failed)",
            };
            let after = worst + cap;
            let effect = if cap > 0.0 {
                format!(
                    " · worst case {} → {}",
                    money(Some(worst)),
                    money(Some(after))
                )
            } else {
                String::new()
            };
            DecisionCard {
                title: format!("#{n} failed · {}", title(n)),
                tag: "needs you".into(),
                sections: vec![Section {
                    title: "WHAT HAPPENED".into(),
                    rows: vec![
                        ("session".into(), what.to_string()),
                        (
                            "cost".into(),
                            money(Some(session.map_or(0.0, |s| s.cost_usd))),
                        ),
                    ],
                }],
                options: vec![
                    DecisionOption {
                        label: format!("Skip #{n}"),
                        note: "it stays open, nothing is marked done; the queue goes on".into(),
                        cost: Cost::Free,
                    },
                    DecisionOption {
                        label: format!("Retry #{n}"),
                        note: format!("a working run is a paid run{effect}"),
                        cost: retry,
                    },
                ],
                selected: state.choice_for(n).min(1),
                later: true,
                evidence: session.is_some(),
            }
        });
        let phase = match (waiting, finished, exec.is_held()) {
            (Some(i), _, _) => Phase::Waiting(exec.items()[i].queued.issue_number),
            (_, true, _) => Phase::Finished,
            _ if exec.is_stopped() => Phase::Stopping,
            (_, _, true) => Phase::Held,
            _ => Phase::Live,
        };
        Self {
            flavour: Flavour::Queue,
            phase,
            rows,
            done,
            running,
            not_started,
            spent,
            cap,
            worst,
            approved,
            card,
            tally: None,
        }
    }
}
