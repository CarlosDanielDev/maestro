//! Headroom routing (issue #1065). Pick the provider with the most window
//! headroom among providers that are `OnTrack`. Pure: no I/O, time is passed in.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use super::pacing::{self, Pacing};
use super::types::ProviderWindows;

/// Headroom of a provider = remaining percent of its binding window, the window
/// with the least headroom. `None` when the provider is not eligible: no window
/// reported, or any reported window is not `OnTrack`.
fn eligible_headroom(windows: &ProviderWindows, now: DateTime<Utc>) -> Option<f64> {
    // A stale snapshot can show an old 0%. It must never win over fresh data.
    // A stale snapshot can show an old 0%. It must never win over fresh data.
    if windows.stale {
        return None;
    }
    let reported = [
        (&windows.five_hour, pacing::FIVE_HOUR),
        (&windows.seven_day, pacing::SEVEN_DAY),
        (&windows.seven_day_sonnet, pacing::SEVEN_DAY),
    ];
    let mut binding: Option<f64> = None;
    for (window, length) in reported {
        let Some(window) = window else { continue };
        if pacing::classify(window, length, now) != Pacing::OnTrack {
            return None;
        }
        let headroom = 100.0 - window.utilization_pct;
        binding = Some(binding.map_or(headroom, |least| least.min(headroom)));
    }
    binding
}

/// `candidates` are `(provider_id, windows)`, checked in the order given.
/// Returns the id with the most headroom. A tie keeps the earlier candidate.
/// A provider is eligible only when its windows are fresh, every reported
/// window is `OnTrack`, and at least one window is reported. `None` windows
/// are never eligible and never count as 0%.
pub fn pick<'a>(
    candidates: &[(&'a str, Option<ProviderWindows>)],
    now: DateTime<Utc>,
) -> Option<&'a str> {
    let mut best: Option<(&'a str, f64)> = None;
    for (id, windows) in candidates {
        let Some(headroom) = windows.as_ref().and_then(|w| eligible_headroom(w, now)) else {
            continue;
        };
        if best.is_none_or(|(_, top)| headroom > top) {
            best = Some((id, headroom));
        }
    }
    best.map(|(id, _)| id)
}

/// Load windows for each provider id. Off means no fetch at all: no HTTP and
/// no credential-file read, because `fetcher.windows` is never called.
pub async fn collect_windows(
    enabled: bool,
    fetcher: &super::QuotaFetcher,
    provider_ids: &[&str],
) -> HashMap<String, ProviderWindows> {
    let mut out = HashMap::new();
    if !enabled {
        return out;
    }

    for id in provider_ids {
        if let Some(windows) = fetcher.windows(id).await {
            out.insert((*id).to_string(), windows);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_provider::minimax::quota::Clock;
    use crate::agent_provider::quota::QuotaFetcher;
    use crate::agent_provider::quota::fetch::{HttpResponse, QuotaHttp};
    use crate::agent_provider::quota::types::{QuotaWindow, TransportError};
    use chrono::{Duration, TimeZone};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 17, 12, 0, 0).unwrap()
    }

    /// Five-hour window, `elapsed_pct` ~ 50% (2h30m left).
    fn on_pace_50(util: f64) -> ProviderWindows {
        ProviderWindows {
            five_hour: Some(QuotaWindow {
                utilization_pct: util,
                resets_at: now() + Duration::minutes(150),
            }),
            seven_day: None,
            seven_day_sonnet: None,
            stale: false,
        }
    }

    #[test]
    fn pick_chooses_most_headroom_among_on_track() {
        let claude = on_pace_50(20.0); // headroom 80
        let qwen = on_pace_50(60.0); // headroom 40
        let got = pick(&[("qwen", Some(qwen)), ("claude", Some(claude))], now());
        assert_eq!(got, Some("claude"));
    }

    #[test]
    fn pick_skips_ahead_and_critical_providers() {
        let claude = on_pace_50(90.0); // delta 40: critical
        let qwen = on_pace_50(60.0); // delta 10: ahead
        let minimax = on_pace_50(55.0); // delta 5: on track
        let got = pick(
            &[
                ("claude", Some(claude)),
                ("qwen", Some(qwen)),
                ("minimax", Some(minimax)),
            ],
            now(),
        );
        assert_eq!(got, Some("minimax"));
    }

    #[test]
    fn pick_never_prefers_unknown_provider_over_known_on_track() {
        let known = on_pace_50(40.0); // headroom 60, on track
        let got = pick(&[("claude", None), ("qwen", Some(known))], now());
        assert_eq!(got, Some("qwen"));
    }

    #[test]
    fn pick_returns_none_when_nothing_is_eligible() {
        let hot = on_pace_50(90.0);
        assert_eq!(pick(&[("claude", None), ("qwen", Some(hot))], now()), None);
        assert_eq!(pick(&[("claude", None)], now()), None);
        assert_eq!(pick(&[], now()), None);
    }

    #[derive(Debug)]
    struct FixedClock(DateTime<Utc>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    /// Counts requests. Returns one on-track five-hour window.
    #[derive(Clone, Default)]
    struct CountingHttp(Arc<AtomicUsize>);

    #[async_trait::async_trait]
    impl QuotaHttp for CountingHttp {
        async fn get_usage(&self, _token: &str) -> Result<HttpResponse, TransportError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(HttpResponse {
                status: 200,
                body: r#"{"five_hour":{"utilization":40.0,"resets_at":"2026-10-17T14:30:00Z"}}"#
                    .to_string(),
            })
        }
    }

    /// Home dir with a credential file, so a read would succeed if it happened.
    fn fetcher_with_token(
        calls: &CountingHttp,
    ) -> (tempfile::TempDir, tempfile::TempDir, QuotaFetcher) {
        let home = tempfile::tempdir().expect("home dir");
        let cache = tempfile::tempdir().expect("cache dir");
        let creds_dir = home.path().join(".claude");
        std::fs::create_dir_all(&creds_dir).expect("claude dir");
        std::fs::write(
            creds_dir.join(".credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"sk-test-fake"}}"#,
        )
        .expect("write creds");
        let fetcher = QuotaFetcher::new(
            home.path().to_path_buf(),
            cache.path().to_path_buf(),
            Box::new(calls.clone()),
            Box::new(FixedClock(now())),
        );
        (home, cache, fetcher)
    }

    #[tokio::test]
    async fn quota_off_makes_no_request() {
        let calls = CountingHttp::default();
        let (_home, _cache, fetcher) = fetcher_with_token(&calls);

        let out = collect_windows(false, &fetcher, &["claude"]).await;

        assert!(out.is_empty(), "off must return no windows");
        assert_eq!(calls.0.load(Ordering::SeqCst), 0, "off must not send HTTP");
    }

    #[tokio::test]
    async fn quota_on_fetches_claude_windows() {
        // Control: proves the off test above is not vacuous.
        let calls = CountingHttp::default();
        let (_home, _cache, fetcher) = fetcher_with_token(&calls);

        let out = collect_windows(true, &fetcher, &["claude"]).await;

        assert!(out.contains_key("claude"));
        assert_eq!(calls.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn pick_ignores_stale_windows_even_at_zero_percent() {
        // Stale 0% has the most headroom on paper, but it is old data.
        let mut stale = on_pace_50(0.0);
        stale.stale = true;
        let fresh = on_pace_50(40.0); // headroom 60, on track
        assert_eq!(
            pick(&[("claude", Some(stale)), ("qwen", Some(fresh))], now()),
            Some("qwen")
        );
    }

    #[test]
    fn pick_returns_none_when_every_window_is_stale() {
        let mut stale = on_pace_50(0.0);
        stale.stale = true;
        assert_eq!(pick(&[("claude", Some(stale))], now()), None);
    }

    #[test]
    fn pick_uses_binding_window_headroom() {
        // five-hour has 80% headroom, seven-day has only 10%: binding is 10%.
        let tight = ProviderWindows {
            five_hour: on_pace_50(20.0).five_hour,
            seven_day: Some(QuotaWindow {
                utilization_pct: 90.0,
                resets_at: now() + Duration::days(1), // 6 of 7 days elapsed: on track
            }),
            seven_day_sonnet: None,
            stale: false,
        };
        let roomy = on_pace_50(50.0); // headroom 50
        assert_eq!(
            pick(&[("claude", Some(tight)), ("qwen", Some(roomy))], now()),
            Some("qwen")
        );
    }
}
