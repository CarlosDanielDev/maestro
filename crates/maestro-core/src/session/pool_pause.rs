//! Pause and exit paths for `SessionPool` (#1266): the budget cap and a
//! confirmed quit stop agents without throwing their work away.

use super::manager::ManagedSession;
use super::pool::SessionPool;
use super::retry::RetryPolicy;
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
                if managed.session.worktree_path.is_none() {
                    managed.session.worktree_path = managed.worktree_path.clone();
                }
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
            // No live turn (a quit, restart or cap stop cancelled it): a
            // SIGCONT would wake nothing; only `maestro resume` brings it back.
            if managed.session.status == SessionStatus::Paused && managed.has_live_turn() {
                let _ = managed.resume();
                let _ = (managed.session)
                    .transition_to(SessionStatus::Running, TransitionReason::UserResume);
            }
        }
    }

    /// Exit path, step 1 (#1266): record what each session becomes, before
    /// anything is stopped, so the state saved next already holds it.
    /// Running and Paused become Paused; Queued (parked at the pre-spawn
    /// gate), Spawning, Stalled, Retrying, Errored, CiFix, ConflictFix and
    /// Interactive keep their status (#1423) — `maestro resume` restarts all
    /// of these, each in its own worktree, recorded here. Only what
    /// [`Self::quit_would_end`] names is recorded Killed with reason
    /// `UserQuit`. Nothing is stopped here: call [`Self::stop_all_turns`]
    /// after the save. Returns how many sessions are Paused.
    pub fn pause_for_exit(&mut self) -> usize {
        let mut paused = 0;
        for managed in &mut self.active {
            let status = managed.session.status;
            if status.is_terminal() {
                continue;
            }
            if quit_ends(&managed.session) {
                let _ = (managed.session)
                    .transition_to(SessionStatus::Killed, TransitionReason::UserQuit);
                (managed.session).log_activity(
                    "Killed at quit: an adopted conversation cannot be resumed; adopt it again"
                        .into(),
                );
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
                | SessionStatus::Errored
                | SessionStatus::CiFix
                | SessionStatus::ConflictFix
                | SessionStatus::Interactive => {}
                // GatesRunning, NeedsPr: no agent process to pause; leave
                // them as they are.
                _ => continue,
            }
            // A session that fell back to the main checkout runs in `.`
            // (#1423): never record that as its worktree.
            if managed.session.worktree_path.is_none() {
                managed.session.worktree_path =
                    (managed.worktree_path.clone()).filter(|p| p.is_absolute());
            }
        }
        paused
    }

    /// Sessions a confirmed quit ends instead of saving for `maestro
    /// resume` (#1423), so Confirm Exit can name them before the quit.
    pub fn quit_would_end(&self) -> impl Iterator<Item = &Session> {
        (self.active.iter())
            .map(|m| &m.session)
            .filter(|s| quit_ends(s))
    }

    /// Exit path, step 2: interrupt every agent turn (after the save).
    pub fn stop_all_turns(&mut self) {
        for managed in &mut self.active {
            managed.cancel_inflight_turn();
        }
    }

    /// Keep sessions an earlier run saved for `maestro resume` in the
    /// pool's finished list, so this run's state saves do not drop them:
    /// Paused; Interactive, CiFix and ConflictFix (#1423); and, with a
    /// recorded worktree, Queued, Spawning, Stalled, Retrying and cap-stopped
    /// ones.
    /// Never an adopted conversation (resume skips it).
    pub fn keep_saved_resumable(&mut self, saved: &[Session]) {
        let resumable = |s: &&Session| {
            use SessionStatus::*;
            s.adopted_cwd.is_none()
                && match s.status {
                    Paused | Interactive | CiFix | ConflictFix => true,
                    Queued | Spawning | Stalled | Retrying => s.worktree_path.is_some(),
                    _ => RetryPolicy::is_budget_cap_stop(s) && s.worktree_path.is_some(),
                }
        };
        self.keep_saved(saved.iter().filter(resumable));
    }

    /// Keep every one of these saved records, whatever its status, in the
    /// pool's finished list. `maestro resume` passes each record it could
    /// pick up and did not restore (#1487), so a crashed Running first turn
    /// or an Errored one below the cap is still there for a later resume.
    pub fn keep_saved<'a>(&mut self, saved: impl IntoIterator<Item = &'a Session>) {
        for s in saved {
            if self.get_session(s.id).is_none() {
                self.finished.push(ManagedSession::new(s.clone()));
            }
        }
    }

    /// Paused sessions `r` can resume: this run's, not an earlier run's
    /// kept in `finished`.
    pub fn active_paused_count(&self) -> usize {
        (self.active.iter())
            .filter(|m| m.session.status == SessionStatus::Paused)
            .count()
    }
}

/// A session `maestro resume` cannot bring back (#1423): an adopted
/// conversation runs in its transcript's own cwd, never a worktree, and
/// `claude --resume` finds it only there. A quit ends it; the transcript
/// stays, so it can be adopted again.
/// #1500 (owner, 2026-11-27): NeedsPr can move to Killed for `X`, but a
/// quit keeps it as NeedsPr, as before: only `X` kills it.
fn quit_ends(s: &Session) -> bool {
    s.adopted_cwd.is_some()
        && s.status != SessionStatus::NeedsPr
        && s.status.can_transition_to(SessionStatus::Killed)
}

#[cfg(test)]
mod tests {
    use super::super::pool::tests::{make_pool, make_session};
    use super::*;

    #[test]
    fn quit_keeps_adopted_needs_pr_session() {
        let mut pool = make_pool(2);
        let mut ids = Vec::new();
        for _ in 0..2 {
            let mut session = make_session("adopted");
            session.adopted_cwd = Some("/tmp/adopted-1500".into());
            ids.push(session.id);
            pool.enqueue(session);
        }
        pool.try_promote();
        let (needs_pr, running) = (ids[0], ids[1]);
        for (id, status) in [
            (needs_pr, SessionStatus::NeedsPr),
            (running, SessionStatus::Running),
        ] {
            pool.get_active_mut(id).unwrap().session.status = status;
        }

        let ends: Vec<_> = pool.quit_would_end().map(|s| s.id).collect();
        assert_eq!(ends, vec![running], "Confirm Exit names only the running one");
        pool.pause_for_exit();
        let status = |id| pool.get_session(id).map(|s| s.status);
        assert_eq!(status(needs_pr), Some(SessionStatus::NeedsPr));
        assert_eq!(status(running), Some(SessionStatus::Killed));
    }
}
