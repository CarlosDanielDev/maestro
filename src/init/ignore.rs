//! `.gitignore` handling for `maestro init` (#1151): keep maestro's runtime
//! output (logs, worktrees, state lock, log file) out of the user's
//! `git status` while leaving every other path — including the trackable
//! `.maestro/templates/` — untouched.

use anyhow::{Context, Result};
use std::path::Path;

/// Result of [`ensure_runtime_gitignore`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitignoreUpdate {
    /// True when `.gitignore` did not exist and was created by this call.
    pub created: bool,
    /// Entries the file was missing, appended by this call (in
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

/// Make sure `root/.gitignore` covers [`RUNTIME_ENTRIES`], without
/// duplicating anything and without touching the user's own content.
/// Missing entries are appended inside a marked `# --- maestro ---` block
/// (created when absent), so re-running `maestro init` is a no-op.
///
/// Matching is by exact (trimmed) line, not by gitignore pattern — a user
/// glob like `*.lock` is not treated as covering `maestro-state.json.lock`,
/// and a deliberate `!entry` negation later in the file would win over our
/// appended entries (gitignore semantics: last match wins).
pub fn ensure_runtime_gitignore(root: &Path) -> Result<GitignoreUpdate> {
    let path = root.join(".gitignore");
    let created = !path.exists();
    let existing = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(anyhow::Error::from(e).context(format!("reading {}", path.display()))),
    };

    let missing: Vec<&'static str> = RUNTIME_ENTRIES
        .iter()
        .copied()
        .filter(|entry| !existing.lines().any(|line| line.trim() == *entry))
        .collect();
    if missing.is_empty() {
        return Ok(GitignoreUpdate {
            created: false,
            added: Vec::new(),
        });
    }

    let new_content = {
        let lines: Vec<&str> = existing.lines().collect();
        let start = lines.iter().position(|line| line.trim() == BLOCK_START);
        let end = lines.iter().position(|line| line.trim() == BLOCK_END);
        match (start, end) {
            // Block already present: insert the missing entries before its
            // end marker so a re-run keeps a single block (#1151).
            (Some(start), Some(end)) if start < end => {
                let mut lines = lines;
                for (offset, entry) in missing.iter().enumerate() {
                    lines.insert(end + offset, entry);
                }
                lines.join("\n") + "\n"
            }
            // No (sane) block: append one after the user's content,
            // terminating an unterminated last line first so their final
            // pattern survives intact.
            _ => {
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
        }
    };

    std::fs::write(&path, new_content).with_context(|| format!("writing {}", path.display()))?;
    Ok(GitignoreUpdate {
        created,
        added: missing,
    })
}

#[cfg(test)]
#[path = "ignore_tests.rs"]
mod tests;
