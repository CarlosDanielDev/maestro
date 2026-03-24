use std::collections::HashSet;
use uuid::Uuid;

/// Action the enforcer recommends after a budget check.
#[derive(Debug, Clone, PartialEq)]
pub enum BudgetAction {
    /// Within budget, no action needed.
    Ok,
    /// Budget threshold crossed — emit a warning. Contains the current percentage.
    Alert(u8),
    /// Budget exceeded — kill the session or all sessions.
    Kill,
}

/// Trait for budget enforcement, enabling mock injection in tests.
pub trait BudgetCheck: Send {
    /// Check whether a single session has exceeded its per-session budget.
    fn check_session(&self, session_cost: f64) -> BudgetAction;

    /// Check whether the global budget has been exceeded.
    fn check_global(&self, total_cost: f64) -> BudgetAction;

    /// Record that an alert has been sent for a given session to avoid duplicates.
    /// Returns `true` if this is a new alert (not previously sent).
    fn record_alert(&mut self, session_id: Uuid) -> bool;
}

/// Production budget enforcer backed by config values.
pub struct BudgetEnforcer {
    per_session_usd: f64,
    total_usd: f64,
    alert_threshold_pct: u8,
    alerted_sessions: HashSet<Uuid>,
    global_alert_sent: bool,
}

impl BudgetEnforcer {
    pub fn new(per_session_usd: f64, total_usd: f64, alert_threshold_pct: u8) -> Self {
        Self {
            per_session_usd,
            total_usd,
            alert_threshold_pct,
            alerted_sessions: HashSet::new(),
            global_alert_sent: false,
        }
    }

    fn compute_action(&self, spent: f64, limit: f64) -> BudgetAction {
        if limit <= 0.0 {
            return BudgetAction::Ok;
        }
        let pct = ((spent / limit) * 100.0) as u8;
        if spent >= limit {
            BudgetAction::Kill
        } else if pct >= self.alert_threshold_pct {
            BudgetAction::Alert(pct)
        } else {
            BudgetAction::Ok
        }
    }
}

impl BudgetCheck for BudgetEnforcer {
    fn check_session(&self, session_cost: f64) -> BudgetAction {
        self.compute_action(session_cost, self.per_session_usd)
    }

    fn check_global(&self, total_cost: f64) -> BudgetAction {
        self.compute_action(total_cost, self.total_usd)
    }

    fn record_alert(&mut self, session_id: Uuid) -> bool {
        self.alerted_sessions.insert(session_id)
    }
}

impl BudgetEnforcer {
    /// Whether a global alert has already been sent (to deduplicate).
    pub fn global_alert_sent(&self) -> bool {
        self.global_alert_sent
    }

    /// Mark global alert as sent.
    pub fn mark_global_alert_sent(&mut self) {
        self.global_alert_sent = true;
    }

    /// Get per-session limit for display.
    pub fn per_session_limit(&self) -> f64 {
        self.per_session_usd
    }

    /// Get total limit for display.
    pub fn total_limit(&self) -> f64 {
        self.total_usd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_session_ok_when_under_budget() {
        let enforcer = BudgetEnforcer::new(5.0, 50.0, 80);
        assert_eq!(enforcer.check_session(2.0), BudgetAction::Ok);
    }

    #[test]
    fn check_session_alert_at_threshold() {
        let enforcer = BudgetEnforcer::new(5.0, 50.0, 80);
        assert_eq!(enforcer.check_session(4.0), BudgetAction::Alert(80));
    }

    #[test]
    fn check_session_kill_at_limit() {
        let enforcer = BudgetEnforcer::new(5.0, 50.0, 80);
        assert_eq!(enforcer.check_session(5.0), BudgetAction::Kill);
    }

    #[test]
    fn check_session_kill_over_limit() {
        let enforcer = BudgetEnforcer::new(5.0, 50.0, 80);
        assert_eq!(enforcer.check_session(6.0), BudgetAction::Kill);
    }

    #[test]
    fn check_global_ok_when_under_budget() {
        let enforcer = BudgetEnforcer::new(5.0, 50.0, 80);
        assert_eq!(enforcer.check_global(10.0), BudgetAction::Ok);
    }

    #[test]
    fn check_global_alert_at_threshold() {
        let enforcer = BudgetEnforcer::new(5.0, 50.0, 80);
        assert_eq!(enforcer.check_global(40.0), BudgetAction::Alert(80));
    }

    #[test]
    fn check_global_kill_at_limit() {
        let enforcer = BudgetEnforcer::new(5.0, 50.0, 80);
        assert_eq!(enforcer.check_global(50.0), BudgetAction::Kill);
    }

    #[test]
    fn check_global_kill_over_limit() {
        let enforcer = BudgetEnforcer::new(5.0, 50.0, 80);
        assert_eq!(enforcer.check_global(55.0), BudgetAction::Kill);
    }

    #[test]
    fn zero_limit_always_ok() {
        let enforcer = BudgetEnforcer::new(0.0, 0.0, 80);
        assert_eq!(enforcer.check_session(100.0), BudgetAction::Ok);
        assert_eq!(enforcer.check_global(100.0), BudgetAction::Ok);
    }

    #[test]
    fn record_alert_returns_true_first_time() {
        let mut enforcer = BudgetEnforcer::new(5.0, 50.0, 80);
        let id = Uuid::new_v4();
        assert!(enforcer.record_alert(id));
    }

    #[test]
    fn record_alert_returns_false_on_duplicate() {
        let mut enforcer = BudgetEnforcer::new(5.0, 50.0, 80);
        let id = Uuid::new_v4();
        enforcer.record_alert(id);
        assert!(!enforcer.record_alert(id));
    }

    #[test]
    fn global_alert_sent_initially_false() {
        let enforcer = BudgetEnforcer::new(5.0, 50.0, 80);
        assert!(!enforcer.global_alert_sent());
    }

    #[test]
    fn mark_global_alert_sent_sets_flag() {
        let mut enforcer = BudgetEnforcer::new(5.0, 50.0, 80);
        enforcer.mark_global_alert_sent();
        assert!(enforcer.global_alert_sent());
    }

    #[test]
    fn alert_at_90_percent_with_80_threshold() {
        let enforcer = BudgetEnforcer::new(10.0, 100.0, 80);
        assert_eq!(enforcer.check_session(9.0), BudgetAction::Alert(90));
        assert_eq!(enforcer.check_global(90.0), BudgetAction::Alert(90));
    }
}
