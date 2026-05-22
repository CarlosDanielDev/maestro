//! Per-model Codex pricing for cost computation in the Codex parser.
//!
//! Rates are USD per million tokens, sourced from the OpenAI pricing page.
//! Cache-read tokens (the "cached input" tier) are billed at the per-model
//! cached-input rate when one is published; for models without a cached-input
//! tier the rate falls back to the regular input rate.
//!
//! Updates: edit the per-model arms below and run
//! `cargo test --lib codex::pricing` to refresh the assertions.
use crate::session::types::TokenUsage;

#[derive(Debug, Clone, Copy)]
struct CodexModelPrice {
    input_per_mtok: f64,
    output_per_mtok: f64,
    cache_read_per_mtok: f64,
}

const GPT_5_CODEX: CodexModelPrice = CodexModelPrice {
    input_per_mtok: 2.50,
    output_per_mtok: 10.00,
    cache_read_per_mtok: 0.25,
};

const GPT_5: CodexModelPrice = CodexModelPrice {
    input_per_mtok: 1.25,
    output_per_mtok: 10.00,
    cache_read_per_mtok: 0.13,
};

const O3: CodexModelPrice = CodexModelPrice {
    input_per_mtok: 2.00,
    output_per_mtok: 8.00,
    cache_read_per_mtok: 0.50,
};

fn price_for(model: &str) -> Option<CodexModelPrice> {
    if model.contains("gpt-5-codex") || model.contains("gpt-5.4-codex") {
        Some(GPT_5_CODEX)
    } else if model.contains("gpt-5") {
        Some(GPT_5)
    } else if model.contains("o3") {
        Some(O3)
    } else {
        None
    }
}

/// USD cost for a single `usage` snapshot. Unknown models return 0.0.
pub(crate) fn compute_cost(model: &str, usage: &TokenUsage) -> f64 {
    let Some(p) = price_for(model) else {
        return 0.0;
    };
    let per_mtok = 1_000_000.0;
    (usage.input_tokens as f64 * p.input_per_mtok
        + usage.output_tokens as f64 * p.output_per_mtok
        + usage.cache_read_tokens as f64 * p.cache_read_per_mtok)
        / per_mtok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(input: u64, output: u64, cache_read: u64) -> TokenUsage {
        TokenUsage {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: cache_read,
            cache_creation_tokens: 0,
        }
    }

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn gpt_5_codex_pricing_matches_published_rates() {
        // 1M input + 1M output → 2.50 + 10.00 = $12.50.
        let cost = compute_cost("gpt-5-codex", &usage(1_000_000, 1_000_000, 0));
        assert!(approx_eq(cost, 12.50), "gpt-5-codex baseline: {cost}");
    }

    #[test]
    fn gpt_5_pricing_matches_published_rates() {
        // 1M input + 1M output → 1.25 + 10.00 = $11.25.
        let cost = compute_cost("gpt-5", &usage(1_000_000, 1_000_000, 0));
        assert!(approx_eq(cost, 11.25), "gpt-5 baseline: {cost}");
    }

    #[test]
    fn o3_pricing_matches_published_rates() {
        // 1M input + 1M output → 2.00 + 8.00 = $10.00.
        let cost = compute_cost("o3", &usage(1_000_000, 1_000_000, 0));
        assert!(approx_eq(cost, 10.00), "o3 baseline: {cost}");
    }

    #[test]
    fn cache_read_uses_published_cached_rate_for_gpt_5() {
        // 1M cache-read at gpt-5 cached rate → 0.13.
        let cost = compute_cost("gpt-5", &usage(0, 0, 1_000_000));
        assert!(approx_eq(cost, 0.13), "gpt-5 cache-read: {cost}");
    }

    #[test]
    fn unknown_model_returns_zero() {
        let cost = compute_cost("not-a-real-model", &usage(1_000_000, 1_000_000, 0));
        assert!(approx_eq(cost, 0.0));
    }

    #[test]
    fn zero_usage_returns_zero() {
        let cost = compute_cost("gpt-5-codex", &usage(0, 0, 0));
        assert!(approx_eq(cost, 0.0));
    }

    #[test]
    fn missing_usage_block_via_empty_struct_returns_zero() {
        // turn.completed with usage object that decoded to all zeros
        // must NOT divide-by-zero or panic.
        let cost = compute_cost(
            "gpt-5-codex",
            &TokenUsage {
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            },
        );
        assert!(approx_eq(cost, 0.0));
    }
}
