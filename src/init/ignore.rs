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
/// An entry also stops counting as covered when a `!entry` negation appears
/// after its last line inside a maestro block (inside or after the block):
/// the entry is then re-added after that negation — in the last block when
/// the negation sits inside it, otherwise in a new block at the end of the
/// file.
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
    let spans = block_spans(&lines);
    let in_block = |i: usize| spans.iter().any(|&(start, end)| start < i && i < end);
    // Last `!entry` negation anywhere in the file (gitignore: last match wins).
    let last_negation = |entry: &str| {
        let negated = format!("!{entry}");
        lines.iter().rposition(|line| line.trim() == negated)
    };

    let missing: Vec<&'static str> = RUNTIME_ENTRIES
        .iter()
        .copied()
        .filter(|entry| {
            let last_positive = (0..lines.len())
                .rev()
                .find(|&i| in_block(i) && lines[i].trim() == *entry);
            match (last_positive, last_negation(entry)) {
                (Some(pos), Some(neg)) => neg > pos,
                (Some(_), None) => false,
                (None, _) => true,
            }
        })
        .collect();
    if missing.is_empty() {
        return Ok(GitignoreUpdate {
            created: false,
            added: Vec::new(),
        });
    }

    // Refill the last block only when no missing entry is negated after its
    // end marker; otherwise a new block at the end of the file must follow
    // that negation to win last-match-wins.
    let refill_target = spans.last().copied().filter(|&(_, end)| {
        missing
            .iter()
            .all(|entry| last_negation(entry).is_none_or(|neg| neg < end))
    });

    let new_content = match refill_target {
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

    write_no_follow(&path, &new_content).with_context(|| format!("writing {}", path.display()))?;
    Ok(GitignoreUpdate {
        created,
        added: missing,
    })
}

/// Every `(start, end)` line-index pair of a maestro block: a start marker
/// followed by the next end marker. A start without a later end is ignored.
fn block_spans(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut open = None;
    for (i, line) in lines.iter().enumerate() {
        match (line.trim(), open) {
            (BLOCK_START, None) => open = Some(i),
            (BLOCK_END, Some(start)) => {
                spans.push((start, i));
                open = None;
            }
            _ => {}
        }
    }
    spans
}

#[cfg(test)]
#[path = "ignore_tests.rs"]
mod tests;
