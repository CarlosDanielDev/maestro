use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Manages cleanup of orphaned worktrees left by crashed processes.
pub struct CleanupManager {
    worktree_dir: PathBuf,
}

/// Info about an orphan worktree found during scan.
#[derive(Debug, Clone)]
pub struct OrphanWorktree {
    pub path: PathBuf,
    pub name: String,
}

impl CleanupManager {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            worktree_dir: repo_root.join(".maestro").join("worktrees"),
        }
    }

    /// Scan for orphaned worktree directories that are not tracked by git.
    pub fn scan_orphans(&self) -> Result<Vec<OrphanWorktree>> {
        if !self.worktree_dir.exists() {
            return Ok(Vec::new());
        }

        // Get list of git-tracked worktrees
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let tracked_paths: HashSet<PathBuf> = stdout
            .lines()
            .filter_map(|line| line.strip_prefix("worktree "))
            .map(PathBuf::from)
            .collect();

        // List entries in .maestro/worktrees/
        let mut orphans = Vec::new();
        for entry in std::fs::read_dir(&self.worktree_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
                if !tracked_paths.iter().any(|tp| {
                    let tp_canonical = tp.canonicalize().unwrap_or_else(|_| tp.clone());
                    tp_canonical == canonical
                }) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    orphans.push(OrphanWorktree { path, name });
                }
            }
        }

        Ok(orphans)
    }

    /// Remove orphaned worktrees. Returns the count removed.
    pub fn remove_orphans(&self, orphans: &[OrphanWorktree]) -> Result<usize> {
        let mut removed = 0;
        for orphan in orphans {
            // Try git worktree remove first
            let result = Command::new("git")
                .args(["worktree", "remove", "--force"])
                .arg(&orphan.path)
                .output();

            match result {
                Ok(output) if output.status.success() => {
                    removed += 1;
                }
                _ => {
                    // Fallback: just remove the directory
                    if orphan.path.exists() {
                        std::fs::remove_dir_all(&orphan.path)?;
                        removed += 1;
                    }
                }
            }
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_manager_with_correct_path() {
        let mgr = CleanupManager::new(Path::new("/tmp/repo"));
        assert_eq!(
            mgr.worktree_dir,
            PathBuf::from("/tmp/repo/.maestro/worktrees")
        );
    }

    #[test]
    fn scan_orphans_returns_empty_when_dir_missing() {
        let mgr = CleanupManager::new(Path::new("/tmp/nonexistent-repo-12345"));
        let orphans = mgr.scan_orphans().unwrap();
        assert!(orphans.is_empty());
    }
}
