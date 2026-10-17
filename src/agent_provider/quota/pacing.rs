//! Pacing: is a rate-limit window burning faster than its time elapsed (issue #1065).
//!
//! `delta = utilization_pct - elapsed_pct`. Inside the tolerance band the window
//! is `OnTrack`. Above it, `Ahead`. Far above it, `Critical`.

use chrono::{DateTime, Duration, Utc};

use super::types::QuotaWindow;

pub const FIVE_HOUR: Duration = Duration::hours(5);
pub const SEVEN_DAY: Duration = Duration::days(7);

/// Delta (percent points) allowed before a window counts as ahead of pace.
pub const TOLERANCE_PCT: f64 = 5.0;
/// Delta (percent points) above which a window counts as critical.
pub const CRITICAL_DELTA_PCT: f64 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pacing {
    OnTrack,
    Ahead,
    Critical,
}

pub fn classify(window: &QuotaWindow, length: Duration, now: DateTime<Utc>) -> Pacing {
    let total = length.num_milliseconds() as f64;
    if total <= 0.0 {
        return Pacing::OnTrack;
    }
    let left = (window.resets_at - now).clamp(Duration::zero(), length);
    let elapsed_pct = 100.0 * (1.0 - left.num_milliseconds() as f64 / total);
    let delta = window.utilization_pct - elapsed_pct;
    if delta <= TOLERANCE_PCT {
        Pacing::OnTrack
    } else if delta <= CRITICAL_DELTA_PCT {
        Pacing::Ahead
    } else {
        Pacing::Critical
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 17, 12, 0, 0).unwrap()
    }

    /// `elapsed_pct` of a five-hour window is `100 - remaining/5h*100`.
    /// `remaining` is the time until `resets_at`.
    fn window(utilization_pct: f64, remaining: Duration) -> QuotaWindow {
        QuotaWindow {
            utilization_pct,
            resets_at: now() + remaining,
        }
    }

    #[test]
    fn pacing_classify_window_table() {
        // (utilization, time left in window, expected)
        // elapsed 0%: window just opened
        // elapsed 50%: 2h30m left of 5h
        // elapsed 100%: reset time already passed
        let cases = [
            (0.0, FIVE_HOUR, Pacing::OnTrack, "fresh window, no use"),
            (
                50.0,
                Duration::minutes(150),
                Pacing::OnTrack,
                "on pace at 50%",
            ),
            (
                55.0,
                Duration::minutes(150),
                Pacing::OnTrack,
                "delta 5 is inside band",
            ),
            (
                56.0,
                Duration::minutes(150),
                Pacing::Ahead,
                "delta 6 is over band",
            ),
            (
                70.0,
                Duration::minutes(150),
                Pacing::Ahead,
                "delta 20 is not critical yet",
            ),
            (
                71.0,
                Duration::minutes(150),
                Pacing::Critical,
                "delta 21 is critical",
            ),
            (
                40.0,
                Duration::minutes(150),
                Pacing::OnTrack,
                "under pace is on track",
            ),
            (
                90.0,
                Duration::zero(),
                Pacing::OnTrack,
                "reset passed: elapsed 100%",
            ),
            (
                0.0,
                FIVE_HOUR + Duration::hours(1),
                Pacing::OnTrack,
                "reset in future clamps to 0%",
            ),
        ];
        for (util, remaining, expected, why) in cases {
            let got = classify(&window(util, remaining), FIVE_HOUR, now());
            assert_eq!(got, expected, "{why}: util={util} remaining={remaining:?}");
        }
    }
}
