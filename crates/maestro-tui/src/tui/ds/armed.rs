//! Armed-Enter: the only way a screen starts a GitHub write.
//!
//! The first Enter arms and the bar shows the exact write; a second Enter
//! within [`ARM_WINDOW`] commits. The window expiring, Esc or any other key
//! disarms. Blocked is inert: Enter does nothing until the reason is fixed.
//! Pure state machine: the caller passes `now`, so tests use a fake clock.
//! Drawn by `widgets::armed_bar::ArmedBar`.

use std::time::{Duration, Instant};

/// How long an armed write waits for the second Enter.
pub const ARM_WINDOW: Duration = Duration::from_secs(3);

/// Where a write is in its life.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArmedState {
    /// The write cannot run; the reason says what to fix first.
    Blocked(String),
    /// Nothing written yet; Enter arms.
    Ready,
    /// Armed at this instant; Enter within `ARM_WINDOW` commits.
    Armed(Instant),
    /// The write is running.
    Working,
    /// The write finished.
    Done,
    /// The write failed; the text says how to fix it.
    Failed(String),
}

/// What an Enter press did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnterOutcome {
    /// Nothing happened (blocked, working, done or failed).
    Ignored,
    /// The write is now armed.
    Armed,
    /// Start the write now. The state is `Working` until `finish`.
    Commit,
}

/// Armed-Enter state machine for one GitHub write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmedEnter {
    state: ArmedState,
}

impl Default for ArmedEnter {
    fn default() -> Self {
        Self::new()
    }
}

impl ArmedEnter {
    /// Starts ready.
    pub fn new() -> Self {
        Self {
            state: ArmedState::Ready,
        }
    }

    pub fn state(&self) -> &ArmedState {
        &self.state
    }

    /// Blocks the write with a reason (for example a lint error).
    pub fn block(&mut self, reason: impl Into<String>) {
        self.state = ArmedState::Blocked(reason.into());
    }

    /// Back to ready: the block is fixed, or the user wants to retry.
    pub fn reset(&mut self) {
        self.state = ArmedState::Ready;
    }

    /// Enter: ready arms, armed within the window commits.
    /// A late second Enter re-arms instead of committing.
    pub fn on_enter(&mut self, now: Instant) -> EnterOutcome {
        match self.state {
            ArmedState::Armed(at) if now.duration_since(at) <= ARM_WINDOW => {
                self.state = ArmedState::Working;
                EnterOutcome::Commit
            }
            ArmedState::Ready | ArmedState::Armed(_) => {
                self.state = ArmedState::Armed(now);
                EnterOutcome::Armed
            }
            _ => EnterOutcome::Ignored,
        }
    }

    /// App tick: an armed write whose window passed goes back to ready.
    pub fn tick(&mut self, now: Instant) {
        if let ArmedState::Armed(at) = self.state
            && now.duration_since(at) > ARM_WINDOW
        {
            self.state = ArmedState::Ready;
        }
    }

    /// Esc or any other key: an armed write goes back to ready.
    pub fn disarm(&mut self) {
        if matches!(self.state, ArmedState::Armed(_)) {
            self.state = ArmedState::Ready;
        }
    }

    /// The write returned. `Err` carries the fix to show.
    pub fn finish(&mut self, result: Result<(), String>) {
        if self.state == ArmedState::Working {
            self.state = match result {
                Ok(()) => ArmedState::Done,
                Err(fix) => ArmedState::Failed(fix),
            };
        }
    }

    /// Time left in the armed window, or `None` when not armed.
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        match self.state {
            ArmedState::Armed(at) => Some(ARM_WINDOW.saturating_sub(now.duration_since(at))),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: Duration = Duration::from_secs(1);
    const Z: Duration = Duration::ZERO;

    #[derive(Debug, Clone, Copy)]
    enum Step {
        Enter(Duration),
        Tick(Duration),
        Disarm,
        Reset,
        Finish(bool),
    }

    fn kind(state: &ArmedState) -> &'static str {
        match state {
            ArmedState::Blocked(_) => "blocked",
            ArmedState::Ready => "ready",
            ArmedState::Armed(_) => "armed",
            ArmedState::Working => "working",
            ArmedState::Done => "done",
            ArmedState::Failed(_) => "failed",
        }
    }

    #[test]
    fn armed_enter_transitions() {
        use Step::*;
        // (case, start blocked?, steps, expected state after each step)
        let table: &[(&str, bool, &[Step], &[&str])] = &[
            (
                "blocked -> ready -> armed -> working -> done",
                true,
                &[Enter(Z), Reset, Enter(S), Enter(S * 2), Finish(true)],
                &["blocked", "ready", "armed", "working", "done"],
            ),
            (
                "second Enter at exactly 3s still commits",
                false,
                &[Enter(Z), Enter(S * 3)],
                &["armed", "working"],
            ),
            (
                "working -> failed",
                false,
                &[Enter(Z), Enter(S), Finish(false)],
                &["armed", "working", "failed"],
            ),
            (
                "tick inside the window keeps armed",
                false,
                &[Enter(Z), Tick(S * 3)],
                &["armed", "armed"],
            ),
            (
                "armed expires to ready after 3s",
                false,
                &[Enter(Z), Tick(S * 3 + Duration::from_millis(1))],
                &["armed", "ready"],
            ),
            (
                "late second Enter re-arms instead of committing",
                false,
                &[Enter(Z), Enter(S * 4)],
                &["armed", "armed"],
            ),
            (
                "Esc disarms",
                false,
                &[Enter(Z), Disarm],
                &["armed", "ready"],
            ),
            (
                "Enter while working does nothing",
                false,
                &[Enter(Z), Enter(S), Enter(S * 2)],
                &["armed", "working", "working"],
            ),
            (
                "Enter after done does nothing",
                false,
                &[Enter(Z), Enter(S), Finish(true), Enter(S * 2)],
                &["armed", "working", "done", "done"],
            ),
            (
                "finish outside working is ignored",
                false,
                &[Finish(true), Enter(Z), Finish(false)],
                &["ready", "armed", "armed"],
            ),
        ];

        let t0 = Instant::now();
        for (case, blocked, steps, expected) in table {
            let mut armed = ArmedEnter::new();
            if *blocked {
                armed.block("2 placeholders left");
            }
            assert_eq!(steps.len(), expected.len(), "{case}: table row lengths");
            for (i, (step, want)) in steps.iter().zip(expected.iter()).enumerate() {
                match *step {
                    Enter(at) => {
                        armed.on_enter(t0 + at);
                    }
                    Reset => armed.reset(),
                    Tick(at) => armed.tick(t0 + at),
                    Disarm => armed.disarm(),
                    Finish(ok) => armed.finish(if ok { Ok(()) } else { Err("retry".into()) }),
                }
                assert_eq!(kind(armed.state()), *want, "{case}: step {i} {step:?}");
            }
        }
    }

    #[test]
    fn armed_enter_outcomes_and_remaining() {
        let t0 = Instant::now();
        let mut armed = ArmedEnter::new();
        assert_eq!(armed.remaining(t0), None);
        assert_eq!(armed.on_enter(t0), EnterOutcome::Armed);
        assert_eq!(armed.remaining(t0 + S), Some(S * 2));
        assert_eq!(armed.remaining(t0 + S * 5), Some(Duration::ZERO));
        assert_eq!(armed.on_enter(t0 + S), EnterOutcome::Commit);
        assert_eq!(armed.on_enter(t0 + S * 2), EnterOutcome::Ignored);
        armed.finish(Err("gh: HTTP 401".into()));
        assert_eq!(armed.state(), &ArmedState::Failed("gh: HTTP 401".into()));
    }

    #[test]
    fn armed_enter_blocked_is_inert() {
        let t0 = Instant::now();
        let mut armed = ArmedEnter::new();
        armed.block("2 placeholders left");
        for at in [0, 1, 2] {
            assert_eq!(armed.on_enter(t0 + S * at), EnterOutcome::Ignored);
        }
        armed.tick(t0 + S * 10);
        armed.disarm();
        assert_eq!(
            armed.state(),
            &ArmedState::Blocked("2 placeholders left".into())
        );
        assert_eq!(armed.remaining(t0), None);
    }
}
