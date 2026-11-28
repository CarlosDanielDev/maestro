//! Destructive worktree teardown primitive (issue #740).
//!
//! `wipe_worktree` removes a git worktree and its branch when a PR-terminator
//! fires for an interaction session. It is the destructive primitive that #741
//! wires into the UI lifecycle; this module exposes it and tests it in
//! isolation — there is **no** automatic invocation here.
//!
//! Two non-negotiable safety properties:
//!
//! 1. **Sanity-gated.** The worktree path must canonicalize to a location under
//!    the configured `worktree_root`. A root of `/` or `$HOME` is refused
//!    outright (`UnsafeRoot`). Symlinks that escape the root resolve to their
//!    real location and fail the containment check (`OutOfRoot`).
//! 2. **Idempotent.** The terminator may fire twice across crashes, so a
//!    missing path, an already-removed worktree, or a missing branch are all
//!    treated as success, not error.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::git_within::{GIT_COUNT_TIMEOUT, output_within};
use super::unique_commits::unique_commit_count;

/// Typed failure modes for [`wipe_worktree`]. No path here ever panics; every
/// fallible step maps to one of these variants.
#[derive(Debug, thiserror::Error)]
pub enum TeardownError {
    /// The worktree path canonicalizes outside `worktree_root`.
    #[error("worktree path {} is outside the worktree root {}", path.display(), root.display())]
    OutOfRoot { path: PathBuf, root: PathBuf },

    /// A filesystem operation (canonicalize, existence check) failed.
    #[error("io error on {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// A `git` invocation exited non-zero with an stderr we do not treat as a
    /// benign "already clean" signal.
    #[error("git command failed ({cmd}) status={status}: {stderr}")]
    GitCommandFailed {
        cmd: String,
        stderr: String,
        status: i32,
    },

    /// `git worktree remove` reported success but the directory is still on disk.
    #[error("worktree path still exists after teardown: {}", _0.display())]
    PathStillExists(PathBuf),

    /// `worktree_root` resolved to `/` or `$HOME` — refused as a misconfiguration.
    #[error("refusing to operate on unsafe worktree root: {}", _0.display())]
    UnsafeRoot(PathBuf),
}

/// Remove the worktree at `path` and delete `branch`, refusing to act unless
/// `path` canonicalizes safely under `worktree_root`. Idempotent: a second call
/// after the worktree is gone returns `Ok(())`.
pub fn wipe_worktree(
    issue_number: u64,
    path: &Path,
    branch: &str,
    worktree_root: &Path,
) -> Result<(), TeardownError> {
    wipe(issue_number, path, branch, worktree_root, false)
}

/// How many commits only `branch` holds (`None`: unknown), read from a
/// directory inside the repo: [`unique_commit_count`], or a stand-in.
type CountUnique<'a> = &'a dyn Fn(&Path, &str) -> Option<usize>;

/// Whether git says `branch` is gone, read from a directory inside the
/// repo: [`is_branch_missing`], or a stand-in (#1567).
type IsMissing<'a> = &'a dyn Fn(&Path, &str) -> bool;

/// [`wipe_worktree`], keeping `branch` when `keep_unpushed` and it holds
/// unpushed commits (#1469, `wipe_worktree_keeping_unpushed`).
pub(super) fn wipe(
    issue_number: u64,
    path: &Path,
    branch: &str,
    worktree_root: &Path,
    keep_unpushed: bool,
) -> Result<(), TeardownError> {
    let count: CountUnique = &unique_commit_count;
    let keep = keep_unpushed.then_some(count);
    wipe_counting(
        issue_number,
        path,
        branch,
        worktree_root,
        keep,
        &is_branch_missing,
    )
}

/// [`wipe`] with the unique-commit count (#1481) and the missing-branch
/// check (#1567) injected, so a test can stand in a git that never
/// answers and run real git for the rest. `None`: the branch is deleted
/// without a count.
fn wipe_counting(
    issue_number: u64,
    path: &Path,
    branch: &str,
    worktree_root: &Path,
    keep_unpushed: Option<CountUnique>,
    missing: IsMissing,
) -> Result<(), TeardownError> {
    // Lifecycle span (#742): every git call below runs inside it, and the
    // outcome is recorded before return.
    let span = tracing::info_span!(
        "interaction.teardown",
        issue = issue_number,
        path = %path.display(),
        branch,
        outcome = tracing::field::Empty,
    );
    let _guard = span.enter();
    tracing::info!(
        issue_number,
        branch,
        path = %path.display(),
        "worktree teardown requested"
    );

    // `sanity_check_under_root` canonicalizes and gates the worktree root; the
    // git commands only need a cwd inside the owning repo, so the raw
    // `worktree_root` (always under the repo) serves as that cwd directly.
    let result = (|| match sanity_check_under_root(path, worktree_root)? {
        None => {
            // Path already gone (idempotent re-entry). Still attempt the branch
            // delete — it may have survived a partial earlier teardown.
            tracing::warn!(issue_number, path = %path.display(), "worktree path already absent; skipping remove");
            delete_branch(worktree_root, branch, keep_unpushed, missing)?;
            Ok(())
        }
        Some(canon_path) => {
            run_git_worktree_remove(worktree_root, &canon_path)?;
            delete_branch(worktree_root, branch, keep_unpushed, missing)?;
            if canon_path.exists() {
                return Err(TeardownError::PathStillExists(canon_path));
            }
            Ok(())
        }
    })();
    span.record(
        "outcome",
        match &result {
            Ok(()) => "ok",
            Err(_) => "failed",
        },
    );
    result
}

/// Canonicalize `p`, mapping any io error into [`TeardownError::Io`].
fn canonicalize(p: &Path) -> Result<PathBuf, TeardownError> {
    p.canonicalize().map_err(|source| TeardownError::Io {
        path: p.into(),
        source,
    })
}

/// The user's `$HOME`, canonicalized. `None` when unset or unresolvable — a
/// missing HOME is not itself unsafe, so we simply skip that guard.
fn canonical_home() -> Option<PathBuf> {
    std::env::var_os("HOME").and_then(|h| PathBuf::from(h).canonicalize().ok())
}

/// Canonicalize `path` and `worktree_root`, refuse unsafe roots, and enforce
/// containment. Returns `Ok(None)` when `path` does not exist (idempotent skip),
/// `Ok(Some(canonical_path))` when it exists safely under the root.
fn sanity_check_under_root(
    path: &Path,
    worktree_root: &Path,
) -> Result<Option<PathBuf>, TeardownError> {
    // An empty or relative worktree root cannot be trusted: its meaning depends
    // on the process cwd, so a teardown could escape its intended location
    // (#741 sec — the interaction cwd-fallback sets `worktree_path = "."`,
    // whose parent is `""`). Refuse before canonicalize, which would otherwise
    // resolve a relative root against cwd and pass the `/`/`$HOME` guard.
    if worktree_root.as_os_str().is_empty() || worktree_root.is_relative() {
        return Err(TeardownError::UnsafeRoot(worktree_root.to_path_buf()));
    }

    // Validate the root first: an unsafe root (`/` or `$HOME`) is refused even
    // when the worktree path is already gone, so this must precede the
    // missing-path idempotency skip below.
    let canon_root = canonicalize(worktree_root)?;
    if is_unsafe_root(&canon_root, canonical_home().as_deref()) {
        return Err(TeardownError::UnsafeRoot(canon_root));
    }

    // Missing path → idempotent no-op. Checked before `canonicalize(path)`,
    // which would otherwise fail with NotFound on an absent path.
    if !path.exists() {
        return Ok(None);
    }

    let canon_path = canonicalize(path)?;
    if !canon_path.starts_with(&canon_root) {
        return Err(TeardownError::OutOfRoot {
            path: canon_path,
            root: canon_root,
        });
    }
    Ok(Some(canon_path))
}

/// `git worktree remove --force <path>`, run from `repo_dir`. Idempotent: an
/// "is not a working tree" stderr (already removed) maps to `Ok(())`.
fn run_git_worktree_remove(repo_dir: &Path, path: &Path) -> Result<(), TeardownError> {
    let output = Command::new("git")
        .arg("worktree")
        .arg("remove")
        .arg("--force")
        .arg("--")
        .arg(path)
        .current_dir(repo_dir)
        .output()
        .map_err(|source| TeardownError::Io {
            path: path.into(),
            source,
        })?;

    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if is_worktree_absent(&stderr) {
        tracing::warn!(path = %path.display(), "worktree already absent from git; treating remove as success");
        return Ok(());
    }
    Err(TeardownError::GitCommandFailed {
        cmd: format!("git worktree remove --force -- {}", path.display()),
        stderr: stderr.into_owned(),
        status: output.status.code().unwrap_or(-1),
    })
}

/// Force-delete `branch`; with `keep`, only when that count says it holds
/// no commit another ref lacks (#1469): an uncountable branch stays too.
/// Each outcome logs its own true reason (#1484).
fn delete_branch(
    repo_dir: &Path,
    branch: &str,
    keep: Option<CountUnique>,
    missing: IsMissing,
) -> Result<(), TeardownError> {
    if let Some(count) = keep {
        match count(repo_dir, branch) {
            Some(0) => {}
            Some(n) => {
                tracing::info!(branch, "keeping branch with {n} unique commit(s)");
                return Ok(());
            }
            None if missing(repo_dir, branch) => {
                tracing::info!(branch, "branch already absent; nothing to delete");
                return Ok(());
            }
            None => {
                tracing::info!(branch, "could not count commits; keeping it to be safe");
                return Ok(());
            }
        }
    }
    run_git_branch_delete(repo_dir, branch, true)
}

/// True only when git answers, within [`GIT_COUNT_TIMEOUT`], that
/// `refs/heads/<branch>` does not exist. A failure or a timeout is not
/// proof the branch is gone, so the caller keeps it (#1567).
fn is_branch_missing(repo_dir: &Path, branch: &str) -> bool {
    let mut git = Command::new("git");
    git.args(["for-each-ref", "--format=%(refname)"])
        .arg(format!("refs/heads/{branch}"))
        .current_dir(repo_dir);
    branch_missing_within(git, branch, GIT_COUNT_TIMEOUT)
}

/// True when `command` (a `for-each-ref` listing) answers with success in
/// `timeout` and lists no `refs/heads/<branch>`. `for-each-ref` exits 0
/// on no match, so the shared timeout helper, which only keeps a success,
/// can still tell "missing" from "no answer".
fn branch_missing_within(command: Command, branch: &str, timeout: Duration) -> bool {
    let full = format!("refs/heads/{branch}");
    let listed = output_within(command, timeout, |mut out| {
        let mut text = String::new();
        out.read_to_string(&mut text).ok()?;
        Some(text)
    });
    listed.is_some_and(|text| !text.lines().any(|line| line == full))
}

/// `git branch -D|-d <branch>`, run from `repo_dir`. Idempotent: a
/// "branch ... not found" stderr maps to `Ok(())`.
fn run_git_branch_delete(repo_dir: &Path, branch: &str, force: bool) -> Result<(), TeardownError> {
    let flag = if force { "-D" } else { "-d" };
    let output = Command::new("git")
        .arg("branch")
        .arg(flag)
        // `--` terminates option parsing so a branch name beginning with `-`
        // (e.g. `-r`) is treated as a literal name, not a git flag (#740 sec).
        .arg("--")
        .arg(branch)
        .current_dir(repo_dir)
        .output()
        .map_err(|source| TeardownError::Io {
            path: repo_dir.into(),
            source,
        })?;

    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if is_branch_absent(&stderr) {
        tracing::warn!(branch, "branch already absent; treating delete as success");
        return Ok(());
    }
    Err(TeardownError::GitCommandFailed {
        cmd: format!("git branch {flag} -- {branch}"),
        stderr: stderr.into_owned(),
        status: output.status.code().unwrap_or(-1),
    })
}

/// True when `canon_root` is a root we must never tear a worktree out of.
/// `canon_home` is injected (already canonicalized) so this stays a pure,
/// env-free predicate — see risk R2/R3 in the architect blueprint.
fn is_unsafe_root(canon_root: &Path, canon_home: Option<&Path>) -> bool {
    canon_root == Path::new("/") || canon_home == Some(canon_root)
}

/// True when git stderr indicates the worktree is not registered (already gone).
/// Verified against git's `fatal: '<path>' is not a working tree`.
fn is_worktree_absent(stderr: &str) -> bool {
    stderr.contains("is not a working tree")
}

/// True when git stderr indicates the branch does not exist (already deleted).
/// Anchored on git's actual phrasing `error: branch '<name>' not found` rather
/// than two free-floating substrings, so an unrelated failure that happens to
/// mention both words cannot mask a real error (#740 sec — false-absent guard).
fn is_branch_absent(stderr: &str) -> bool {
    stderr
        .lines()
        .any(|line| line.trim_start().starts_with("error: branch ") && line.contains("not found"))
}

#[cfg(test)]
#[path = "worktree_teardown_tests.rs"]
mod tests;
