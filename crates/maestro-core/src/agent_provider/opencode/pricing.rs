//! Pricing fallback for the OpenCode parser.
//!
//! OpenCode's `step_finish` frame includes a `cost` field, but the live
//! telemetry reports it as `0` for many providers (free-tier, self-hosted,
//! OpenAI-compatible endpoints, opencode-go re-sale routes, etc.). When
//! `cost` is missing or zero this table computes a per-model fallback from
//! the four `TokenUsage` components so cost observability isn't silently
//! broken upstream.
//!
//! Rates are USD per million tokens. Cache-read and cache-write columns are
//! first-class fields so the table mirrors each upstream provider's
//! published pricing instead of hiding a constant multiplier inside the
//! computation. Unknown models return 0.0 (the existing behavior). Update
//! the table when OpenCode adds support for a new backend or when a
//! provider re-prices.

use crate::session::types::TokenUsage;

#[derive(Debug, Clone, Copy)]
struct OpenCodeModelPrice {
    input_per_mtok: f64,
    output_per_mtok: f64,
    cache_read_per_mtok: f64,
    cache_write_per_mtok: f64,
}

// OpenAI: cache reads at ~10% of input (gpt-5 family), ~50% of input
// (gpt-4o). Cache writes at the input rate.
const GPT_5_CODEX: OpenCodeModelPrice = OpenCodeModelPrice {
    input_per_mtok: 2.50,
    output_per_mtok: 10.00,
    cache_read_per_mtok: 0.25,
    cache_write_per_mtok: 2.50,
};

const GPT_5: OpenCodeModelPrice = OpenCodeModelPrice {
    input_per_mtok: 1.25,
    output_per_mtok: 10.00,
    cache_read_per_mtok: 0.125,
    cache_write_per_mtok: 1.25,
};

const GPT_4O: OpenCodeModelPrice = OpenCodeModelPrice {
    input_per_mtok: 2.50,
    output_per_mtok: 10.00,
    cache_read_per_mtok: 1.25,
    cache_write_per_mtok: 2.50,
};

const O3: OpenCodeModelPrice = OpenCodeModelPrice {
    input_per_mtok: 2.00,
    output_per_mtok: 8.00,
    cache_read_per_mtok: 0.50,
    cache_write_per_mtok: 2.00,
};

// Anthropic: cache reads at 10% input, cache writes at 125% input (matches
// `crate::agent_provider::claude_pricing`).
const CLAUDE_OPUS: OpenCodeModelPrice = OpenCodeModelPrice {
    input_per_mtok: 15.00,
    output_per_mtok: 75.00,
    cache_read_per_mtok: 1.50,
    cache_write_per_mtok: 18.75,
};

const CLAUDE_SONNET: OpenCodeModelPrice = OpenCodeModelPrice {
    input_per_mtok: 3.00,
    output_per_mtok: 15.00,
    cache_read_per_mtok: 0.30,
    cache_write_per_mtok: 3.75,
};

const CLAUDE_HAIKU: OpenCodeModelPrice = OpenCodeModelPrice {
    input_per_mtok: 0.80,
    output_per_mtok: 4.00,
    cache_read_per_mtok: 0.08,
    cache_write_per_mtok: 1.00,
};

// DeepSeek public rates (deepseek-chat / v3 / v3.1 / v4 family) per
// <https://api-docs.deepseek.com/quick_start/pricing>. opencode-go routes
// like `opencode-go/deepseek-v4-pro` resell the same backend — these rates
// are a best-effort floor; if the upstream invoice diverges, adjust the
// numbers here and re-run `cargo test --lib opencode::pricing`.
const DEEPSEEK: OpenCodeModelPrice = OpenCodeModelPrice {
    input_per_mtok: 0.27,
    output_per_mtok: 1.10,
    cache_read_per_mtok: 0.07,
    cache_write_per_mtok: 0.27,
};

fn price_for(model: &str) -> Option<OpenCodeModelPrice> {
    // Order matters: more specific patterns first.
    if model.contains("gpt-5-codex") {
        Some(GPT_5_CODEX)
    } else if model.contains("gpt-5") {
        Some(GPT_5)
    } else if model.contains("gpt-4o") {
        Some(GPT_4O)
    } else if model.contains("o3") {
        Some(O3)
    } else if model.contains("claude-opus") {
        Some(CLAUDE_OPUS)
    } else if model.contains("claude-sonnet") {
        Some(CLAUDE_SONNET)
    } else if model.contains("claude-haiku") {
        Some(CLAUDE_HAIKU)
    } else if model.contains("deepseek") {
        Some(DEEPSEEK)
    } else {
        None
    }
}

pub(crate) fn compute_cost(model: &str, usage: &TokenUsage) -> f64 {
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

    fn usage(input: u64, output: u64) -> TokenUsage {
        TokenUsage {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        }
    }

    fn usage_with_cache(input: u64, output: u64, cache_read: u64, cache_write: u64) -> TokenUsage {
        TokenUsage {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: cache_read,
            cache_creation_tokens: cache_write,
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

    #[test]
    fn deepseek_v4_pro_matches_public_pricing() {
        // opencode-go/deepseek-v4-pro resells the deepseek-chat tier.
        let cost = compute_cost("opencode-go/deepseek-v4-pro", &usage(1_000_000, 1_000_000));
        // 0.27 input + 1.10 output = 1.37
        assert!(approx_eq(cost, 1.37), "deepseek-v4-pro: {cost}");
    }

    #[test]
    fn deepseek_with_cache_read_picks_up_cache_rate() {
        // Real-world session: 132 input + 287 output + 96_400 cache_read.
        // Expected: 132 * 0.27 + 287 * 1.10 + 96_400 * 0.07 (per million).
        let cost = compute_cost(
            "opencode-go/deepseek-v4-pro",
            &usage_with_cache(132, 287, 96_400, 0),
        );
        let expected = (132.0 * 0.27 + 287.0 * 1.10 + 96_400.0 * 0.07) / 1_000_000.0;
        assert!(
            approx_eq(cost, expected),
            "deepseek cache-heavy: {cost} != {expected}"
        );
        // Sanity floor — must not be $0.00 once cached tokens are priced.
        assert!(cost > 0.0, "cache-heavy deepseek cost must be > 0");
    }

    #[test]
    fn claude_opus_cache_read_is_one_point_five_per_mtok() {
        let cost = compute_cost("claude-opus-4-7", &usage_with_cache(0, 0, 1_000_000, 0));
        assert!(approx_eq(cost, 1.50), "opus cache_read: {cost}");
    }

    #[test]
    fn sonnet_cache_write_is_three_seventy_five_per_mtok() {
        let cost = compute_cost("claude-sonnet-4-6", &usage_with_cache(0, 0, 0, 1_000_000));
        assert!(approx_eq(cost, 3.75), "sonnet cache_write: {cost}");
    }

    #[test]
    fn gpt_5_cache_read_is_one_eighth_input() {
        let cost = compute_cost("gpt-5", &usage_with_cache(0, 0, 1_000_000, 0));
        assert!(approx_eq(cost, 0.125), "gpt-5 cache_read: {cost}");
    }
}
