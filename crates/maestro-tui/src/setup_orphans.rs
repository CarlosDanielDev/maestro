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

/// The startup notice on screen: its toast text and the dirs it is
/// about. Their shown record is written only when the user dismisses it.
pub(crate) struct StartupOrphans {
    message: String,
    dirs: Vec<PathBuf>,
}

/// Report the untracked dirs not shown before, or the scan error: one
/// sticky toast on the first frame that says where to look. The full list
/// and the exact remove commands go to maestro.log, and to `stderr` when
/// given (`--once`: printed counts as shown). 80x24 has no room for one
/// command per dir on screen, and the log drawer does not scroll.
pub(crate) fn report_startup_orphans(
    app: &mut App,
    repo_root: &Path,
    now: std::time::Instant,
    mut stderr: Option<&mut dyn Write>,
) {
    use crate::tui::ds::toast::{Severity, Toast};
    let Some(Notice {
        message,
        lines,
        dirs,
    }) = orphan_notice(repo_root)
    else {
        return;
    };
    for line in lines {
        let line = sanitize_for_terminal(&line);
        tracing::warn!("{line}");
        if let Some(w) = stderr.as_deref_mut() {
            let _ = writeln!(w, "warning: {line}");
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
    /// The maestro.log (and `--once` stderr) lines.
    lines: Vec<String>,
    /// The dirs to record once the notice was seen.
    dirs: Vec<PathBuf>,
}

/// Message cells of an error toast at 80 columns, in either icon mode: its
/// row is 68 cells (72 wide, less border and padding), less the
/// `<icon> error  ` prefix of the wider icon (`[X]` in ASCII mode: 57).
fn toast_cells() -> usize {
    use crate::tui::icons::{IconId, get_for_mode};
    use unicode_width::UnicodeWidthStr;
    let icon = [true, false]
        .map(|nerd| get_for_mode(IconId::XCircle, nerd).width())
        .into_iter()
        .max()
        .unwrap_or(0);
    68usize.saturating_sub(icon + " error  ".len())
}

/// The toast line that names one dir, when it is plain ASCII (so chars are
/// cells) and fits the toast in either icon mode; else `None`.
pub(crate) fn one_dir_line(name: &str) -> Option<String> {
    let line = format!("untracked dir kept: .maestro/worktrees/{name}");
    let plain = name.chars().all(|c| c.is_ascii_graphic());
    (plain && line.len() <= toast_cells()).then_some(line)
}

/// `None` when there is nothing new to say.
fn orphan_notice(repo_root: &Path) -> Option<Notice> {
    let scan = crate::session::cleanup::CleanupManager::new(repo_root).scan_orphans();
    let orphans = match scan {
        Ok(orphans) => orphans,
        Err(e) => {
            return Some(Notice {
                message: "orphan worktree scan failed · see maestro.log".into(),
                lines: vec![format!("orphan worktree scan failed: {e:#}")],
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
    // One dir is named when the whole line fits; else the count form.
    let named = (n == 1).then(|| one_dir_line(&new[0].name)).flatten();
    let message = named
        .unwrap_or_else(|| format!("{n} untracked dir{s} kept · check: ls .maestro/worktrees"));
    let mut lines = vec![format!(
        "kept {n} dir{s} git does not track in .maestro/worktrees/; to remove, run from the repo root:"
    )];
    lines.extend(new.iter().map(|o| remove_command(&o.name)));
    Some(Notice {
        message,
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

/// The exact remove command for `.maestro/worktrees/<name>`, run from
/// the repo root.
pub(crate) fn remove_command(name: &str) -> String {
    format!(
        "rm -rf {}",
        shell_word(&format!(".maestro/worktrees/{name}"))
    )
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
