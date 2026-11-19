//! Pause and exit paths for `SessionPool` (#1266): the budget cap and a
//! confirmed quit stop agents without throwing their work away.

use super::manager::ManagedSession;
use super::pool::SessionPool;
use super::transition::TransitionReason;
use super::types::{Session, SessionStatus};

/// What [`SessionPool::pause_all`] did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PauseOutcome {
    /// SIGSTOPped and recorded Paused.
    pub paused: usize,
    /// No process to stop (HTTP providers report no pid; non-unix builds
    /// have no SIGSTOP): the turn was cancelled and the session recorded
    /// Errored. Only at the budget cap.
    pub stopped: usize,
}

impl SessionPool {
    /// Stop every Running session from spending. A session with a pid gets
    /// SIGSTOP and is recorded Paused. One that cannot be stopped that way
    /// is never shown as Paused: at the budget cap its turn is cancelled and
    /// it is recorded Errored (its worktree is kept and `maestro resume`
    /// picks it up); a manual pause leaves it running.
    pub fn pause_all(&mut self, reason: TransitionReason) -> PauseOutcome {
        let mut out = PauseOutcome::default();
        for managed in &mut self.active {
            if managed.session.status != SessionStatus::Running {
                continue;
            }
            if cfg!(unix) && managed.session.pid.is_some() {
                #[cfg(unix)]
                let _ = managed.pause();
                if (managed.session)
                    .transition_to(SessionStatus::Paused, reason)
                    .is_ok()
                {
                    out.paused += 1;
                }
            } else if reason == TransitionReason::BudgetCap {
                managed.cancel_inflight_turn();
                let _ = (managed.session).transition_to(SessionStatus::Errored, reason);
                managed.session.log_activity(
                    "Error: stopped at the budget cap (this agent has no process to pause)".into(),
                );
                out.stopped += 1;
            }
        }
        out
    }

    #[cfg(unix)]
    pub fn resume_all(&mut self) {
        for managed in &mut self.active {
            if managed.session.status == SessionStatus::Paused {
                let _ = managed.resume();
                let _ = (managed.session)
                    .transition_to(SessionStatus::Running, TransitionReason::UserResume);
            }
        }
    }

    /// Exit path, step 1 (#1266): record what each session becomes, before
    /// anything is stopped, so the state saved next already holds it.
    /// Running and Paused become Paused; Queued (parked at the pre-spawn
    /// gate), Spawning, Stalled, Retrying and Errored keep their status —
    /// `maestro resume` restarts all of these, each in its own worktree,
    /// recorded here. A status that can neither pause nor resume
    /// (Interactive, CiFix, ConflictFix, …) is recorded Killed with reason
    /// `UserQuit`. Nothing is stopped here: call [`Self::stop_all_turns`]
    /// after the save. Returns how many sessions are Paused.
    pub fn pause_for_exit(&mut self) -> usize {
        let mut paused = 0;
        for managed in &mut self.active {
            let status = managed.session.status;
            if status.is_terminal() {
                continue;
            }
            match status {
                SessionStatus::Running | SessionStatus::Paused => {
                    let _ = (managed.session)
                        .transition_to(SessionStatus::Paused, TransitionReason::UserQuit);
                    paused += 1;
                }
                SessionStatus::Queued
                | SessionStatus::Spawning
                | SessionStatus::Stalled
                | SessionStatus::Retrying
                | SessionStatus::Errored => {}
                _ => {
                    let _ = (managed.session)
                        .transition_to(SessionStatus::Killed, TransitionReason::UserQuit);
                    (managed.session)
                        .log_activity("Killed at quit: this status cannot pause".into());
                    continue;
                }
            }
            if managed.session.worktree_path.is_none() {
                managed.session.worktree_path = managed.worktree_path.clone();
            }
        }
        paused
    }

    /// Exit path, step 2: interrupt every agent turn (after the save).
    pub fn stop_all_turns(&mut self) {
        for managed in &mut self.active {
            managed.cancel_inflight_turn();
        }
    }

    /// Keep Paused sessions saved by an earlier run in the pool's finished
    /// list, so this run's state saves do not drop them before
    /// `maestro resume` gets to them.
    pub fn keep_saved_paused(&mut self, saved: &[Session]) {
        for s in saved.iter().filter(|s| s.status == SessionStatus::Paused) {
            if self.get_session(s.id).is_none() {
                self.finished.push(ManagedSession::new(s.clone()));
            }
        }
    }
}
