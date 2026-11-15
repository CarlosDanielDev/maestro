//! Injectable clock for deterministic undo-window tests.
//!
//! Widgets that depend on wall-clock time take an `Arc<dyn Clock>` and ask
//! it for `now()` instead of calling `Instant::now()` directly. Production
//! code uses [`SystemClock`]; tests use [`FakeClock`].

#[cfg(test)]
use std::time::Duration;
use std::time::Instant;

pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

#[cfg(test)]
pub struct FakeClock {
    base: Instant,
    offset: std::sync::Mutex<Duration>,
}

#[cfg(test)]
impl FakeClock {
    pub fn new() -> Self {
        Self {
            base: Instant::now(),
            offset: std::sync::Mutex::new(Duration::ZERO),
        }
    }

    pub fn advance(&self, by: Duration) {
        let mut guard = self.offset.lock().unwrap_or_else(|e| e.into_inner());
        *guard += by;
    }
}

#[cfg(test)]
impl Clock for FakeClock {
    fn now(&self) -> Instant {
        let guard = self.offset.lock().unwrap_or_else(|e| e.into_inner());
        self.base + *guard
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn system_clock_advances_monotonically() {
        let c = SystemClock;
        let t0 = c.now();
        let t1 = c.now();
        assert!(t1 >= t0);
    }

    #[test]
    fn fake_clock_starts_at_construction() {
        let c = FakeClock::new();
        let t0 = c.now();
        let t1 = c.now();
        assert_eq!(t0, t1, "fake clock should be still until advanced");
    }

    #[test]
    fn fake_clock_advances_by_exact_duration() {
        let c = FakeClock::new();
        let t0 = c.now();
        c.advance(Duration::from_secs(3));
        let t1 = c.now();
        assert_eq!(t1 - t0, Duration::from_secs(3));
    }

    #[test]
    fn fake_clock_through_arc_dyn_clock() {
        let c: Arc<dyn Clock> = Arc::new(FakeClock::new());
        let _ = c.now();
    }
}
