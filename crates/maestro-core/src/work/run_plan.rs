//! One pure plan for a run: which issues are ready, in which waves they
//! launch, which are held back and why, and the worst-case spend.
//!
//! Every consumer (Milestones ready counts, Roadmap levels, the Run queue
//! screen) reads this instead of keeping its own planner. No I/O.

use crate::orchestration::dag::{Edge, IssueMeta, classify_edges, topo_levels};
use crate::state::types::IssueNumber;
use anyhow::Result;
use std::collections::{HashMap, HashSet};

/// Why an open issue is not in any wave.
#[derive(Debug, Clone, PartialEq)]
pub enum Hold {
    /// Another session already claims it; the user must decide.
    Claimed,
    /// Blocked by an open issue outside the planned set.
    BlockedOutside {
        by: IssueNumber,
        milestone: Option<u64>,
    },
    /// Waits (inside the set) on an issue that is itself held.
    WaitsOnDeferred(IssueNumber),
    /// Open, but `metas` has no entry for it (partial fetch).
    NoMetadata,
}

/// The result of [`run_plan`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunPlan {
    /// Launch order. Wave n+1 starts as wave n frees slots.
    pub waves: Vec<Vec<IssueNumber>>,
    /// Every held issue with its reasons, sorted by issue number.
    pub deferred: Vec<(IssueNumber, Vec<Hold>)>,
    /// Claimed issues the user must take over or skip before launch.
    pub needs_decision: Vec<IssueNumber>,
    /// `(issue, after, shared file)`: issue pushed to a later wave because it
    /// edits a file an earlier issue in its wave also edits.
    pub moved: Vec<(IssueNumber, IssueNumber, String)>,
    /// Runnable issues with no known file list; overlap cannot be checked.
    pub unknown_scope: Vec<IssueNumber>,
    /// Issues in waves times the per-session cap.
    pub worst_case_usd: f64,
}

/// Plan a run over the `open` issues.
///
/// - `metas`: state, milestone and blocked-by of every known issue; closed
///   blockers count as satisfied.
/// - `claimed`: issues another session already works on.
/// - `files`: each issue's file list (`None` = unknown scope).
/// - `per_session_cap_usd`: the budget cap of one session.
///
/// Errors only on a dependency cycle inside `open` (held members included);
/// the error names the cycle. An open issue missing from `metas` is held with
/// [`Hold::NoMetadata`], never a panic.
pub fn run_plan(
    open: &HashSet<IssueNumber>,
    metas: &HashMap<IssueNumber, IssueMeta>,
    claimed: &HashSet<IssueNumber>,
    files: &HashMap<IssueNumber, Option<Vec<String>>>,
    per_session_cap_usd: f64,
) -> Result<RunPlan> {
    // classify_edges skips open issues with no meta; give them no edges so
    // indexing never panics, and hold them below.
    let mut edges = classify_edges(open, None, metas);
    for &n in open {
        edges.entry(n).or_default();
    }
    // Report a cycle anywhere in `open`, even one a hold would hide.
    topo_levels(open, &edges)?;

    let mut order: Vec<IssueNumber> = open.iter().copied().collect();
    order.sort_unstable();

    let mut held: HashMap<IssueNumber, Vec<Hold>> = HashMap::new();
    for &n in &order {
        let mut reasons = Vec::new();
        if !metas.contains_key(&n) {
            reasons.push(Hold::NoMetadata);
        }
        let outside = edges[&n].iter().find_map(|e| match e {
            Edge::SameMilestoneOpenExternal(by) | Edge::CrossMilestoneOpenExternal(by) => Some(*by),
            _ => None,
        });
        if let Some(by) = outside {
            let milestone = metas.get(&by).and_then(|m| m.milestone);
            reasons.push(Hold::BlockedOutside { by, milestone });
        }
        if claimed.contains(&n) {
            reasons.push(Hold::Claimed);
        }
        if !reasons.is_empty() {
            held.insert(n, reasons);
        }
    }
    // Anything that waits on a held issue is held too, to a fixed point.
    loop {
        let before = held.len();
        for &n in &order {
            if held.contains_key(&n) {
                continue;
            }
            let dep = edges[&n].iter().find_map(|e| match e {
                Edge::InSlice(d) if held.contains_key(d) => Some(*d),
                _ => None,
            });
            if let Some(d) = dep {
                held.insert(n, vec![Hold::WaitsOnDeferred(d)]);
            }
        }
        if held.len() == before {
            break;
        }
    }

    let runnable: HashSet<IssueNumber> = open
        .iter()
        .copied()
        .filter(|n| !held.contains_key(n))
        .collect();
    let mut waves = topo_levels(&runnable, &edges)?;
    let moved = split_same_wave_overlaps(&mut waves, &edges, files);

    let mut unknown_scope: Vec<IssueNumber> = runnable
        .iter()
        .copied()
        .filter(|n| matches!(files.get(n), None | Some(None)))
        .collect();
    unknown_scope.sort_unstable();

    let mut deferred: Vec<(IssueNumber, Vec<Hold>)> = held.into_iter().collect();
    deferred.sort_unstable_by_key(|(n, _)| *n);
    let needs_decision = deferred
        .iter()
        .filter(|(_, h)| h.contains(&Hold::Claimed))
        .map(|(n, _)| *n)
        .collect();

    Ok(RunPlan {
        worst_case_usd: runnable.len() as f64 * per_session_cap_usd,
        waves,
        deferred,
        needs_decision,
        moved,
        unknown_scope,
    })
}

/// Push an issue to the next wave when it edits a file an earlier issue in
/// the same wave edits, or when one of its blockers was pushed into its wave.
/// Waves are walked in order, so a push cascades to dependents.
fn split_same_wave_overlaps(
    waves: &mut Vec<Vec<IssueNumber>>,
    edges: &HashMap<IssueNumber, Vec<Edge>>,
    files: &HashMap<IssueNumber, Option<Vec<String>>>,
) -> Vec<(IssueNumber, IssueNumber, String)> {
    let mut moved = Vec::new();
    let mut placed: HashSet<IssueNumber> = HashSet::new();
    let mut i = 0;
    while i < waves.len() {
        let mut wave = std::mem::take(&mut waves[i]);
        wave.sort_unstable();
        let mut keep = Vec::new();
        let mut bump = Vec::new();
        let mut owner: HashMap<&str, IssueNumber> = HashMap::new();
        for n in wave {
            let blocked = edges[&n]
                .iter()
                .any(|e| matches!(e, Edge::InSlice(d) if !placed.contains(d)));
            if blocked {
                bump.push(n);
                continue;
            }
            let own: &[String] = files.get(&n).and_then(|f| f.as_deref()).unwrap_or(&[]);
            if let Some(f) = own.iter().find(|f| owner.contains_key(f.as_str())) {
                moved.push((n, owner[f.as_str()], f.clone()));
                bump.push(n);
                continue;
            }
            for f in own {
                owner.insert(f, n);
            }
            keep.push(n);
        }
        placed.extend(keep.iter().copied());
        waves[i] = keep;
        if !bump.is_empty() {
            match waves.get_mut(i + 1) {
                Some(next) => next.extend(bump),
                None => waves.push(bump),
            }
        }
        i += 1;
    }
    moved
}

#[cfg(test)]
#[path = "run_plan_tests.rs"]
mod tests;
