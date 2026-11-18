//! TUI ↔ session::team_runner bridge (#881).
//!
//! `RealTeamLauncher` is the production `TeamLauncher` impl used by the
//! `TuiCommand::RunTeam` arm. It owns a copy of the data-event sender
//! and the active `ProviderConfig` so each per-issue spawn can issue
//! the same `gh issue view`-backed fetch path that `LaunchSession`
//! uses today — promotion into the `SessionPool` happens via the
//! existing `TuiDataEvent::Issue` handler.
//!
//! Living in `src/tui/` keeps `src/session/team_runner.rs` free of TUI
//! types so its tests can stay pure async + trait-based.
//!
//! #1246: the event loop publishes a `SessionMark` per pool session on a
//! `watch` channel after each completion pass; `wait_until_terminal` reads
//! it so the runner holds the next wave until this one has finished.

use crate::config::ProviderConfig;
use crate::provider::create_provider;
use crate::provider::github::client::RepoProvider;
use crate::session::team_runner::{TeamLauncher, team_verdict, wait_for_verdict};
use crate::session::types::{Session, SessionStatus};
use crate::state::types::IssueNumber;
use crate::tui::app;
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;
use uuid::Uuid;

/// What the team runner needs to know about one pool session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SessionMark {
    pub id: Uuid,
    pub issue: Option<IssueNumber>,
    pub status: SessionStatus,
    pub hollow: bool,
}

/// Publish the pool's sessions to the team runners. Does nothing when no
/// team run is listening, and only wakes them when a mark changed.
pub(super) fn publish_sessions(tx: &watch::Sender<Vec<SessionMark>>, sessions: &[&Session]) {
    if tx.receiver_count() == 0 {
        return;
    }
    let marks: Vec<SessionMark> = sessions
        .iter()
        .map(|s| SessionMark {
            id: s.id,
            issue: s.issue_number,
            status: s.status,
            hollow: s.is_hollow_completion,
        })
        .collect();
    tx.send_if_modified(|current| {
        if *current == marks {
            return false;
        }
        *current = marks;
        true
    });
}

/// Verdict for `issue` over the sessions that appeared after its spawn
/// (`prior` holds the ones that already existed). A retry is a new session
/// for the same issue, so wait while any of them is live, then pass if any
/// passed. `None` until the spawned session shows up.
fn issue_verdict(
    marks: &[SessionMark],
    issue: IssueNumber,
    prior: &HashSet<Uuid>,
) -> Option<Result<(), String>> {
    let mut first_err = None;
    let mut passed = false;
    let mut seen = false;
    for mark in marks
        .iter()
        .filter(|m| m.issue == Some(issue) && !prior.contains(&m.id))
    {
        seen = true;
        match team_verdict(mark.status, mark.hollow)? {
            Ok(()) => passed = true,
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    if !seen {
        return None;
    }
    match (passed, first_err) {
        (true, _) | (false, None) => Some(Ok(())),
        (false, Some(e)) => Some(Err(e)),
    }
}

pub(super) struct RealTeamLauncher {
    pub tx: UnboundedSender<app::TuiDataEvent>,
    pub provider_config: ProviderConfig,
    pub sessions: watch::Receiver<Vec<SessionMark>>,
    /// Session ids that existed for an issue before its team spawn.
    pub prior: Mutex<HashMap<IssueNumber, HashSet<Uuid>>>,
}

#[async_trait]
impl TeamLauncher for RealTeamLauncher {
    async fn spawn_for_issue(&self, issue: IssueNumber, agent_id: String) -> Result<Uuid, String> {
        let client = create_provider(&self.provider_config)
            .map_err(|e| format!("provider init failed: {e}"))?;
        let fetched = client.get_issue(issue).await.map_err(|e| e.to_string())?;
        let prior: HashSet<Uuid> = self
            .sessions
            .borrow()
            .iter()
            .filter(|m| m.issue == Some(issue))
            .map(|m| m.id)
            .collect();
        if let Ok(mut map) = self.prior.lock() {
            map.insert(issue, prior);
        }
        // Hand the fetched issue back through the same data-event the
        // single-issue launch path uses. The data-handler turns this
        // into a queued session inside the pool.
        let _ = self
            .tx
            .send(app::TuiDataEvent::Issue(Ok(fetched), None, Some(agent_id)));
        // The pool assigns the actual `Session.id` on promotion; the
        // runner's `Uuid` is only used as a per-spawn ack token. Return
        // a fresh nil-equivalent — the value is intentionally unused
        // by the runner's outcome (it only checks Ok vs Err).
        Ok(Uuid::nil())
    }

    async fn wait_until_terminal(&self, issue: IssueNumber) -> Result<(), String> {
        let prior = self
            .prior
            .lock()
            .map(|map| map.get(&issue).cloned().unwrap_or_default())
            .unwrap_or_default();
        wait_for_verdict(self.sessions.clone(), |marks| {
            issue_verdict(marks, issue, &prior)
        })
        .await
    }

    fn wave_started(&self, wave: usize) {
        let _ = self.tx.send(app::TuiDataEvent::TeamWaveStarted(wave));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(id: u128, issue: IssueNumber, status: SessionStatus, hollow: bool) -> SessionMark {
        SessionMark {
            id: Uuid::from_u128(id),
            issue: Some(issue),
            status,
            hollow,
        }
    }

    #[test]
    fn issue_verdict_ignores_sessions_from_before_the_spawn() {
        let old = mark(1, 10, SessionStatus::Completed, false);
        let prior: HashSet<Uuid> = [old.id].into();
        assert_eq!(issue_verdict(std::slice::from_ref(&old), 10, &prior), None);
        let new = mark(2, 10, SessionStatus::Paused, false);
        assert_eq!(issue_verdict(&[old.clone(), new], 10, &prior), None);
        let done = mark(2, 10, SessionStatus::Completed, false);
        assert_eq!(issue_verdict(&[old, done], 10, &prior), Some(Ok(())));
    }

    #[test]
    fn issue_verdict_waits_for_a_retry_and_passes_if_it_passes() {
        let none = HashSet::new();
        let first = mark(1, 10, SessionStatus::Killed, false);
        let retry = mark(2, 10, SessionStatus::Running, false);
        assert_eq!(issue_verdict(&[first.clone(), retry], 10, &none), None);
        let retry_done = mark(2, 10, SessionStatus::Completed, false);
        assert_eq!(
            issue_verdict(&[first.clone(), retry_done], 10, &none),
            Some(Ok(()))
        );
        let hollow = mark(2, 10, SessionStatus::Completed, true);
        assert!(matches!(
            issue_verdict(&[first, hollow], 10, &none),
            Some(Err(_))
        ));
    }

    #[test]
    fn publish_sessions_skips_work_without_listeners_and_unchanged_marks() {
        let (tx, rx) = watch::channel(Vec::new());
        drop(rx);
        let session = Session::new(
            "p".into(),
            "m".into(),
            "orchestrator".into(),
            Some(10),
            None,
        );
        publish_sessions(&tx, &[&session]);
        assert!(tx.borrow().is_empty(), "no listener: nothing published");

        let mut rx = tx.subscribe();
        publish_sessions(&tx, &[&session]);
        assert!(rx.has_changed().unwrap());
        rx.mark_unchanged();
        publish_sessions(&tx, &[&session]);
        assert!(
            !rx.has_changed().unwrap(),
            "same marks must not wake runners"
        );
        assert_eq!(rx.borrow()[0].issue, Some(10));
    }
}
