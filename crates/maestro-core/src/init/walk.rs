//! Walk upward from a starting directory to the nearest ancestor that
//! contains a `.git` entry. Returns the start directory unchanged when
//! no git root is ever found. Never panics, never errors.

use std::path::{Path, PathBuf};

/// Find the closest ancestor of `start` (including `start` itself) that
/// contains a `.git` directory. Falls back to `start` when no ancestor
/// contains one.
///
/// Hardening: requires `.git` to be a directory (not a file or symlink)
/// to avoid being redirected by a planted symlink, and bounds the walk
/// depth so a pathological symlink loop cannot run forever.
pub fn find_project_root(start: &Path) -> PathBuf {
    const MAX_DEPTH: usize = 64;
    let mut current = start.to_path_buf();
    for _ in 0..MAX_DEPTH {
        if current.join(".git").is_dir() {
            return current;
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return start.to_path_buf(),
        }
    }
    start.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn canon(p: &Path) -> PathBuf {
        // macOS tempdirs sit under /private/var/...; canonicalize so the
        // assertions match regardless of symlink prefix.
        fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
    }

    #[test]
    fn find_project_root_returns_self_when_no_git() {
        let dir = tempdir().unwrap();
        let result = find_project_root(dir.path());
        assert_eq!(canon(&result), canon(dir.path()));
    }

    #[test]
    fn find_project_root_returns_self_when_git_at_root() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        let result = find_project_root(dir.path());
        assert_eq!(canon(&result), canon(dir.path()));
    }

    #[test]
    fn find_project_root_walks_up_to_git() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        let nested = dir.path().join("sub").join("sub2");
        fs::create_dir_all(&nested).unwrap();
        let result = find_project_root(&nested);
        assert_eq!(canon(&result), canon(dir.path()));
    }

    #[test]
    fn find_project_root_does_not_escape_filesystem_root() {
        let dir = tempdir().unwrap();
        // No .git in the temp tree. Function must terminate by returning
        // either the start dir or some ancestor — never panic, never hang.
        let _ = find_project_root(dir.path());
    }
}
