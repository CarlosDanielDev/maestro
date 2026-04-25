//! Topological sort of issues into dependency levels (#329).
//!
//! Pure function over `(node, blockers)` pairs. Used by both the roadmap
//! screen and the existing `milestone::update_milestone_dependency_graph`
//! call (which is currently dead code; the wiring into the milestone
//! screen is a one-liner in a follow-up commit).
//!
//! Why "levels" instead of a flat sort:
//!   - Level 0 = nodes with no blockers (can start now).
//!   - Level 1 = nodes whose blockers are all in Level 0.
//!   - …
//!
//! Within a level the order is stable (input order preserved), so the UI
//! can render `Level N (parallel)` blocks deterministically.

#![deny(clippy::unwrap_used)]
// Reason: Phase 1 foundation for #329. `dep_levels` is consumed by the
// roadmap loader and the dormant `milestone::update_milestone_dependency_graph`
// in Phase 2; tests cover topo-sort + cycle detection today.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

#[derive(Debug, PartialEq, Eq)]
pub enum DepLevelError {
    /// A cycle exists; the cycle members are returned (no specific order).
    Cycle { members: Vec<u64> },
    /// A node references a blocker that doesn't exist in the input set.
    /// The node and the unknown blocker number are returned.
    UnknownBlocker { node: u64, blocker: u64 },
}

impl std::fmt::Display for DepLevelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cycle { members } => {
                write!(f, "dependency cycle among nodes: {members:?}")
            }
            Self::UnknownBlocker { node, blocker } => {
                write!(f, "node #{node} references unknown blocker #{blocker}")
            }
        }
    }
}

impl std::error::Error for DepLevelError {}

/// Compute dependency levels.
///
/// `inputs[i].0` is the node identifier; `inputs[i].1` is its list of
/// blocker identifiers. Returns one `Vec<u64>` per level, in level order.
pub fn dep_levels(inputs: &[(u64, Vec<u64>)]) -> Result<Vec<Vec<u64>>, DepLevelError> {
    let known: HashSet<u64> = inputs.iter().map(|(n, _)| *n).collect();

    // Validate all referenced blockers exist.
    for (node, blockers) in inputs {
        for b in blockers {
            if !known.contains(b) {
                return Err(DepLevelError::UnknownBlocker {
                    node: *node,
                    blocker: *b,
                });
            }
        }
    }

    let mut remaining: HashMap<u64, HashSet<u64>> = inputs
        .iter()
        .map(|(n, bs)| (*n, bs.iter().copied().collect()))
        .collect();

    let mut levels: Vec<Vec<u64>> = Vec::new();

    while !remaining.is_empty() {
        // Stable input order at level construction: walk the ORIGINAL
        // input ordering and keep nodes still in `remaining` whose
        // blocker set is empty.
        let ready: Vec<u64> = inputs
            .iter()
            .map(|(n, _)| *n)
            .filter(|n| remaining.get(n).is_some_and(|bs| bs.is_empty()))
            .collect();

        if ready.is_empty() {
            // Anything left forms a cycle (or depends on a cycle).
            let mut members: Vec<u64> = remaining.keys().copied().collect();
            members.sort_unstable();
            return Err(DepLevelError::Cycle { members });
        }

        for n in &ready {
            remaining.remove(n);
        }
        for blockers in remaining.values_mut() {
            for n in &ready {
                blockers.remove(n);
            }
        }

        levels.push(ready);
    }

    Ok(levels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_empty() {
        let levels = dep_levels(&[]).expect("ok");
        assert!(levels.is_empty());
    }

    #[test]
    fn single_node_no_blockers_is_level_zero() {
        let levels = dep_levels(&[(1, vec![])]).expect("ok");
        assert_eq!(levels, vec![vec![1u64]]);
    }

    #[test]
    fn linear_chain_orders_each_node_in_its_level() {
        let levels = dep_levels(&[(3, vec![2]), (2, vec![1]), (1, vec![])]).expect("ok");
        assert_eq!(levels, vec![vec![1u64], vec![2], vec![3]]);
    }

    #[test]
    fn diamond_shares_level_for_parallel_branches() {
        // 4 depends on 2 and 3; 2 and 3 both depend on 1.
        let levels =
            dep_levels(&[(1, vec![]), (2, vec![1]), (3, vec![1]), (4, vec![2, 3])]).expect("ok");
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec![1u64]);
        assert!(levels[1].contains(&2));
        assert!(levels[1].contains(&3));
        assert_eq!(levels[2], vec![4u64]);
    }

    #[test]
    fn cycle_returns_cycle_error_with_members() {
        // 1 → 2 → 1
        let err = dep_levels(&[(1, vec![2]), (2, vec![1])]).unwrap_err();
        match err {
            DepLevelError::Cycle { members } => {
                assert_eq!(members, vec![1u64, 2]);
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    #[test]
    fn three_node_cycle_detected() {
        let err = dep_levels(&[(1, vec![3]), (2, vec![1]), (3, vec![2])]).unwrap_err();
        assert!(matches!(err, DepLevelError::Cycle { .. }));
    }

    #[test]
    fn unknown_blocker_returns_error() {
        let err = dep_levels(&[(1, vec![999])]).unwrap_err();
        match err {
            DepLevelError::UnknownBlocker { node, blocker } => {
                assert_eq!(node, 1);
                assert_eq!(blocker, 999);
            }
            other => panic!("expected UnknownBlocker, got {other:?}"),
        }
    }

    #[test]
    fn level_order_is_stable_within_level() {
        // Insertion order: 3 then 1 then 2; all on level 0.
        let levels = dep_levels(&[(3, vec![]), (1, vec![]), (2, vec![])]).expect("ok");
        assert_eq!(levels, vec![vec![3u64, 1, 2]]);
    }

    #[test]
    fn multiple_independent_chains_emit_max_depth_levels() {
        let levels = dep_levels(&[
            (10, vec![]),
            (11, vec![10]),
            (20, vec![]),
            (21, vec![20]),
            (22, vec![21]),
        ])
        .expect("ok");
        // Chain A: 10→11 (depth 2). Chain B: 20→21→22 (depth 3).
        assert_eq!(levels.len(), 3);
        assert!(levels[0].contains(&10) && levels[0].contains(&20));
        assert!(levels[1].contains(&11) && levels[1].contains(&21));
        assert_eq!(levels[2], vec![22u64]);
    }

    #[test]
    fn duplicate_blocker_does_not_double_count() {
        // Node 2 lists blocker 1 twice — should still resolve cleanly.
        let levels = dep_levels(&[(1, vec![]), (2, vec![1, 1])]).expect("ok");
        assert_eq!(levels, vec![vec![1u64], vec![2u64]]);
    }
}
