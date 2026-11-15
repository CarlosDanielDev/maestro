use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::util::validate_slug;
use crate::work::worktree_teardown::{TeardownError, wipe_worktree};

/// Trait for managing git worktrees. Mockable in tests.
pub trait WorktreeManager: Send {
    /// Create a worktree at `.maestro/worktrees/<slug>` on branch `maestro/<slug>`.
    /// Returns the absolute path to the worktree directory.
    fn create(&self, slug: &str) -> Result<PathBuf>;

    /// Remove the worktree for the given slug.
    fn remove(&self, slug: &str) -> Result<()>;

    /// Check if a worktree exists for the given slug.
    #[allow(dead_code)] // Reason: worktree existence check — used in session orchestration
    fn exists(&self, slug: &str) -> bool;
}

/// Real implementation using `git worktree` commands.
pub struct GitWorktreeManager {
    repo_root: PathBuf,
}

impl GitWorktreeManager {
    pub fn new(repo_root: PathBuf) -> Self {
        Self { repo_root }
    }

    fn worktree_dir(&self) -> PathBuf {
        self.repo_root.join(".maestro").join("worktrees")
    }

    fn worktree_path(&self, slug: &str) -> PathBuf {
        self.worktree_dir().join(slug)
    }

    fn branch_name(&self, slug: &str) -> String {
        format!("maestro/{}", slug)
    }

    /// Guarded teardown step used by [`WorktreeManager::remove`] (#938). Routes
    /// the worktree + branch removal through the sanity-checked
    /// [`wipe_worktree`] primitive. `issue_number` is unattributed here (the
    /// slug, not an issue id, owns these worktrees), so `0` is passed for the
    /// tracing field only. Returns the typed error so the caller can choose to
    /// warn-and-continue (best-effort cleanup).
    fn guarded_remove(
        path: &Path,
        branch: &str,
        worktree_root: &Path,
    ) -> Result<(), TeardownError> {
        wipe_worktree(0, path, branch, worktree_root)
    }
}

impl WorktreeManager for GitWorktreeManager {
    fn create(&self, slug: &str) -> Result<PathBuf> {
        validate_slug(slug)?;
        let path = self.worktree_path(slug);
        let branch = self.branch_name(slug);

        // Ensure parent dir exists
        std::fs::create_dir_all(self.worktree_dir())?;

        let output = std::process::Command::new("git")
            .arg("worktree")
            .arg("add")
            .arg(&path)
            .arg("-b")
            .arg(&branch)
            .current_dir(&self.repo_root)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // If branch already exists, try without -b
            if stderr.contains("already exists") {
                let output2 = std::process::Command::new("git")
                    .arg("worktree")
                    .arg("add")
                    .arg(&path)
                    .arg(&branch)
                    .current_dir(&self.repo_root)
                    .output()?;
                if !output2.status.success() {
                    anyhow::bail!(
                        "git worktree add failed: {}",
                        String::from_utf8_lossy(&output2.stderr)
                    );
                }
            } else {
                anyhow::bail!("git worktree add failed: {}", stderr);
            }
        }

        Ok(path)
    }

    fn remove(&self, slug: &str) -> Result<()> {
        validate_slug(slug)?;
        let path = self.worktree_path(slug);
        let branch = self.branch_name(slug);
        // #938: route the destructive step through the hardened `wipe_worktree`
        // primitive so it inherits the root sanity-check (refuses `/` / `$HOME`,
        // canonicalized containment) and the leading-dash injection guard. Keep
        // the existing best-effort contract: a refusal or git failure is logged,
        // never propagated — `remove` has always returned `Ok` on failure.
        if let Err(e) = Self::guarded_remove(&path, &branch, &self.worktree_dir()) {
            tracing::warn!("worktree teardown refused/failed for {}: {}", slug, e);
        }
        Ok(())
    }

    fn exists(&self, slug: &str) -> bool {
        if validate_slug(slug).is_err() {
            return false;
        }
        self.worktree_path(slug).exists()
    }
}

/// Mock worktree manager for testing.
#[allow(dead_code)] // Reason: test mock — used in integration tests
pub struct MockWorktreeManager {
    created: std::sync::Mutex<Vec<String>>,
    fail_create: std::sync::Mutex<bool>,
}

#[allow(dead_code)] // Reason: test mock API — used in integration tests
impl MockWorktreeManager {
    pub fn new() -> Self {
        Self {
            created: std::sync::Mutex::new(Vec::new()),
            fail_create: std::sync::Mutex::new(false),
        }
    }

    pub fn set_create_error(&self, fail: bool) {
        *self.fail_create.lock().unwrap() = fail;
    }

    pub fn created_slugs(&self) -> Vec<String> {
        self.created.lock().unwrap().clone()
    }
}

impl Default for MockWorktreeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WorktreeManager for MockWorktreeManager {
    fn create(&self, slug: &str) -> Result<PathBuf> {
        validate_slug(slug)?;
        if *self.fail_create.lock().unwrap() {
            anyhow::bail!("mock: create error");
        }
        {
            let mut created = self.created.lock().unwrap();
            if created.contains(&slug.to_string()) {
                anyhow::bail!("mock: worktree already exists for {}", slug);
            }
            created.push(slug.to_string());
        }
        Ok(PathBuf::from(format!("/tmp/mock-worktrees/{}", slug)))
    }

    fn remove(&self, slug: &str) -> Result<()> {
        let mut created = self.created.lock().unwrap();
        if let Some(pos) = created.iter().position(|s| s == slug) {
            created.remove(pos);
            drop(created);
            Ok(())
        } else {
            anyhow::bail!("mock: no worktree for {}", slug)
        }
    }

    fn exists(&self, slug: &str) -> bool {
        self.created.lock().unwrap().contains(&slug.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_exists_false_for_unknown() {
        let mock = MockWorktreeManager::new();
        assert!(!mock.exists("nonexistent"));
    }

    #[test]
    fn mock_exists_true_after_create() {
        let mock = MockWorktreeManager::new();
        mock.create("my-slug").unwrap();
        assert!(mock.exists("my-slug"));
    }

    #[test]
    fn mock_exists_false_after_remove() {
        let mock = MockWorktreeManager::new();
        mock.create("temp").unwrap();
        mock.remove("temp").unwrap();
        assert!(!mock.exists("temp"));
    }

    #[test]
    fn mock_create_returns_path_with_slug() {
        let mock = MockWorktreeManager::new();
        let path = mock.create("feature-slug").unwrap();
        assert!(path.to_string_lossy().contains("feature-slug"));
    }

    #[test]
    fn mock_create_duplicate_returns_error() {
        let mock = MockWorktreeManager::new();
        mock.create("dup").unwrap();
        assert!(mock.create("dup").is_err());
    }

    #[test]
    fn mock_remove_nonexistent_returns_error() {
        let mock = MockWorktreeManager::new();
        assert!(mock.remove("ghost").is_err());
    }

    #[test]
    fn mock_can_be_set_to_fail_create() {
        let mock = MockWorktreeManager::new();
        mock.set_create_error(true);
        assert!(mock.create("any").is_err());
    }

    #[test]
    fn mock_tracks_created_slugs() {
        let mock = MockWorktreeManager::new();
        mock.create("a").unwrap();
        mock.create("b").unwrap();
        let slugs = mock.created_slugs();
        assert_eq!(slugs, vec!["a", "b"]);
    }

    #[test]
    fn create_rejects_path_traversal() {
        let mock = MockWorktreeManager::new();
        assert!(mock.create("../../../etc").is_err());
    }

    #[test]
    fn create_rejects_slashes() {
        let mock = MockWorktreeManager::new();
        assert!(mock.create("foo/bar").is_err());
    }

    #[test]
    fn create_rejects_empty_slug() {
        let mock = MockWorktreeManager::new();
        assert!(mock.create("").is_err());
    }

    #[test]
    fn exists_returns_false_for_invalid_slug() {
        let mock = MockWorktreeManager::new();
        assert!(!mock.exists("../escape"));
    }

    // #938: the destructive step of `GitWorktreeManager::remove` now routes
    // through the guarded `wipe_worktree` primitive. `guarded_remove` is that
    // exact teardown function, so exercising it with an unsafe root proves the
    // site refuses `/` (and, by the primitive, `$HOME`).
    #[test]
    fn guarded_remove_refuses_root_slash() {
        let result =
            GitWorktreeManager::guarded_remove(Path::new("/etc"), "maestro/x", Path::new("/"));
        assert!(
            matches!(result, Err(TeardownError::UnsafeRoot(_))),
            "expected UnsafeRoot for `/` root, got {result:?}"
        );
    }

    #[test]
    fn guarded_remove_refuses_empty_root() {
        let result =
            GitWorktreeManager::guarded_remove(Path::new("wt"), "maestro/x", Path::new(""));
        assert!(
            matches!(result, Err(TeardownError::UnsafeRoot(_))),
            "expected UnsafeRoot for empty root, got {result:?}"
        );
    }
}
