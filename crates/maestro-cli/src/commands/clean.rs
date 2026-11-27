use crate::session::cleanup::CleanupManager;
use crate::state::store::StateStore;

use super::clean_stale::{clean_stale, kept_branches_section, print_stale_clean_summary};

pub fn cmd_clean(dry_run: bool, stale: bool) -> anyhow::Result<()> {
    let repo_root = std::env::current_dir()?;

    if stale {
        let report = clean_stale(&repo_root, &StateStore::default_path(), dry_run)?;
        print_stale_clean_summary(&report, dry_run);
        return Ok(());
    }

    let mgr = CleanupManager::new(&repo_root);
    let orphans = mgr.scan_orphans()?;

    if orphans.is_empty() {
        println!("No orphaned worktrees found.");
        return Ok(());
    }

    println!("Found {} orphaned worktree(s):", orphans.len());
    for orphan in &orphans {
        println!("  {} ({})", orphan.name, orphan.path.display());
    }

    if dry_run {
        println!("\nDry run — no changes made.");
    } else {
        let report = mgr.remove_orphans(&orphans)?;
        println!("\nRemoved {} worktree(s).", report.removed);
        print!("{}", kept_branches_section(&report.kept_branches));
    }

    Ok(())
}
