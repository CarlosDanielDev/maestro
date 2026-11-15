use super::state::MascotState;
use std::time::{Duration, Instant};

const DEFAULT_FLIP_INTERVAL_MS: u64 = 850;

/// Abstraction over time for testability.
pub trait Clock {
    fn now(&self) -> Instant;
}

/// Production clock using real system time.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Tick-based frame driver. Flips between frame A (index 0) and B (index 1)
/// every 850 ms. Driven by the app's draw loop — no threads or async.
#[derive(Debug, Clone)]
pub struct MascotAnimator {
    state: MascotState,
    frame_index: usize,
    last_flip: Instant,
    flip_interval: Duration,
    revert_to: Option<(MascotState, Instant, Duration)>,
}

impl MascotAnimator {
    pub fn new(clock: &dyn Clock) -> Self {
        Self {
            state: MascotState::Idle,
            frame_index: 0,
            last_flip: clock.now(),
            flip_interval: Duration::from_millis(DEFAULT_FLIP_INTERVAL_MS),
            revert_to: None,
        }
    }

    /// Called every draw cycle. Returns true if frame or state changed.
    pub fn tick(&mut self, clock: &dyn Clock) -> bool {
        let now = clock.now();
        let mut changed = false;

        // Check auto-revert
        if let Some((target, started, duration)) = self.revert_to
            && now.duration_since(started) >= duration
        {
            self.state = target;
            self.frame_index = 0;
            self.last_flip = now;
            self.revert_to = None;
            changed = true;
        }

        // Check frame flip
        if now.duration_since(self.last_flip) >= self.flip_interval {
            self.frame_index = 1 - self.frame_index;
            self.last_flip = now;
            changed = true;
        }

        changed
    }

    /// Current frame index (0 or 1).
    pub fn frame_index(&self) -> usize {
        self.frame_index
    }

    /// Current state.
    pub fn state(&self) -> MascotState {
        self.state
    }

    /// Transition to a new state. Schedules auto-revert if applicable.
    pub fn set_state(&mut self, new_state: MascotState, clock: &dyn Clock) {
        if self.state == new_state {
            return;
        }
        self.state = new_state;
        self.frame_index = 0;
        self.last_flip = clock.now();

        self.revert_to = new_state
            .auto_revert_ms()
            .map(|ms| (MascotState::Idle, clock.now(), Duration::from_millis(ms)));
    }
}
