//! Quota-aware provider routing for `SessionPool` (issue #1065).
//!
//! Glue only. The pick lives in `agent_provider::quota::routing`. The pool
//! reads cached windows and a clock on each decision. It never fetches.

use std::collections::HashMap;
use std::sync::Arc;

use super::pool::SessionPool;
use crate::agent_provider::AgentProvider;
use crate::agent_provider::minimax::quota::Clock;
use crate::agent_provider::quota::routing;
use crate::agent_provider::quota::types::ProviderWindows;

/// Quota windows per provider id, plus the clock read on every decision.
/// The clock is read per decision, never cached, so pacing follows wall time.
#[derive(Debug)]
pub(super) struct QuotaRouting {
    pub(super) windows: HashMap<String, ProviderWindows>,
    pub(super) clock: Box<dyn Clock>,
}

impl SessionPool {
    /// Turn headroom routing on. Off by default (issue #1065).
    pub fn set_quota_routing(
        &mut self,
        windows: HashMap<String, ProviderWindows>,
        clock: Box<dyn Clock>,
    ) {
        self.quota_routing = Some(QuotaRouting { windows, clock });
    }

    /// Every provider id the pool can launch: the default plus each agent.
    /// Sorted and unique, so badge rows and refresh order are stable.
    pub fn quota_provider_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = std::iter::once(self.provider.id().to_string())
            .chain(self.agent_providers.keys().cloned())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// The configured provider with the most window headroom among on-track
    /// candidates. `None` when routing is off or no candidate is eligible.
    pub(crate) fn quota_routed_provider(&self) -> Option<&Arc<dyn AgentProvider>> {
        let routing_input = self.quota_routing.as_ref()?;
        let now = routing_input.clock.now();
        let default_id = self.provider.id();
        let mut agent_ids: Vec<&String> = self.agent_providers.keys().collect();
        agent_ids.sort();
        let candidates: Vec<(&str, Option<ProviderWindows>)> = std::iter::once(default_id)
            .chain(agent_ids.into_iter().map(String::as_str))
            .map(|id| (id, routing_input.windows.get(id).cloned()))
            .collect();
        let picked = routing::pick(&candidates, now)?;
        if picked == default_id {
            Some(&self.provider)
        } else {
            self.agent_providers.get(picked)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::pool::tests::{make_pool, make_session};
    use super::*;
    use crate::agent_provider::quota::types::QuotaWindow;
    use crate::agent_provider::test_fakes::FakeHttpProvider;
    use chrono::{DateTime, TimeZone, Utc};
    use std::sync::Mutex;

    /// Test clock. The test moves it between decisions.
    #[derive(Debug, Clone)]
    struct SharedClock(Arc<Mutex<DateTime<Utc>>>);

    impl Clock for SharedClock {
        fn now(&self) -> DateTime<Utc> {
            *self.0.lock().expect("clock lock")
        }
    }

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 17, hour, minute, 0).unwrap()
    }

    /// Five-hour window that resets at 14:30. Elapsed is 50% at 12:00, 30% at 11:00.
    fn window(util: f64) -> ProviderWindows {
        ProviderWindows {
            five_hour: Some(QuotaWindow {
                utilization_pct: util,
                resets_at: at(14, 30),
            }),
            seven_day: None,
            seven_day_sonnet: None,
            stale: false,
        }
    }

    fn pool_with_qwen(
        windows: HashMap<String, ProviderWindows>,
        clock: SharedClock,
    ) -> SessionPool {
        let mut pool = make_pool(4);
        let mut providers: HashMap<String, Arc<dyn AgentProvider>> = HashMap::new();
        providers.insert("qwen".to_string(), Arc::new(FakeHttpProvider));
        pool.set_agent_providers(providers);
        pool.set_quota_routing(windows, Box::new(clock));
        pool
    }

    fn promoted_provider(pool: &mut SessionPool, prompt: &str) -> String {
        pool.enqueue(make_session(prompt));
        let ids = pool.try_promote();
        pool.get_active_mut(ids[0])
            .expect("promoted")
            .provider_id()
            .to_string()
    }

    #[test]
    fn no_explicit_provider_routes_to_on_track_candidate() {
        // Claude is critical (util 90 at 50% elapsed), qwen is on track.
        let mut windows = HashMap::new();
        windows.insert("claude".to_string(), window(90.0));
        windows.insert("qwen".to_string(), window(40.0));
        let clock = SharedClock(Arc::new(Mutex::new(at(12, 0))));
        let mut pool = pool_with_qwen(windows, clock);

        assert_eq!(promoted_provider(&mut pool, "routed"), "qwen");
    }

    #[test]
    fn explicit_provider_always_wins_over_quota_routing() {
        // qwen has the headroom, but the session names claude. Claude must win.
        let mut windows = HashMap::new();
        windows.insert("claude".to_string(), window(90.0));
        windows.insert("qwen".to_string(), window(40.0));
        let clock = SharedClock(Arc::new(Mutex::new(at(12, 0))));
        let mut pool = pool_with_qwen(windows, clock);
        pool.enqueue(make_session("pinned").with_agent_id(Some("claude".to_string())));

        let ids = pool.try_promote();
        let managed = pool.get_active_mut(ids[0]).expect("promoted");
        assert_eq!(managed.provider_id(), "claude");
    }

    #[test]
    fn quota_routing_off_keeps_default_provider() {
        // No routing input (feature off): the launch keeps the Claude default.
        let mut pool = make_pool(4);
        assert_eq!(promoted_provider(&mut pool, "default"), "claude");
    }

    #[test]
    fn routing_reads_clock_per_decision() {
        // At 11:00 qwen is ahead of pace (util 40, 30% elapsed): not eligible.
        // Claude has no data. Default wins. At 12:00 qwen is on track: it wins.
        let mut windows = HashMap::new();
        windows.insert("qwen".to_string(), window(40.0));
        let shared = Arc::new(Mutex::new(at(11, 0)));
        let clock = SharedClock(Arc::clone(&shared));
        let mut pool = pool_with_qwen(windows, clock);

        assert_eq!(promoted_provider(&mut pool, "early"), "claude");
        *shared.lock().expect("clock lock") = at(12, 0);
        assert_eq!(promoted_provider(&mut pool, "later"), "qwen");
    }

    #[test]
    fn stale_windows_never_route() {
        // qwen shows 0% but is stale: it must not beat the default.
        let mut stale = window(0.0);
        stale.stale = true;
        let mut windows = HashMap::new();
        windows.insert("qwen".to_string(), stale);
        let clock = SharedClock(Arc::new(Mutex::new(at(12, 0))));
        let mut pool = pool_with_qwen(windows, clock);

        assert_eq!(promoted_provider(&mut pool, "stale"), "claude");
    }
}
