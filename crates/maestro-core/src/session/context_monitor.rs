#![allow(dead_code)] // Reason: context overflow monitoring — to be wired into session manager
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// An overflow event: fired when a session's context crosses the overflow threshold.
#[derive(Debug, Clone)]
pub struct ContextOverflowEvent {
    pub session_id: Uuid,
    pub context_pct: f64,
}

/// Trait for context usage monitoring, enabling mock injection in tests.
pub trait ContextMonitor: Send {
    fn record_context(&mut self, session_id: Uuid, context_pct: f64);
    fn check_overflow(&self, session_id: Uuid, threshold_pct: f64) -> Option<ContextOverflowEvent>;
    fn check_commit_prompt(&self, session_id: Uuid, threshold_pct: f64) -> bool;
    fn mark_commit_prompted(&mut self, session_id: Uuid);
    fn mark_overflow_triggered(&mut self, session_id: Uuid);
    fn remove(&mut self, session_id: Uuid);
    fn get_context_pct(&self, session_id: Uuid) -> Option<f64>;
}

/// Production implementation backed by an in-memory HashMap.
pub struct ProductionContextMonitor {
    context_map: HashMap<Uuid, f64>,
    commit_prompted: HashSet<Uuid>,
    /// Sessions that have already triggered an overflow fork.
    overflow_triggered: HashSet<Uuid>,
}

impl ProductionContextMonitor {
    pub fn new() -> Self {
        Self {
            context_map: HashMap::new(),
            commit_prompted: HashSet::new(),
            overflow_triggered: HashSet::new(),
        }
    }
}

impl Default for ProductionContextMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextMonitor for ProductionContextMonitor {
    fn record_context(&mut self, session_id: Uuid, context_pct: f64) {
        if context_pct.is_finite() && context_pct >= 0.0 {
            self.context_map
                .insert(session_id, context_pct.clamp(0.0, 1.0));
        }
    }

    fn check_overflow(&self, session_id: Uuid, threshold_pct: f64) -> Option<ContextOverflowEvent> {
        if self.overflow_triggered.contains(&session_id) {
            return None;
        }
        self.context_map.get(&session_id).and_then(|&pct| {
            if pct >= threshold_pct {
                Some(ContextOverflowEvent {
                    session_id,
                    context_pct: pct,
                })
            } else {
                None
            }
        })
    }

    fn mark_overflow_triggered(&mut self, session_id: Uuid) {
        self.overflow_triggered.insert(session_id);
    }

    fn check_commit_prompt(&self, session_id: Uuid, threshold_pct: f64) -> bool {
        self.context_map
            .get(&session_id)
            .map(|&pct| pct >= threshold_pct && !self.commit_prompted.contains(&session_id))
            .unwrap_or(false)
    }

    fn mark_commit_prompted(&mut self, session_id: Uuid) {
        self.commit_prompted.insert(session_id);
    }

    fn remove(&mut self, session_id: Uuid) {
        self.context_map.remove(&session_id);
        self.commit_prompted.remove(&session_id);
        self.overflow_triggered.remove(&session_id);
    }

    fn get_context_pct(&self, session_id: Uuid) -> Option<f64> {
        self.context_map.get(&session_id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // All context_pct values use 0.0-1.0 ratio scale (e.g., 0.7 = 70%)

    #[test]
    fn record_context_stores_value() {
        let mut monitor = ProductionContextMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_context(id, 0.45);
        assert_eq!(monitor.get_context_pct(id), Some(0.45));
    }

    #[test]
    fn record_context_overwrites_previous_value() {
        let mut monitor = ProductionContextMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_context(id, 0.30);
        monitor.record_context(id, 0.65);
        assert_eq!(monitor.get_context_pct(id), Some(0.65));
    }

    #[test]
    fn get_context_pct_returns_none_for_unknown_session() {
        let monitor = ProductionContextMonitor::new();
        assert_eq!(monitor.get_context_pct(Uuid::new_v4()), None);
    }

    #[test]
    fn check_overflow_returns_event_at_threshold() {
        let mut monitor = ProductionContextMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_context(id, 0.70);
        let event = monitor.check_overflow(id, 0.70);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.session_id, id);
        assert!((event.context_pct - 0.70).abs() < f64::EPSILON);
    }

    #[test]
    fn check_overflow_returns_event_above_threshold() {
        let mut monitor = ProductionContextMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_context(id, 0.85);
        assert!(monitor.check_overflow(id, 0.70).is_some());
    }

    #[test]
    fn check_overflow_returns_none_below_threshold() {
        let mut monitor = ProductionContextMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_context(id, 0.50);
        assert!(monitor.check_overflow(id, 0.70).is_none());
    }

    #[test]
    fn check_overflow_returns_none_for_unknown_session() {
        let monitor = ProductionContextMonitor::new();
        assert!(monitor.check_overflow(Uuid::new_v4(), 0.70).is_none());
    }

    #[test]
    fn check_commit_prompt_true_when_above_threshold_and_not_prompted() {
        let mut monitor = ProductionContextMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_context(id, 0.55);
        assert!(monitor.check_commit_prompt(id, 0.50));
    }

    #[test]
    fn check_commit_prompt_false_after_mark_commit_prompted() {
        let mut monitor = ProductionContextMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_context(id, 0.55);
        monitor.mark_commit_prompted(id);
        assert!(!monitor.check_commit_prompt(id, 0.50));
    }

    #[test]
    fn check_commit_prompt_false_below_threshold() {
        let mut monitor = ProductionContextMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_context(id, 0.30);
        assert!(!monitor.check_commit_prompt(id, 0.50));
    }

    #[test]
    fn remove_clears_session_tracking() {
        let mut monitor = ProductionContextMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_context(id, 0.90);
        monitor.remove(id);
        assert_eq!(monitor.get_context_pct(id), None);
        assert!(monitor.check_overflow(id, 0.70).is_none());
    }

    #[test]
    fn remove_clears_commit_prompted_flag() {
        let mut monitor = ProductionContextMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_context(id, 0.55);
        monitor.mark_commit_prompted(id);
        monitor.remove(id);
        monitor.record_context(id, 0.55);
        assert!(monitor.check_commit_prompt(id, 0.50));
    }

    #[test]
    fn check_overflow_returns_none_after_mark_overflow_triggered() {
        let mut monitor = ProductionContextMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_context(id, 0.85);
        monitor.mark_overflow_triggered(id);
        assert!(monitor.check_overflow(id, 0.70).is_none());
    }

    #[test]
    fn record_context_clamps_value_to_unit_range() {
        let mut monitor = ProductionContextMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_context(id, 1.5);
        assert_eq!(monitor.get_context_pct(id), Some(1.0));
    }

    #[test]
    fn record_context_rejects_nan() {
        let mut monitor = ProductionContextMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_context(id, f64::NAN);
        assert_eq!(monitor.get_context_pct(id), None);
    }

    #[test]
    fn record_context_rejects_negative() {
        let mut monitor = ProductionContextMonitor::new();
        let id = Uuid::new_v4();
        monitor.record_context(id, -0.5);
        assert_eq!(monitor.get_context_pct(id), None);
    }

    #[test]
    fn multiple_sessions_tracked_independently() {
        let mut monitor = ProductionContextMonitor::new();
        let id_a = Uuid::new_v4();
        let id_b = Uuid::new_v4();
        monitor.record_context(id_a, 0.40);
        monitor.record_context(id_b, 0.80);
        assert!(monitor.check_overflow(id_a, 0.70).is_none());
        assert!(monitor.check_overflow(id_b, 0.70).is_some());
    }
}
