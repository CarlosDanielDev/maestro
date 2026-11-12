//! `maestro clean --stale` (#1149): remove the worktrees of stale sessions
//! and close the sessions out. Split from `clean.rs` to honor the
//! 400-line file cap; tests live in `clean_stale_tests.rs`.

use crate::session::cleanup::{CleanupManager, OrphanWorktree};
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus};
use crate::state::store::StateStore;
use crate::state::types::MaestroState;
use std::path::Path;

/// Outcome of one `maestro clean --stale` pass (#1149). Returned so tests
/// can assert on data instead of stdout.
#[derive(Debug)]
pub struct StaleCleanReport {
    /// All stale sessions in the state file, with or without a worktree.
    pub stale_total: usize,
    /// Safe candidates: stale worktrees whose slug no live session owns.
    pub found: Vec<OrphanWorktree>,
    /// Candidates refused because a live session now owns that slug —
    /// listed, never removed.
    pub conflicts: Vec<OrphanWorktree>,
    pub removed: usize,
    /// Names of worktrees whose wipe failed; their sessions stay stale
    /// for retry instead of being marked `Killed` blind.
    pub failed: Vec<String>,
    pub marked_killed: usize,
}

/// Move stale sessions (#1149) to the terminal `Killed` state.
/// User-initiated: `clean --stale` is the explicit act that closes them —
/// `status` never mutates the state file on read. `Killed` is a valid
/// target from every spawn-bearing status, so the transition always
/// succeeds for sessions the staleness predicate selected. `keep_stale`
/// lets the caller hold a session open for retry (its worktree wipe
/// failed) instead of closing it blind.
pub fn mark_stale_sessions_killed(
    state: &mut MaestroState,
    keep_stale: impl Fn(&Session) -> bool,
) -> usize {
    let mut marked = 0;
    for session in &mut state.sessions {
        if session.is_stale()
            && !keep_stale(session)
            && session
                .transition_to(SessionStatus::Killed, TransitionReason::UserKill)
                .is_ok()
        {
            marked += 1;
        }
    }
    marked
}

/// `maestro clean --stale` (#1149): list and remove the worktrees of stale
/// sessions (honoring `dry_run`), then close the stale sessions out as
/// `Killed` in the state file. Conflicted slugs (owned by a live session)
/// are never removed; sessions whose wipe failed stay stale for retry;
/// sessions without a worktree are still marked — there is simply nothing
/// to remove for them.
pub fn clean_stale(
    repo_root: &Path,
    state_path: &Path,
    dry_run: bool,
) -> anyhow::Result<StaleCleanReport> {
    let mgr = CleanupManager::new(repo_root);
    let store = StateStore::new(state_path.to_path_buf());
    let mut state = store.load()?;
    let stale_total = state.sessions.iter().filter(|s| s.is_stale()).count();
    let scan = mgr.stale_worktrees(&state);

    if dry_run {
        return Ok(StaleCleanReport {
            stale_total,
            found: scan.safe,
            conflicts: scan.conflicts,
            removed: 0,
            failed: Vec::new(),
            marked_killed: 0,
        });
    }

    let mut removed = 0;
    let mut failed = Vec::new();
    for orphan in &scan.safe {
        match mgr.remove_orphan(orphan) {
            Ok(()) => removed += 1,
            Err(e) => {
                tracing::warn!(
                    "stale worktree teardown refused/failed for {}: {}",
                    orphan.name,
                    e
                );
                failed.push(orphan.name.clone());
            }
        }
    }

    let marked_killed = mark_stale_sessions_killed(&mut state, |s| {
        match mgr.stale_candidate_path(s) {
            // Wipe failed for this session's worktree: hold it open for
            // retry instead of closing it blind.
            Some((_, name)) => failed.contains(&name),
            None => false,
        }
    });
    store.save(&state)?;

    Ok(StaleCleanReport {
        stale_total,
        found: scan.safe,
        conflicts: scan.conflicts,
        removed,
        failed,
        marked_killed,
    })
}

pub(crate) fn print_stale_clean_summary(report: &StaleCleanReport, dry_run: bool) {
    if report.stale_total == 0 {
        println!("No stale sessions found.");
        return;
    }

    if !report.found.is_empty() {
        println!("Found {} stale session worktree(s):", report.found.len());
        for orphan in &report.found {
            println!("  {} ({})", orphan.name, orphan.path.display());
        }
    }
    if !report.conflicts.is_empty() {
        println!(
            "Skipped {} worktree(s) now owned by a live session:",
            report.conflicts.len()
        );
        for orphan in &report.conflicts {
            println!("  {} ({})", orphan.name, orphan.path.display());
        }
    }
    let without_worktree = report.stale_total - report.found.len() - report.conflicts.len();
    if without_worktree > 0 {
        println!("{without_worktree} stale session(s) carry no worktree.");
    }

    if dry_run {
        println!("\nDry run — no changes made.");
        return;
    }

    if !report.failed.is_empty() {
        println!(
            "Failed to remove {} worktree(s) — their sessions stay stale for retry:",
            report.failed.len()
        );
        for name in &report.failed {
            println!("  {name}");
        }
    }
    println!(
        "\nRemoved {} worktree(s). Marked {} stale session(s) as KILLED.",
        report.removed, report.marked_killed
    );
}

#[cfg(test)]
#[path = "clean_stale_tests.rs"]
mod tests;
