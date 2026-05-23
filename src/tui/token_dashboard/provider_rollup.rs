//! Per-provider rollup view-model for the token dashboard (#776/#848).
//!
//! Pure functions only — no rendering, no I/O. The TUI rollup widget
//! (#849) consumes `ProviderRow` values produced here. Tests do not
//! require a terminal.
//!
//! Sanitization (`sanitize_cost`, `sanitize_pct`) drops NaN / negative /
//! Infinity values to safe defaults at view-model assembly so the render
//! layer in #849 never sees them.

use std::collections::BTreeMap;

use crate::budget::quota_snapshot::{ProviderQuotaSnapshots, QuotaRow};
use crate::session::types::Session;

/// One row in the per-provider rollup table.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderRow {
    pub provider_id: String,
    pub display_name: String,
    pub total_cost_usd: f64,
    pub context_used: u64,
    pub context_window: Option<u32>,
    pub quota: Option<QuotaRow>,
    pub session_count: usize,
}

/// Build one `ProviderRow` per distinct `session.agent_id`. Sessions without
/// an `agent_id` collapse into a single `"unknown"` bucket. Cost values are
/// sanitized (NaN / negative / Infinity → 0.0) at sum-time so the renderer
/// in #849 never sees non-finite values.
///
/// `window_lookup` returns the per-provider context-window denominator (e.g.
/// 200_000 for Claude 4.x). The first session's `model` field is passed to
/// the lookup; downstream models inside the same provider group are ignored
/// since the cap is per-provider not per-model in practice.
///
/// Rows are returned in alphabetical order by `provider_id` for deterministic
/// rendering and stable snapshots in #849.
pub fn build_provider_rows<F>(
    sessions: &[&Session],
    quota_snapshots: &dyn ProviderQuotaSnapshots,
    window_lookup: F,
) -> Vec<ProviderRow>
where
    F: Fn(&str, &str) -> Option<u32>,
{
    let mut groups: BTreeMap<String, Vec<&Session>> = BTreeMap::new();
    for s in sessions {
        let key = s.agent_id.as_deref().unwrap_or("unknown").to_string();
        groups.entry(key).or_default().push(s);
    }

    groups
        .into_iter()
        .map(|(provider_id, group)| {
            let mut total_cost_usd = 0.0_f64;
            let mut context_used: u64 = 0;
            for s in &group {
                total_cost_usd += sanitize_cost(s.cost_usd);
                context_used = context_used
                    .saturating_add(s.token_usage.input_tokens)
                    .saturating_add(s.token_usage.cache_read_tokens);
            }
            let model = group.first().map(|s| s.model.as_str()).unwrap_or("");
            let context_window = window_lookup(&provider_id, model);
            let quota = quota_snapshots.quota_for(&provider_id);
            let display_name = display_name_for(&provider_id);
            let session_count = group.len();
            ProviderRow {
                provider_id,
                display_name,
                total_cost_usd,
                context_used,
                context_window,
                quota,
                session_count,
            }
        })
        .collect()
}

/// Per-provider context-window constant. Single-source so #849's renderer
/// never sprinkles magic numbers. Unknown providers return `None` — the
/// renderer omits the denominator.
pub fn provider_context_window(provider_id: &str, _model: &str) -> Option<u32> {
    match provider_id {
        "claude" => Some(200_000),
        "minimax" => Some(204_800),
        _ => None,
    }
}

/// Clamp cost values to a sane range: NaN / Infinity / negative → 0.0.
/// Applied at view-model assembly time, NOT inside the render layer.
pub fn sanitize_cost(cost: f64) -> f64 {
    if !cost.is_finite() || cost.is_sign_negative() {
        0.0
    } else {
        cost
    }
}

/// Clamp percent values to `[0.0, 1.0]`. NaN / negative → 0.0; >1.0 → 1.0.
/// Applied at view-model assembly time so the render layer never receives
/// values that would underflow gauge math.
pub fn sanitize_pct(pct: f64) -> f64 {
    if pct.is_nan() || pct < 0.0 {
        0.0
    } else if pct > 1.0 {
        1.0
    } else {
        pct
    }
}

fn display_name_for(provider_id: &str) -> String {
    let mut chars = provider_id.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::quota_snapshot::{QuotaBucket, QuotaRow};
    use crate::session::types::Session;
    use std::collections::HashMap;

    struct FakeProviderQuotaSnapshots {
        entries: HashMap<String, QuotaRow>,
    }

    impl FakeProviderQuotaSnapshots {
        fn new() -> Self {
            Self {
                entries: HashMap::new(),
            }
        }

        fn with(mut self, provider_id: &str, row: QuotaRow) -> Self {
            self.entries.insert(provider_id.to_string(), row);
            self
        }
    }

    impl ProviderQuotaSnapshots for FakeProviderQuotaSnapshots {
        fn quota_for(&self, provider_id: &str) -> Option<QuotaRow> {
            self.entries.get(provider_id).copied()
        }
    }

    fn make_session(agent_id: Option<&str>, cost: f64, model: &str) -> Session {
        let mut s = Session::new(
            "test".to_string(),
            model.to_string(),
            "orchestrator".to_string(),
            None,
            None,
        );
        s.agent_id = agent_id.map(|x| x.to_string());
        s.cost_usd = cost;
        s
    }

    // ── Seam 1: build_provider_rows ──────────────────────────────────────

    #[test]
    fn build_provider_rows_groups_sessions_by_agent_id() {
        let s1 = make_session(Some("claude"), 0.10, "claude-opus-4-5");
        let s2 = make_session(Some("claude"), 0.20, "claude-opus-4-5");
        let s3 = make_session(Some("minimax"), 0.05, "MiniMax-M1");
        let sessions = [&s1, &s2, &s3];
        let fake = FakeProviderQuotaSnapshots::new();
        let rows = build_provider_rows(&sessions, &fake, provider_context_window);
        assert_eq!(rows.len(), 2);
        let claude = rows.iter().find(|r| r.provider_id == "claude").unwrap();
        assert!((claude.total_cost_usd - 0.30).abs() < f64::EPSILON);
        assert_eq!(claude.session_count, 2);
        let minimax = rows.iter().find(|r| r.provider_id == "minimax").unwrap();
        assert!((minimax.total_cost_usd - 0.05).abs() < f64::EPSILON);
        assert_eq!(minimax.session_count, 1);
    }

    #[test]
    fn build_provider_rows_missing_agent_id_falls_into_unknown() {
        let s = make_session(None, 0.07, "claude-opus-4-5");
        let fake = FakeProviderQuotaSnapshots::new();
        let rows = build_provider_rows(&[&s], &fake, provider_context_window);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider_id, "unknown");
        assert!((rows[0].total_cost_usd - 0.07).abs() < f64::EPSILON);
    }

    #[test]
    fn build_provider_rows_multiple_sessions_same_provider_sums_cost() {
        let s1 = make_session(Some("claude"), 0.10, "claude-opus-4-5");
        let s2 = make_session(Some("claude"), 0.15, "claude-opus-4-5");
        let s3 = make_session(Some("claude"), 0.25, "claude-opus-4-5");
        let fake = FakeProviderQuotaSnapshots::new();
        let rows = build_provider_rows(&[&s1, &s2, &s3], &fake, provider_context_window);
        assert_eq!(rows.len(), 1);
        assert!((rows[0].total_cost_usd - 0.50).abs() < f64::EPSILON);
        assert_eq!(rows[0].session_count, 3);
    }

    #[test]
    fn build_provider_rows_zero_sessions_returns_empty() {
        let fake = FakeProviderQuotaSnapshots::new();
        let rows = build_provider_rows(&[], &fake, provider_context_window);
        assert!(rows.is_empty());
    }

    #[test]
    fn build_provider_rows_quota_only_set_for_minimax() {
        let s_claude = make_session(Some("claude"), 0.10, "claude-opus-4-5");
        let s_minimax = make_session(Some("minimax"), 0.05, "MiniMax-M1");
        let row = QuotaRow {
            used: 247,
            limit: 4500,
            window_label: "5h",
            status: QuotaBucket::Warn,
        };
        let fake = FakeProviderQuotaSnapshots::new().with("minimax", row);
        let rows = build_provider_rows(&[&s_claude, &s_minimax], &fake, provider_context_window);
        let claude = rows.iter().find(|r| r.provider_id == "claude").unwrap();
        let minimax = rows.iter().find(|r| r.provider_id == "minimax").unwrap();
        assert!(claude.quota.is_none());
        assert_eq!(minimax.quota, Some(row));
    }

    #[test]
    fn build_provider_rows_nan_cost_sanitized_to_zero() {
        let s = make_session(Some("claude"), f64::NAN, "claude-opus-4-5");
        let fake = FakeProviderQuotaSnapshots::new();
        let rows = build_provider_rows(&[&s], &fake, provider_context_window);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].total_cost_usd, 0.0);
    }

    #[test]
    fn build_provider_rows_infinity_cost_sanitized() {
        let s = make_session(Some("claude"), f64::INFINITY, "claude-opus-4-5");
        let fake = FakeProviderQuotaSnapshots::new();
        let rows = build_provider_rows(&[&s], &fake, provider_context_window);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].total_cost_usd.is_finite());
        assert_eq!(rows[0].total_cost_usd, 0.0);
    }

    #[test]
    fn build_provider_rows_context_pct_nan_does_not_propagate_to_row() {
        // ProviderRow does not carry context_pct directly; this test just
        // proves a NaN context_pct on a session does not panic when building
        // rows (the renderer in #849 will use sanitize_pct on the value it
        // ultimately renders).
        let mut s = make_session(Some("minimax"), 0.02, "MiniMax-M1");
        s.context_pct = f64::NAN;
        let fake = FakeProviderQuotaSnapshots::new();
        let rows = build_provider_rows(&[&s], &fake, provider_context_window);
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn build_provider_rows_context_window_populated_for_claude() {
        let mut s = make_session(Some("claude"), 0.10, "claude-opus-4-5");
        s.token_usage.input_tokens = 10_000;
        s.token_usage.cache_read_tokens = 5_000;
        let fake = FakeProviderQuotaSnapshots::new();
        let rows = build_provider_rows(&[&s], &fake, provider_context_window);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].context_window, Some(200_000));
        assert_eq!(rows[0].context_used, 15_000);
    }

    #[test]
    fn build_provider_rows_single_session_single_provider() {
        let s = make_session(Some("ollama"), 0.00, "llama3.2");
        let fake = FakeProviderQuotaSnapshots::new();
        let rows = build_provider_rows(&[&s], &fake, provider_context_window);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider_id, "ollama");
        assert_eq!(rows[0].total_cost_usd, 0.0);
        assert_eq!(rows[0].session_count, 1);
    }

    // ── Seam 2: provider_context_window ───────────────────────────────────

    #[test]
    fn provider_context_window_claude_returns_200k() {
        assert_eq!(
            provider_context_window("claude", "claude-opus-4-5"),
            Some(200_000)
        );
    }

    #[test]
    fn provider_context_window_minimax_returns_204800() {
        assert_eq!(
            provider_context_window("minimax", "MiniMax-M1"),
            Some(204_800)
        );
    }

    #[test]
    fn provider_context_window_ollama_returns_none() {
        assert_eq!(provider_context_window("ollama", "llama3.2"), None);
    }

    #[test]
    fn provider_context_window_unknown_provider_returns_none() {
        assert_eq!(provider_context_window("nonexistent", "model-x"), None);
    }

    // ── Seam 3: sanitize_cost / sanitize_pct ──────────────────────────────

    #[test]
    fn sanitize_cost_nan_returns_zero() {
        assert_eq!(sanitize_cost(f64::NAN), 0.0);
    }

    #[test]
    fn sanitize_cost_negative_returns_zero() {
        assert_eq!(sanitize_cost(-1.5), 0.0);
    }

    #[test]
    fn sanitize_cost_normal_passes_through() {
        assert_eq!(sanitize_cost(0.42), 0.42);
    }

    #[test]
    fn sanitize_cost_zero_passes_through() {
        assert_eq!(sanitize_cost(0.0), 0.0);
    }

    #[test]
    fn sanitize_cost_infinity_returns_zero() {
        assert_eq!(sanitize_cost(f64::INFINITY), 0.0);
    }

    #[test]
    fn sanitize_pct_nan_returns_zero() {
        assert_eq!(sanitize_pct(f64::NAN), 0.0);
    }

    #[test]
    fn sanitize_pct_negative_returns_zero() {
        assert_eq!(sanitize_pct(-0.5), 0.0);
    }

    #[test]
    fn sanitize_pct_above_one_clamps_to_one() {
        assert_eq!(sanitize_pct(1.5), 1.0);
    }

    #[test]
    fn sanitize_pct_normal_passes_through() {
        assert_eq!(sanitize_pct(0.75), 0.75);
    }

    #[test]
    fn sanitize_pct_exactly_one_passes_through() {
        assert_eq!(sanitize_pct(1.0), 1.0);
    }
}
