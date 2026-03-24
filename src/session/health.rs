use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Trait for health monitoring, enabling mock injection in tests.
pub trait HealthCheck: Send {
    /// Record that a session produced activity just now.
    fn record_activity(&mut self, session_id: Uuid);

    /// Remove tracking for a session (e.g., when it completes).
    fn remove(&mut self, session_id: Uuid);

    /// Return IDs of sessions whose last activity exceeds `timeout`.
    fn check_stalls(&self, timeout: Duration) -> Vec<Uuid>;
}

/// Production implementation backed by `Instant` timestamps.
pub struct HealthMonitor {
    last_event: HashMap<Uuid, Instant>,
}

impl HealthMonitor {
    pub fn new() -> Self {
        Self {
            last_event: HashMap::new(),
        }
    }
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthCheck for HealthMonitor {
    fn record_activity(&mut self, session_id: Uuid) {
        self.last_event.insert(session_id, Instant::now());
    }

    fn remove(&mut self, session_id: Uuid) {
        self.last_event.remove(&session_id);
    }

    fn check_stalls(&self, timeout: Duration) -> Vec<Uuid> {
        let now = Instant::now();
        self.last_event
            .iter()
            .filter(|(_, last)| now.duration_since(**last) > timeout)
            .map(|(id, _)| *id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn record_activity_tracks_session() {
        let mut monitor = HealthMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_activity(id);
        let stalled = monitor.check_stalls(Duration::from_secs(9999));
        assert!(stalled.is_empty());
    }

    #[test]
    fn check_stalls_detects_stalled_session() {
        let mut monitor = HealthMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_activity(id);

        thread::sleep(Duration::from_millis(20));
        let stalled = monitor.check_stalls(Duration::from_millis(5));
        assert_eq!(stalled.len(), 1);
        assert_eq!(stalled[0], id);
    }

    #[test]
    fn check_stalls_ignores_active_sessions() {
        let mut monitor = HealthMonitor::new();
        let stale_id = Uuid::new_v4();
        monitor.record_activity(stale_id);

        thread::sleep(Duration::from_millis(20));

        let fresh_id = Uuid::new_v4();
        monitor.record_activity(fresh_id);

        let stalled = monitor.check_stalls(Duration::from_millis(10));
        assert_eq!(stalled.len(), 1);
        assert_eq!(stalled[0], stale_id);
    }

    #[test]
    fn remove_stops_tracking() {
        let mut monitor = HealthMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_activity(id);
        monitor.remove(id);

        thread::sleep(Duration::from_millis(20));
        let stalled = monitor.check_stalls(Duration::from_millis(5));
        assert!(stalled.is_empty());
    }

    #[test]
    fn record_activity_resets_timer() {
        let mut monitor = HealthMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_activity(id);

        thread::sleep(Duration::from_millis(20));
        monitor.record_activity(id);

        let stalled = monitor.check_stalls(Duration::from_millis(50));
        assert!(stalled.is_empty());
    }

    #[test]
    fn check_stalls_empty_monitor_returns_empty() {
        let monitor = HealthMonitor::new();
        let stalled = monitor.check_stalls(Duration::from_millis(1));
        assert!(stalled.is_empty());
    }

    #[test]
    fn default_creates_empty_monitor() {
        let monitor = HealthMonitor::default();
        assert!(monitor.last_event.is_empty());
    }
}
