//! Quota alert with hysteresis (issue #1065). Fires once when a window crosses
//! 97%. Re-arms only after it drops below 90%, so a window that hovers near
//! 97% does not alert on every poll.

use std::collections::HashMap;

use super::types::ProviderWindows;

pub const FIRE_PCT: f64 = 97.0;
pub const REARM_PCT: f64 = 90.0;

/// Longest provider id we render. Ids come from config keys, so cap them.
const MAX_ID_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub fire: bool,
    pub armed: bool,
}

/// Per-provider alert state. Keeps the armed flag between polls.
#[derive(Debug, Default)]
pub struct QuotaAlerts {
    armed: HashMap<String, bool>,
}

impl QuotaAlerts {
    /// Feed one poll. Returns `(provider, busiest percent)` for each provider
    /// that fires on this poll. Stale windows and missing data never fire and
    /// never change the armed flag.
    pub fn observe(&mut self, windows: &HashMap<String, ProviderWindows>) -> Vec<(String, f64)> {
        let mut ids: Vec<&String> = windows.keys().collect();
        ids.sort();
        let mut fired = Vec::new();
        for id in ids {
            let snapshot = &windows[id];
            if snapshot.stale {
                continue;
            }
            let Some(busiest) = busiest_pct(snapshot) else {
                continue;
            };
            let armed = self.armed.get(id).copied().unwrap_or(true);
            let decision = decide(armed, busiest);
            self.armed.insert(id.clone(), decision.armed);
            if decision.fire {
                fired.push((sanitize_provider_id(id), busiest));
            }
        }
        fired
    }
}

/// Highest utilization among the reported windows. `None` when none reported.
fn busiest_pct(windows: &ProviderWindows) -> Option<f64> {
    [
        &windows.five_hour,
        &windows.seven_day,
        &windows.seven_day_sonnet,
    ]
    .into_iter()
    .flatten()
    .map(|window| window.utilization_pct)
    .reduce(f64::max)
}

/// Keep ASCII letters, digits, `-` and `_`. Drop everything else, so a bad
/// config key cannot inject terminal escapes or newlines into the UI.
pub fn sanitize_provider_id(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(MAX_ID_LEN)
        .collect()
}

/// Pure. `armed` is the state after the previous poll. Start with `true`.
pub fn decide(armed: bool, utilization_pct: f64) -> Decision {
    if utilization_pct < REARM_PCT {
        return Decision {
            fire: false,
            armed: true,
        };
    }
    if armed && utilization_pct >= FIRE_PCT {
        return Decision {
            fire: true,
            armed: false,
        };
    }
    Decision { fire: false, armed }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_provider::quota::types::{ProviderWindows, QuotaWindow};
    use chrono::{TimeZone, Utc};
    use std::collections::HashMap;

    fn windows_at(util: f64, stale: bool) -> ProviderWindows {
        ProviderWindows {
            five_hour: Some(QuotaWindow {
                utilization_pct: util,
                resets_at: Utc.with_ymd_and_hms(2026, 10, 17, 14, 30, 0).unwrap(),
            }),
            seven_day: None,
            seven_day_sonnet: None,
            stale,
        }
    }

    fn one(id: &str, util: f64) -> HashMap<String, ProviderWindows> {
        HashMap::from([(id.to_string(), windows_at(util, false))])
    }

    #[test]
    fn observe_fires_once_per_crossing() {
        let mut alerts = QuotaAlerts::default();
        assert_eq!(alerts.observe(&one("claude", 97.0)).len(), 1);
        assert!(alerts.observe(&one("claude", 99.0)).is_empty());
        assert!(alerts.observe(&one("claude", 95.0)).is_empty());
        assert!(alerts.observe(&one("claude", 89.0)).is_empty());
        assert_eq!(alerts.observe(&one("claude", 97.5)).len(), 1);
    }

    #[test]
    fn observe_tracks_each_provider_separately() {
        let mut alerts = QuotaAlerts::default();
        let both = HashMap::from([
            ("claude".to_string(), windows_at(97.0, false)),
            ("qwen".to_string(), windows_at(50.0, false)),
        ]);
        let fired = alerts.observe(&both);
        assert_eq!(fired, vec![("claude".to_string(), 97.0)]);
        assert!(alerts.observe(&both).is_empty());
    }

    #[test]
    fn observe_skips_stale_windows() {
        let mut alerts = QuotaAlerts::default();
        let stale = HashMap::from([("claude".to_string(), windows_at(99.0, true))]);
        assert!(alerts.observe(&stale).is_empty());
    }

    #[test]
    fn observe_ignores_providers_without_data() {
        let mut alerts = QuotaAlerts::default();
        let empty = HashMap::from([(
            "claude".to_string(),
            ProviderWindows {
                five_hour: None,
                seven_day: None,
                seven_day_sonnet: None,
                stale: false,
            },
        )]);
        assert!(alerts.observe(&empty).is_empty());
    }

    #[test]
    fn sanitize_provider_id_keeps_safe_chars_and_caps_length() {
        assert_eq!(sanitize_provider_id("qwen-3_max"), "qwen-3_max");
        assert_eq!(sanitize_provider_id("bad\x1b[2J id\n"), "bad2Jid");
        assert_eq!(sanitize_provider_id(&"a".repeat(100)).len(), 32);
    }

    #[test]
    fn quota_alert_fires_once_with_hysteresis() {
        // (utilization, expected fire)
        let polls = [
            (50.0, false),
            (96.9, false),
            (97.0, true),  // crosses 97: fire once
            (99.0, false), // still high: no repeat
            (100.0, false),
            (95.0, false), // dropped, but not below 90: still disarmed
            (90.0, false), // exactly 90 does not re-arm
            (97.5, false), // disarmed: no fire
            (89.9, false), // below 90: re-arm
            (97.0, true),  // second crossing fires again
        ];
        let mut armed = true;
        for (step, (util, expect_fire)) in polls.into_iter().enumerate() {
            let decision = decide(armed, util);
            assert_eq!(
                decision.fire, expect_fire,
                "step {step}: util={util} armed_before={armed}"
            );
            armed = decision.armed;
        }
    }
}
