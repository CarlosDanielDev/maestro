//! Pricing fallback for the OpenCode parser.
//!
//! OpenCode's `step_finish` frame includes a `cost` field, but the live
//! telemetry reports it as `0` for many providers (free-tier, self-hosted,
//! OpenAI-compatible endpoints, etc.). When `cost` is missing or zero this
//! table computes a per-model fallback from `tokens.input` + `tokens.output`
//! so cost observability isn't silently broken upstream.
//!
//! Rates are USD per million tokens. Unknown models return 0.0 (the existing
//! behavior). Update the table when OpenCode adds support for a new backend.
use crate::session::types::TokenUsage;

pub(crate) fn compute_cost(model: &str, usage: &TokenUsage) -> f64 {
    let (input_per_mtok, output_per_mtok) = match model {
        m if m.contains("gpt-5-codex") => (2.50, 10.00),
        m if m.contains("gpt-5") => (1.25, 10.00),
        m if m.contains("gpt-4o") => (2.50, 10.00),
        m if m.contains("o3") => (2.00, 8.00),
        m if m.contains("claude-opus") => (15.00, 75.00),
        m if m.contains("claude-sonnet") => (3.00, 15.00),
        m if m.contains("claude-haiku") => (0.80, 4.00),
        _ => return 0.0,
    };
    let per_mtok = 1_000_000.0;
    (usage.input_tokens as f64 * input_per_mtok + usage.output_tokens as f64 * output_per_mtok)
        / per_mtok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(input: u64, output: u64) -> TokenUsage {
        TokenUsage {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        }
    }

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn gpt_5_codex_fallback_matches_pricing_table() {
        let cost = compute_cost("gpt-5-codex", &usage(1_000_000, 1_000_000));
        assert!(approx_eq(cost, 12.50), "gpt-5-codex: {cost}");
    }

    #[test]
    fn sonnet_fallback_matches_pricing_table() {
        let cost = compute_cost("claude-sonnet-4-6", &usage(1_000_000, 1_000_000));
        assert!(approx_eq(cost, 18.00), "sonnet: {cost}");
    }

    #[test]
    fn unknown_model_returns_zero() {
        let cost = compute_cost("future-model", &usage(1_000_000, 1_000_000));
        assert!(approx_eq(cost, 0.0));
    }

    #[test]
    fn zero_tokens_returns_zero() {
        let cost = compute_cost("gpt-5", &usage(0, 0));
        assert!(approx_eq(cost, 0.0));
    }
}
