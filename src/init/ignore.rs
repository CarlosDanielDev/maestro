//! `.gitignore` handling for `maestro init` (#1151): keep maestro's runtime
//! output (logs, worktrees, state lock, log file) out of the user's
//! `git status` while leaving every other path — including the trackable
//! `.maestro/templates/` — untouched. Writes refuse to follow symlinks and
//! coverage is scoped to maestro's own block (see [`ensure_runtime_gitignore`]).

use crate::util::write_no_follow;
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
/// An entry also stops counting as covered when any `!` negation line
/// (`!entry` or a glob like `!*.json`, which is not parsed) appears after its
/// last line inside a maestro block, inside or after the block: the entry is
/// then re-added after that negation — in the last block when the negation
/// sits inside it, otherwise in a new block at the end of the file.
///
/// Matching is by exact line as git reads it (see [`git_pattern`]), not by
/// gitignore pattern — a user glob like `*.lock` is not treated as covering
/// `maestro-state.json.lock`, and ` maestro.log` (leading space) does not
/// cover `maestro.log`.
///
/// A repo-planted symlinked `.gitignore` (dangling, or aimed at a victim
/// file), a FIFO or any other non-regular file is refused outright: the read
/// goes through one no-follow handle ([`read_existing`]) and the write
/// through [`write_no_follow`], which re-checks and writes atomically via a
/// temp file plus rename (CWE-59 / CWE-73 / CWE-367).
pub fn ensure_runtime_gitignore(root: &Path) -> Result<GitignoreUpdate> {
    let path = root.join(".gitignore");
    let existing = read_existing(&path)?;
    let created = existing.is_none();
    let existing = existing.unwrap_or_default();

    let lines: Vec<&str> = existing.lines().collect();
    let spans = block_spans(&lines);
    let in_block = |i: usize| spans.iter().any(|&(start, end)| start < i && i < end);
    // Last negation of ANY pattern in the file (gitignore: last match wins).
    // Globs like `!*.json` are not parsed, so every `!` line is treated as
    // possibly re-including any entry. A `#` comment never starts with `!`.
    let last_negation = lines.iter().rposition(|line| line.trim().starts_with('!'));

    let missing: Vec<&'static str> = RUNTIME_ENTRIES
        .iter()
        .copied()
        .filter(|entry| {
            let last_positive = (0..lines.len())
                .rev()
                .find(|&i| in_block(i) && git_pattern(lines[i]) == *entry);
            match (last_positive, last_negation) {
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

    // Refill the last block only when no negation follows its end marker;
    // otherwise a new block at the end of the file must follow that
    // negation to win last-match-wins.
    let refill_target = spans
        .last()
        .copied()
        .filter(|&(_, end)| last_negation.is_none_or(|neg| neg < end));

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

/// Read `.gitignore` through one handle, or `None` when it does not exist.
///
/// The open itself refuses a symlink (`O_NOFOLLOW`), so there is no
/// check-then-read window for a link to be swapped in (CWE-367 / CWE-59).
/// `O_NONBLOCK` keeps a FIFO from blocking the open, and `fstat` on the same
/// handle refuses anything that is not a regular file before reading.
fn read_existing(path: &Path) -> Result<Option<String>> {
    use std::io::Read as _;
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
    }
    #[cfg(not(unix))]
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        anyhow::bail!("refusing to update {}: path is a symlink", path.display());
    }
    let mut file = match opts.open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        #[cfg(unix)]
        Err(e) if e.raw_os_error() == Some(libc::ELOOP) => {
            anyhow::bail!("refusing to update {}: path is a symlink", path.display())
        }
        Err(e) => return Err(anyhow::Error::from(e).context(format!("opening {}", path.display()))),
    };
    let meta = file
        .metadata()
        .with_context(|| format!("inspecting {}", path.display()))?;
    if !meta.is_file() {
        anyhow::bail!("refusing to update {}: not a regular file", path.display());
    }
    let mut content = String::new();
    file.read_to_string(&mut content)
        .with_context(|| format!("reading {}", path.display()))?;
    Ok(Some(content))
}

/// The pattern git sees for `line`: a trailing `\r` and trailing spaces
/// not escaped with a backslash are dropped; everything else, leading
/// whitespace included, is kept.
fn git_pattern(line: &str) -> &str {
    let mut pattern = line.strip_suffix('\r').unwrap_or(line);
    while let Some(rest) = pattern.strip_suffix(' ') {
        if rest.ends_with('\\') {
            break;
        }
        pattern = rest;
    }
    pattern
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

#[cfg(test)]
#[path = "ignore_gaps_tests.rs"]
mod gaps_tests;
