//! `.gitignore` handling for `maestro init` (#1151): keep maestro's runtime
//! output (logs, worktrees, state lock, log file) out of the user's
//! `git status` while leaving every other path — including the trackable
//! `.maestro/templates/` — untouched. Writes refuse to follow symlinks and
//! coverage is scoped to maestro's own block (see [`ensure_runtime_gitignore`]).

use crate::util::{exists_no_follow, write_no_follow};
use anyhow::{Context, Result};
use std::path::Path;

/// Result of [`ensure_runtime_gitignore`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitignoreUpdate {
    /// True when `.gitignore` did not exist (in any form, symlinks
    /// included) and was created by this call.
    pub created: bool,
    /// Entries missing from maestro's block, appended by this call (in
    /// [`RUNTIME_ENTRIES`] order).
    pub added: Vec<&'static str>,
}

/// The five runtime paths maestro writes into the project root (#1151).
/// `.maestro/templates/` is deliberately absent: it stays trackable.
pub const RUNTIME_ENTRIES: &[&str] = &[
    ".maestro/logs/",
    ".maestro/worktrees/",
    "maestro-state.json",
    "maestro-state.json.lock",
    "maestro.log",
];

const BLOCK_START: &str = "# --- maestro ---";
const BLOCK_END: &str = "# --- end maestro ---";

/// Make sure `root/.gitignore` covers [`RUNTIME_ENTRIES`] inside maestro's
/// marked `# --- maestro ---` block, without touching the user's own
/// content. Entries missing from the block are appended there (the block is
/// created when absent), so re-running `maestro init` is a no-op.
///
/// Coverage is checked only against lines inside maestro's own block, never
/// against the user's lines (gitignore's last match wins): a foreign exact
/// line could sit before a later negation, and a foreign `!entry` could
/// re-include a path maestro believes covered, so neither may suppress an
/// append. Because the block is appended at the end of the file (or refilled
/// before its end marker), maestro's entries come after any user line and
/// win the last-match comparison.
///
/// Matching is by exact (trimmed) line, not by gitignore pattern — a user
/// glob like `*.lock` is not treated as covering `maestro-state.json.lock`.
///
/// A repo-planted symlinked `.gitignore` (dangling, or aimed at a victim
/// file) is refused outright: pre-existence is detected with
/// [`exists_no_follow`] and the write goes through [`write_no_follow`],
/// which re-checks and writes atomically via a temp file plus rename
/// (CWE-59 / CWE-73).
pub fn ensure_runtime_gitignore(root: &Path) -> Result<GitignoreUpdate> {
    let path = root.join(".gitignore");
    let created = !exists_no_follow(&path);
    if !created {
        let meta = std::fs::symlink_metadata(&path)
            .with_context(|| format!("inspecting {}", path.display()))?;
        if meta.file_type().is_symlink() {
            anyhow::bail!("refusing to update {}: path is a symlink", path.display());
        }
    }
    let existing = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(anyhow::Error::from(e).context(format!("reading {}", path.display()))),
    };

    let lines: Vec<&str> = existing.lines().collect();
    let start = lines.iter().position(|line| line.trim() == BLOCK_START);
    let end = lines.iter().position(|line| line.trim() == BLOCK_END);
    let block_span = match (start, end) {
        (Some(start), Some(end)) if start < end => Some((start, end)),
        // No (sane) block: treated as absent below.
        _ => None,
    };

    let missing: Vec<&'static str> = RUNTIME_ENTRIES
        .iter()
        .copied()
        .filter(|entry| match block_span {
            Some((start, end)) => !lines[start + 1..end].iter().any(|line| line.trim() == *entry),
            None => true,
        })
        .collect();
    if missing.is_empty() {
        return Ok(GitignoreUpdate {
            created: false,
            added: Vec::new(),
        });
    }

    let new_content = match block_span {
        // Block already present: insert the missing entries before its
        // end marker so a re-run keeps a single block (#1151).
        Some((_start, end)) => {
            let mut lines = lines;
            for (offset, entry) in missing.iter().enumerate() {
                lines.insert(end + offset, entry);
            }
            lines.join("\n") + "\n"
        }
        // No (sane) block: append one after the user's content,
        // terminating an unterminated last line first so their final
        // pattern survives intact.
        None => {
            let mut out = existing.clone();
            if !out.is_empty() {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push('\n');
            }
            out.push_str(BLOCK_START);
            out.push('\n');
            for entry in &missing {
                out.push_str(entry);
                out.push('\n');
            }
            out.push_str(BLOCK_END);
            out.push('\n');
            out
        }
    };

    write_no_follow(&path, &new_content)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(GitignoreUpdate {
        created,
        added: missing,
    })
}

#[cfg(test)]
#[path = "ignore_tests.rs"]
mod tests;
