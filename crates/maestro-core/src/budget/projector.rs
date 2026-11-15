//! Per-session next-turn cost projection for the pre-spawn budget gate (#776/#848).
//!
//! The trait `BudgetProjector` returns a projected USD cost for the *next* turn
//! of a session. `DefaultBudgetProjector` uses `max(session.cost_usd, floor)`
//! where `floor = per_session_usd * floor_pct`. There is no per-turn ledger
//! today, so `session.cost_usd` (cumulative) is the best proxy for "last turn
//! cost" once a session has run at least once. The floor protects the projection
//! for fresh sessions that have not yet recorded any cost.
//!
//! Tests inject `FakeBudgetProjector` (defined inline in test mods).

use crate::budget::sanitize::sanitize_cost;
use crate::session::types::Session;

/// Project the next-turn USD cost for a session.
pub trait BudgetProjector: Send + Sync {
    fn projected_turn_cost(&self, session: &Session) -> f64;
}

/// Production projector — returns `max(session.cost_usd, per_session_usd * floor_pct)`.
/// NaN / negative `cost_usd` sanitized to the floor.
#[derive(Debug, Clone, Copy)]
pub struct DefaultBudgetProjector {
    per_session_usd: f64,
    floor_pct: f64,
}

impl DefaultBudgetProjector {
    /// Construct with the current per-session limit and a floor fraction
    /// (e.g. `0.10` for 10% of the limit as the minimum projection).
    pub fn new(per_session_usd: f64, floor_pct: f64) -> Self {
        Self {
            per_session_usd,
            floor_pct,
        }
    }

    fn floor(&self) -> f64 {
        self.per_session_usd * self.floor_pct
    }
}

impl BudgetProjector for DefaultBudgetProjector {
    fn projected_turn_cost(&self, session: &Session) -> f64 {
        let floor = self.floor();
        let usable = sanitize_cost(session.cost_usd);
        if usable > floor { usable } else { floor }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::test_support::make_session;

    /// Model string is irrelevant to projection math; tag with a placeholder
    /// so call-sites don't read as Claude-specific.
    const TEST_MODEL: &str = "test-model";

    #[test]
    fn default_budget_projector_zero_history_returns_floor() {
        let projector = DefaultBudgetProjector::new(5.0, 0.10);
        let session = make_session(None, 0.0, TEST_MODEL);
        assert_eq!(projector.projected_turn_cost(&session), 0.50);
    }

    #[test]
    fn default_budget_projector_with_history_returns_max_of_delta_and_floor() {
        let projector = DefaultBudgetProjector::new(5.0, 0.10);
        let session = make_session(None, 0.80, TEST_MODEL);
        assert_eq!(projector.projected_turn_cost(&session), 0.80);
    }

    #[test]
    fn default_budget_projector_nan_cost_returns_floor() {
        let projector = DefaultBudgetProjector::new(5.0, 0.10);
        let session = make_session(None, f64::NAN, TEST_MODEL);
        assert_eq!(projector.projected_turn_cost(&session), 0.50);
    }

    #[test]
    fn default_budget_projector_cost_below_floor_returns_floor() {
        let projector = DefaultBudgetProjector::new(5.0, 0.10);
        let session = make_session(None, 0.10, TEST_MODEL);
        assert_eq!(projector.projected_turn_cost(&session), 0.50);
    }
}
