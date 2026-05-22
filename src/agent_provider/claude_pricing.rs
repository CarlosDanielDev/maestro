//! Per-model Anthropic pricing for Claude provider cost computation.
//!
//! Rates are USD per million tokens, sourced from <https://www.anthropic.com/pricing>.
//! Cache-read tokens are billed at 10% of the input rate per Anthropic's
//! prompt-caching discount; cache-write tokens are billed at 125% of the input
//! rate. The acceptance criteria for #771 require the 10% cache-read multiplier
//! but the type carries the cache-write rate as a first-class field so the
//! pricing schema mirrors Anthropic's published table instead of hiding a
//! constant multiplier inside the computation.
//!
//! Updates: edit the per-model constants below + run
//! `cargo test --lib claude_pricing` to refresh the assertions.
use crate::session::types::TokenUsage;

#[derive(Debug, Clone, Copy)]
struct ClaudeModelPrice {
    input_per_mtok: f64,
    output_per_mtok: f64,
    cache_read_per_mtok: f64,
    cache_write_per_mtok: f64,
}

const OPUS_4_X: ClaudeModelPrice = ClaudeModelPrice {
    input_per_mtok: 15.0,
    output_per_mtok: 75.0,
    cache_read_per_mtok: 1.50,
    cache_write_per_mtok: 18.75,
};

const SONNET_4_X: ClaudeModelPrice = ClaudeModelPrice {
    input_per_mtok: 3.0,
    output_per_mtok: 15.0,
    cache_read_per_mtok: 0.30,
    cache_write_per_mtok: 3.75,
};

const HAIKU_4_X: ClaudeModelPrice = ClaudeModelPrice {
    input_per_mtok: 0.80,
    output_per_mtok: 4.0,
    cache_read_per_mtok: 0.08,
    cache_write_per_mtok: 1.00,
};

fn price_for(model: &str) -> Option<ClaudeModelPrice> {
    if model.contains("opus") {
        Some(OPUS_4_X)
    } else if model.contains("sonnet") {
        Some(SONNET_4_X)
    } else if model.contains("haiku") {
        Some(HAIKU_4_X)
    } else {
        None
    }
}

/// USD cost for a single `usage` snapshot. Unknown models return 0.0.
pub fn compute_cost(model: &str, usage: &TokenUsage) -> f64 {
    let Some(p) = price_for(model) else {
        return 0.0;
    };
    let per_mtok = 1_000_000.0;
    (usage.input_tokens as f64 * p.input_per_mtok
        + usage.output_tokens as f64 * p.output_per_mtok
        + usage.cache_read_tokens as f64 * p.cache_read_per_mtok
        + usage.cache_creation_tokens as f64 * p.cache_write_per_mtok)
        / per_mtok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(input: u64, output: u64, cache_read: u64, cache_create: u64) -> TokenUsage {
        TokenUsage {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: cache_read,
            cache_creation_tokens: cache_create,
        }
    }

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn opus_pricing_matches_published_rates() {
        // 1M input + 1M output → 15 + 75 = $90 for Opus 4.x.
        let cost = compute_cost("claude-opus-4-7", &usage(1_000_000, 1_000_000, 0, 0));
        assert!(approx_eq(cost, 90.0), "opus baseline cost: {cost}");
    }

    #[test]
    fn sonnet_pricing_matches_published_rates() {
        // 1M input + 1M output → 3 + 15 = $18 for Sonnet 4.x.
        let cost = compute_cost("claude-sonnet-4-6", &usage(1_000_000, 1_000_000, 0, 0));
        assert!(approx_eq(cost, 18.0), "sonnet baseline cost: {cost}");
    }

    #[test]
    fn haiku_pricing_matches_published_rates() {
        // 1M input + 1M output → 0.8 + 4.0 = $4.80 for Haiku 4.x.
        let cost = compute_cost(
            "claude-haiku-4-5-20251001",
            &usage(1_000_000, 1_000_000, 0, 0),
        );
        assert!(approx_eq(cost, 4.80), "haiku baseline cost: {cost}");
    }

    #[test]
    fn cache_read_uses_10pct_of_input_rate_for_sonnet() {
        // 1M cache-read tokens at Sonnet rate → 0.30 = 10% of 3.00.
        let cost = compute_cost("claude-sonnet-4-6", &usage(0, 0, 1_000_000, 0));
        assert!(
            approx_eq(cost, 0.30),
            "sonnet cache-read should be 10% of input: {cost}"
        );
    }

    #[test]
    fn cache_read_uses_10pct_of_input_rate_for_opus() {
        // 1M cache-read tokens at Opus rate → 1.50 = 10% of 15.00.
        let cost = compute_cost("claude-opus-4-7", &usage(0, 0, 1_000_000, 0));
        assert!(
            approx_eq(cost, 1.50),
            "opus cache-read should be 10% of input: {cost}"
        );
    }

    #[test]
    fn cache_write_uses_125pct_of_input_rate_for_sonnet() {
        // 1M cache-write tokens at Sonnet rate → 3.75 = 125% of 3.00.
        let cost = compute_cost("claude-sonnet-4-6", &usage(0, 0, 0, 1_000_000));
        assert!(
            approx_eq(cost, 3.75),
            "sonnet cache-write should be 125% of input: {cost}"
        );
    }

    #[test]
    fn unknown_model_returns_zero() {
        let cost = compute_cost("gpt-9000-imaginary", &usage(1_000_000, 1_000_000, 0, 0));
        assert!(approx_eq(cost, 0.0));
    }

    #[test]
    fn zero_usage_returns_zero() {
        let cost = compute_cost("claude-sonnet-4-6", &usage(0, 0, 0, 0));
        assert!(approx_eq(cost, 0.0));
    }

    #[test]
    fn fixture_transcript_recorded_assistant_frame_costs() {
        // Recorded numbers from a real Sonnet 4.6 turn:
        //   cache_creation=1_000, cache_read=5_000, input=200, output=400.
        //
        //   input         200    × 3.00  / 1e6 = 0.0006
        //   output        400    × 15.00 / 1e6 = 0.006
        //   cache_read   5_000   × 0.30  / 1e6 = 0.0015
        //   cache_write  1_000   × 3.75  / 1e6 = 0.00375
        //   total                              = 0.01185
        let cost = compute_cost(
            "claude-sonnet-4-6",
            &TokenUsage {
                input_tokens: 200,
                output_tokens: 400,
                cache_read_tokens: 5_000,
                cache_creation_tokens: 1_000,
            },
        );
        assert!(
            approx_eq(cost, 0.011_85),
            "fixture cost mismatch: {cost} (expected 0.01185)"
        );
    }
}
