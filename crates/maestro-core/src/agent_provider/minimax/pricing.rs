//! Per-model MiniMax pricing.
//!
//! MiniMax's free tier today reports `cost: 0` for chat calls. The pricing
//! shape is preserved so we don't introduce another abstraction when MiniMax
//! publishes paid-tier rates — only the numbers below need to change.
use crate::session::types::TokenUsage;

#[allow(dead_code)]
pub(crate) fn compute_cost(_model: &str, _usage: &TokenUsage) -> f64 {
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pricing_returns_zero_for_all_models_today() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        };
        assert_eq!(compute_cost("MiniMax-M2.7", &usage), 0.0);
        assert_eq!(compute_cost("anything-else", &usage), 0.0);
    }
}
