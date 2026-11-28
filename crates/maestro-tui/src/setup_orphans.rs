//! #1563: startup reports the untracked dirs under `.maestro/worktrees/`
//! and never deletes them: a dir git does not track may hold the only copy
//! of files no git ref has, and `git worktree remove` cannot remove it.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::tui::app::App;
use crate::tui::screens::sanitize_for_terminal;

/// Marker inside an untracked dir: the user saw the startup notice that
/// named it. Removing the dir by hand removes the marker too, so a new dir
/// with the same name is shown again; nothing has to match a recreated dir
/// to an old record.
pub(crate) const SHOWN_MARKER: &str = ".maestro-orphan-shown";

/// The startup notice on screen: its toast text and the dirs it names.
/// Their shown record is written only when the user dismisses it.
pub(crate) struct StartupOrphans {
    message: String,
    dirs: Vec<PathBuf>,
}

/// Report the untracked dirs not shown before, or the scan error: one
/// sticky toast on the first frame that points to `⌃L`, where the activity
/// log has the full remove line for each dir. Also logged to maestro.log,
/// and written to `stderr` when given (`--once`: printed counts as shown).
pub(crate) fn report_startup_orphans(
    app: &mut App,
    repo_root: &Path,
    now: std::time::Instant,
    mut stderr: Option<&mut dyn Write>,
) {
    use crate::tui::activity_log::LogLevel;
    use crate::tui::ds::toast::{Severity, Toast};
    let Some(notice) = orphan_notice(repo_root) else {
        return;
    };
    let Notice {
        message,
        lines,
        dirs,
    } = notice;
    for line in lines {
        let full = sanitize_for_terminal(&line.join(""));
        tracing::warn!("{full}");
        if let Some(w) = stderr.as_deref_mut() {
            let _ = writeln!(w, "warning: {full}");
        }
        // Each piece fits one 80-column drawer row; a shell `\` at the end
        // joins it to the next, so nothing is cut with `…`.
        let last = line.len() - 1;
        for (i, piece) in line.iter().enumerate() {
            let piece = sanitize_for_terminal(piece);
            let row = if i < last {
                format!("{piece}\\")
            } else {
                piece
            };
            app.activity_log
                .push_simple("cleanup".into(), row, LogLevel::Info);
        }
    }
    if stderr.is_some() {
        mark_shown(&dirs);
    }
    app.push_first_frame_toast(Toast::new(Severity::Error, message.clone()), now);
    app.startup_orphans = Some(StartupOrphans { message, dirs });
}

impl App {
    /// `Esc` dismissed the toast `message`: when it is the startup notice,
    /// the user saw it, so its dirs are recorded as shown.
    pub(crate) fn ack_startup_orphans(&mut self, message: &str) {
        if (self.startup_orphans.as_ref()).is_some_and(|s| s.message == message)
            && let Some(notice) = self.startup_orphans.take()
        {
            mark_shown(&notice.dirs);
        }
    }
}

/// What one launch reports.
struct Notice {
    /// The toast text.
    message: String,
    /// Each line as the pieces of its drawer rows; joined, the full line.
    lines: Vec<Vec<String>>,
    /// The dirs to record once the notice was seen.
    dirs: Vec<PathBuf>,
}

/// Message cells of an activity-log row in the 80-column log drawer: 78
/// inside the border, less `HH:MM:SS [cleanup] ` (19), less 1 spare.
pub(crate) const LOG_ROW_CELLS: usize = 58;

/// `None` when there is nothing new to say.
fn orphan_notice(repo_root: &Path) -> Option<Notice> {
    let scan = crate::session::cleanup::CleanupManager::new(repo_root).scan_orphans();
    let orphans = match scan {
        Ok(orphans) => orphans,
        Err(e) => {
            return Some(Notice {
                message: "orphan worktree scan failed · ⌃L shows why".into(),
                lines: vec![vec![format!("orphan worktree scan failed: {e:#}")]],
                dirs: Vec::new(),
            });
        }
    };
    let new: Vec<_> = orphans.into_iter().filter(|o| !shown(&o.path)).collect();
    if new.is_empty() {
        return None;
    }
    let n = new.len();
    let s = if n == 1 { "" } else { "s" };
    let mut lines = vec![vec![format!(
        "kept {n} dir{s} git does not track; remove with:"
    )]];
    lines.extend(new.iter().map(|o| remove_command(&o.name)));
    Some(Notice {
        message: format!("kept {n} untracked dir{s} · ⌃L shows how to remove"),
        lines,
        dirs: new.into_iter().map(|o| o.path).collect(),
    })
}

/// A real dir (not a symlink) that holds the marker.
fn shown(dir: &Path) -> bool {
    real_dir(dir) && std::fs::symlink_metadata(dir.join(SHOWN_MARKER)).is_ok()
}

fn real_dir(dir: &Path) -> bool {
    std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir())
}

/// Write the marker in each dir. A symlink (never write outside the
/// worktree root) or a marker that cannot be written stays unrecorded, so
/// it is shown again: never silent.
fn mark_shown(dirs: &[PathBuf]) {
    for dir in dirs.iter().filter(|d| real_dir(d)) {
        let marker = dir.join(SHOWN_MARKER);
        let made = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker);
        if let Err(e) = made
            && e.kind() != std::io::ErrorKind::AlreadyExists
        {
            tracing::warn!("cannot record orphan notice {}: {e}", marker.display());
        }
    }
}

/// The remove line for `.maestro/worktrees/<name>`, run from the repo
/// root, as pieces of at most [`LOG_ROW_CELLS`] - 1 chars (room for the
/// `\` that joins a piece to the next). The path is cut between pieces,
/// and each piece is quoted on its own when the shell would split it, so
/// the joined pieces (or the rows with their `\`) are one exact command.
pub(crate) fn remove_command(name: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut head = String::from("rm -rf ");
    let mut part = String::new();
    for c in format!(".maestro/worktrees/{name}").chars() {
        let grown = format!("{part}{c}");
        let cells = head.chars().count() + shell_word(&grown).chars().count();
        if cells >= LOG_ROW_CELLS && !part.is_empty() {
            pieces.push(format!("{head}{}", shell_word(&part)));
            head.clear();
            part = c.to_string();
        } else {
            part = grown;
        }
    }
    pieces.push(format!("{head}{}", shell_word(&part)));
    pieces
}

/// `s` as one shell word: as is when every char is plain, else
/// single-quoted.
fn shell_word(s: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "._/-@%+=:,".contains(c);
    if s.chars().all(plain) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
#[path = "setup_orphan_tests.rs"]
pub(crate) mod tests;
