use crate::session;

pub fn cmd_clean(dry_run: bool) -> anyhow::Result<()> {
    let repo_root = std::env::current_dir()?;
    let mgr = session::cleanup::CleanupManager::new(&repo_root);
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
        let removed = mgr.remove_orphans(&orphans)?;
        println!("\nRemoved {} worktree(s).", removed);
    }

    Ok(())
}
