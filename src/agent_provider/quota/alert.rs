//! Quota alert with hysteresis (issue #1065). Fires once when a window crosses
//! 97%. Re-arms only after it drops below 90%, so a window that hovers near
//! 97% does not alert on every poll.

pub const FIRE_PCT: f64 = 97.0;
pub const REARM_PCT: f64 = 90.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub fire: bool,
    pub armed: bool,
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
