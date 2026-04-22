//! Token budget helper for text-preserving compression features.
//!
//! Used by fork-handoff, system-prompt, and knowledge compression to select
//! ranked segments while staying under a token limit. The first segment
//! always survives even if it alone exceeds the limit, so callers never
//! receive an empty selection when they provided at least one segment.

#[derive(Debug, Clone)]
pub struct TokenBudget {
    limit: u64,
}

#[derive(Debug, Clone)]
pub struct BudgetSelection {
    pub indices: Vec<usize>,
    #[allow(dead_code)] // Reason: surfaced by metrics callers and tests
    pub tokens_used: u64,
    pub truncated_first: bool,
}

impl TokenBudget {
    pub const fn new(limit: u64) -> Self {
        Self { limit }
    }

    pub fn select<F: Fn(usize) -> u64>(
        &self,
        ranked: &[(usize, f32)],
        token_cost: F,
    ) -> BudgetSelection {
        let mut picked = Vec::new();
        let mut used = 0u64;
        let mut truncated_first = false;
        for &(i, _) in ranked {
            let c = token_cost(i);
            if picked.is_empty() && c > self.limit {
                picked.push(i);
                used = c;
                truncated_first = true;
                break;
            }
            if used + c > self.limit {
                break;
            }
            picked.push(i);
            used += c;
        }
        BudgetSelection {
            indices: picked,
            tokens_used: used,
            truncated_first,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_ranked_returns_empty_selection() {
        let budget = TokenBudget::new(100);
        let sel = budget.select(&[], |_| 10);
        assert!(sel.indices.is_empty());
        assert_eq!(sel.tokens_used, 0);
        assert!(!sel.truncated_first);
    }

    #[test]
    fn single_oversized_segment_kept_with_truncated_flag() {
        let budget = TokenBudget::new(100);
        let ranked = vec![(0, 0.9), (1, 0.5)];
        let sel = budget.select(&ranked, |i| if i == 0 { 500 } else { 50 });
        assert_eq!(sel.indices, vec![0]);
        assert_eq!(sel.tokens_used, 500);
        assert!(sel.truncated_first);
    }

    #[test]
    fn select_stops_at_budget_boundary() {
        let budget = TokenBudget::new(100);
        let ranked = vec![(0, 0.9), (1, 0.8), (2, 0.7)];
        let sel = budget.select(&ranked, |_| 40);
        assert_eq!(sel.indices, vec![0, 1]);
        assert_eq!(sel.tokens_used, 80);
        assert!(!sel.truncated_first);
    }

    #[test]
    fn select_fills_exact_budget() {
        let budget = TokenBudget::new(90);
        let ranked = vec![(0, 0.9), (1, 0.8), (2, 0.7)];
        let sel = budget.select(&ranked, |_| 30);
        assert_eq!(sel.indices, vec![0, 1, 2]);
        assert_eq!(sel.tokens_used, 90);
    }

    #[test]
    fn select_preserves_ranking_order_in_indices() {
        let budget = TokenBudget::new(200);
        let ranked = vec![(3, 0.9), (1, 0.7), (2, 0.5)];
        let sel = budget.select(&ranked, |_| 50);
        assert_eq!(sel.indices, vec![3, 1, 2]);
    }
}
