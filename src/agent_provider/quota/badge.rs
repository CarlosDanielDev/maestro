//! One-line quota badge per provider (issue #1065). Pure text from already
//! cached windows. Never fetches, never reads the disk, never shows 0% for a
//! missing window.

use chrono::{DateTime, Duration, Utc};

use super::pacing::{self, Pacing};
use super::types::ProviderWindows;

/// `claude 90% ↻2h30m ⇑`. The window with the highest use is the one shown.
/// `None` renders `<provider> no data`.
pub fn badge(provider_id: &str, windows: Option<&ProviderWindows>, now: DateTime<Utc>) -> String {
    let no_data = format!("{provider_id} no data");
    let Some(windows) = windows else {
        return no_data;
    };
    let reported = [
        (&windows.five_hour, pacing::FIVE_HOUR),
        (&windows.seven_day, pacing::SEVEN_DAY),
        (&windows.seven_day_sonnet, pacing::SEVEN_DAY),
    ];
    let busiest = reported
        .iter()
        .filter_map(|(window, length)| window.as_ref().map(|w| (w, *length)))
        .max_by(|a, b| a.0.utilization_pct.total_cmp(&b.0.utilization_pct));
    let Some((window, length)) = busiest else {
        return no_data;
    };
    let glyph = match pacing::classify(window, length, now) {
        Pacing::OnTrack => "→",
        Pacing::Ahead => "↑",
        Pacing::Critical => "⇑",
    };
    let resets_in = format_resets_in(window.resets_at - now);
    let mut line = format!(
        "{provider_id} {:.0}% ↻{resets_in} {glyph}",
        window.utilization_pct
    );
    if windows.stale {
        line.push_str(" stale");
    }
    line
}

/// `2d4h`, `2h30m`, or `45m`. Past reset times show `0m`.
fn format_resets_in(left: Duration) -> String {
    let minutes = left.num_minutes().max(0);
    let (days, hours, mins) = (minutes / 1440, (minutes % 1440) / 60, minutes % 60);
    if days > 0 {
        format!("{days}d{hours}h")
    } else if hours > 0 {
        format!("{hours}h{mins}m")
    } else {
        format!("{mins}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_provider::quota::types::QuotaWindow;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 17, 12, 0, 0).unwrap()
    }

    /// Five-hour window, 150 min left (50% elapsed).
    fn five_hour(util: f64) -> Option<QuotaWindow> {
        Some(QuotaWindow {
            utilization_pct: util,
            resets_at: now() + Duration::minutes(150),
        })
    }

    #[test]
    fn quota_badge_renders_each_provider_state() {
        let claude = ProviderWindows {
            five_hour: five_hour(90.0), // critical
            seven_day: None,
            seven_day_sonnet: None,
            stale: false,
        };
        let qwen = ProviderWindows {
            five_hour: five_hour(40.0), // on track
            seven_day: None,
            seven_day_sonnet: None,
            stale: true,
        };
        let rendered = [
            badge("claude", Some(&claude), now()),
            badge("qwen", Some(&qwen), now()),
            badge("minimax", None, now()),
        ]
        .join("\n");

        insta::assert_snapshot!(rendered, @r"
        claude 90% ↻2h30m ⇑
        qwen 40% ↻2h30m → stale
        minimax no data
        ");
    }
}
