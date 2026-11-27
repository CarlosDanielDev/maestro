//! The kill dialog (#1261, design §05 "Killing the wrong agent"): type the
//! session's issue number before `⏎` kills it.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use super::App;
use super::helpers::session_label;
use crate::session::pool::session_slug;
use crate::session::types::{Session, SessionStatus};
use crate::tui::ds::confirm::TypedConfirm;
use crate::tui::ds::status_bar::Place;
use crate::tui::ds::toast::{Severity, Toast};
use crate::work::unique_commits::unique_commit_count;

#[cfg(test)]
#[path = "kill_confirm_tests.rs"]
mod tests;

impl App {
    /// Open the kill dialog for `session_id` with an empty field. The
    /// uncommitted files and unpushed commits are counted here, once,
    /// never per frame.
    /// A session Kill cannot end (Errored, GatesRunning, NeedsPr: no
    /// `Killed` transition) gets a toast instead: Kill would do nothing.
    /// So does a kept record no process runs (#1507).
    pub fn open_kill_confirm(&mut self, session_id: uuid::Uuid) {
        self.open_kill_confirm_counting(session_id, count_uncommitted);
    }

    /// [`Self::open_kill_confirm`] with the uncommitted count injected, so
    /// a test can stand in a git that never answers.
    fn open_kill_confirm_counting(
        &mut self,
        session_id: uuid::Uuid,
        count: impl FnOnce(&Path) -> Option<LostFiles>,
    ) {
        if let Some(status) = self.unkillable_status(session_id) {
            let label = (self.pool.get_session(session_id))
                .map_or_else(|| "The session".to_string(), session_label);
            let text = format!(
                "{label} is {}: Kill cannot stop it, nothing changes",
                status.label().to_lowercase()
            );
            let toast = Toast::new(Severity::Info, text);
            self.push_toast(toast, std::time::Instant::now());
            return;
        }
        if let Some(label) = self.kept_record_label(session_id) {
            let text = format!("{label} is a saved record, not running: Kill has nothing to stop");
            let toast = Toast::new(Severity::Info, text);
            self.push_toast(toast, std::time::Instant::now());
            return;
        }
        let uncommitted = self.torn_down_worktree(session_id).and_then(count);
        let unpushed = self.unpushed_commits(session_id);
        let confirm = self.kill_confirm_with(session_id, uncommitted, unpushed);
        self.screen_state.kill_confirm = Some((session_id, confirm));
        self.navigate_to(super::TuiMode::ConfirmKill(session_id));
    }

    /// The dialog for `session_id` without running git (the draw fallback):
    /// both counts read as unknown, and still warn.
    pub fn kill_confirm(&self, session_id: uuid::Uuid) -> TypedConfirm {
        self.kill_confirm_with(session_id, None, None)
    }

    /// The dialog for `session_id`. The token is the issue number; a
    /// session with no issue uses the first 8 characters of its id.
    /// What happens to the worktree comes from [`Self::kill_teardown_rows`].
    fn kill_confirm_with(
        &self,
        session_id: uuid::Uuid,
        uncommitted: Option<LostFiles>,
        unpushed: Option<usize>,
    ) -> TypedConfirm {
        let short = session_id.to_string()[..8].to_string();
        let session = self.pool.get_session(session_id);
        let label = session
            .map(session_label)
            .unwrap_or_else(|| format!("S-{short}"));
        let token = session
            .and_then(|s| s.issue_number)
            .map_or(short, |n| n.to_string());
        let others = self
            .pool
            .all_sessions()
            .iter()
            .filter(|s| s.id != session_id && !s.status.is_terminal())
            .count();
        let other = match others {
            0 => "no other session is running".to_string(),
            1 => "1 other session keeps running".to_string(),
            n => format!("{n} other sessions keep running"),
        };
        // #1514: a queued session has no agent yet; `⏎` takes it out of
        // the queue (`pool.kill_session` → `dequeue`).
        let queued = self.place_of(session_id) == Some(Place::Queued);
        let (title, action, what, kept) = if queued {
            let title = format!("Remove session {label} from the queue?");
            let what = "It has not started: no agent to stop.";
            (title, "remove", what, "its record, marked killed")
        } else {
            let title = format!("Kill session {label}?");
            (title, "kill", "Stops the agent now.", "the transcript")
        };
        let mut rows = vec![
            (String::new(), what.into()),
            (String::new(), String::new()),
        ];
        rows.extend(self.kill_teardown_rows(session_id, uncommitted, unpushed));
        rows.extend([("kept".into(), kept.into()), ("other".into(), other)]);
        TypedConfirm::new(title, token, action, rows)
    }

    /// #1456: what Kill does to the session's work, read from the real
    /// teardown. A Killed session is finalized on the next tick, and
    /// `pool.finalize` keeps the worktree only for FailedGates and Errored,
    /// so its worktree (`git worktree remove --force`) goes, and its
    /// uncommitted files with it. #1469: the local branch is deleted only
    /// when it holds no unpushed commit; otherwise it is kept.
    /// `uncommitted` is [`Self::uncommitted_files`] and `unpushed`
    /// [`Self::unpushed_commits`]; `None` reads "unknown", never "nothing".
    pub(crate) fn kill_teardown_rows(
        &self,
        session_id: uuid::Uuid,
        uncommitted: Option<LostFiles>,
        unpushed: Option<usize>,
    ) -> Vec<(String, String)> {
        if let Some(status) = self.unkillable_status(session_id) {
            let text = format!(
                "it is {}: Kill leaves it as it is",
                status.label().to_lowercase()
            );
            return vec![("skips".into(), text)];
        }
        // #1514: the run loop's Stop › Kill reads these rows too.
        match self.place_of(session_id) {
            Some(Place::Queued) => {
                return vec![
                    ("gone".into(), "its place in the queue: it never starts".into()),
                    ("kept".into(), "every file: no worktree is touched".into()),
                ];
            }
            Some(Place::Kept) => {
                let text = "it is a saved record, not running: Kill leaves it as it is";
                return vec![("skips".into(), text.into())];
            }
            _ => {}
        }
        let managed = self.pool.get_active(session_id);
        if self.torn_down_worktree(session_id).is_none() {
            return vec![("lost".into(), "the turn in progress".into())];
        }
        let branch = match managed.and_then(|m| m.branch_name.as_deref()) {
            Some(branch) => format!("branch {branch}"),
            None => "its branch".to_string(),
        };
        let mut rows = match unpushed {
            Some(0) => vec![("gone".into(), format!("the worktree and {branch}"))],
            Some(n) => {
                let s = if n == 1 { "" } else { "s" };
                let kept = format!("{branch}, {n} unpushed commit{s}");
                vec![
                    ("gone".into(), "the worktree".into()),
                    ("kept".into(), kept),
                ]
            }
            None => {
                let kept = format!("{branch} if it has unpushed commits");
                vec![
                    ("gone".into(), "the worktree".into()),
                    ("kept".into(), kept),
                ]
            }
        };
        let lost = match uncommitted.map(|l| l.uncommitted) {
            Some(0) => "the turn in progress".to_string(),
            Some(1) => "1 uncommitted file, the turn in progress".to_string(),
            Some(n) => format!("{n} uncommitted files, the turn in progress"),
            None => "uncommitted files (count unknown), turn in progress".to_string(),
        };
        rows.push(("lost".into(), lost));
        // #1471: teardown deletes git-ignored files too. Their own row, so
        // build output (`target/`, one entry per folder) never reads as
        // lost work.
        match uncommitted.map(|l| l.ignored) {
            None | Some(0) => {}
            Some(1) => rows.push(("also".into(), "1 ignored file or folder, deleted".into())),
            Some(n) => rows.push((
                "also".into(),
                format!("{n} ignored files or folders, deleted"),
            )),
        }
        rows
    }

    /// The uncommitted and ignored files Kill throws away in `session_id`'s
    /// worktree. Runs git: call it once, when a dialog opens.
    pub(crate) fn uncommitted_files(&self, session_id: uuid::Uuid) -> Option<LostFiles> {
        count_uncommitted(self.torn_down_worktree(session_id)?)
    }

    /// How many commits only the session's branch holds: teardown keeps
    /// the branch unless this is `Some(0)`. Runs git: call it once.
    pub(crate) fn unpushed_commits(&self, session_id: uuid::Uuid) -> Option<usize> {
        let worktree = self.torn_down_worktree(session_id)?;
        let managed = self.pool.get_active(session_id)?;
        let branch = format!("maestro/{}", session_slug(&managed.session));
        unique_commit_count(worktree, &branch)
    }

    /// The session's worktree when it is the one teardown removes:
    /// `wipe_worktree` acts on `.maestro/worktrees/<session_slug>` only. A
    /// session in cwd (`None`, or `.` for an interaction whose worktree
    /// failed: the main checkout) loses no worktree and no files to Kill.
    fn torn_down_worktree(&self, session_id: uuid::Uuid) -> Option<&Path> {
        let managed = self.pool.get_active(session_id)?;
        let path = managed.worktree_path.as_deref()?;
        let slug = session_slug(&managed.session);
        path.ends_with(Path::new(".maestro").join("worktrees").join(slug))
            .then_some(path)
    }

    /// #1507: the label of a record no process runs ([`Place::Kept`]).
    /// `pool.kill_session` acts on active sessions only, so Kill on it
    /// would do nothing.
    fn kept_record_label(&self, session_id: uuid::Uuid) -> Option<String> {
        let sessions = self.sessions_by_place();
        let (session, place) = sessions.into_iter().find(|(s, _)| s.id == session_id)?;
        (place == Place::Kept).then(|| session_label(session))
    }

    /// The list `session_id` is in ([`Self::sessions_by_place`]).
    fn place_of(&self, session_id: uuid::Uuid) -> Option<Place> {
        let sessions = self.sessions_by_place();
        let (_, place) = sessions.into_iter().find(|(s, _)| s.id == session_id)?;
        Some(place)
    }

    /// #1507: every pool session in `all_sessions` order (active, finished,
    /// then queued) with the list it is in.
    pub(crate) fn sessions_by_place(&self) -> Vec<(&Session, Place)> {
        let sessions = self.pool.all_sessions();
        let active = self.pool.active_count();
        let queued_from = sessions.len() - self.pool.queued_count();
        let place = |i| match i {
            i if i < active => Place::Active,
            i if i >= queued_from => Place::Queued,
            _ => Place::Kept,
        };
        (sessions.into_iter().enumerate())
            .map(|(i, s)| (s, place(i)))
            .collect()
    }

    /// The status of a session Kill cannot move to `Killed`, if it is one.
    fn unkillable_status(&self, session_id: uuid::Uuid) -> Option<SessionStatus> {
        let status = self.pool.get_session(session_id)?.status;
        (!status.can_transition_to(SessionStatus::Killed)).then_some(status)
    }
}

/// What teardown deletes that no commit holds, one per `git status` line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LostFiles {
    /// Changed and untracked files, one per file.
    pub(crate) uncommitted: usize,
    /// #1471: git-ignored files, an ignored folder counted once.
    pub(crate) ignored: usize,
}

/// `None` when git cannot tell (the path is gone, not a repo) or does not
/// answer in [`GIT_COUNT_TIMEOUT`]: the dialog says unknown.
fn count_uncommitted(worktree: &Path) -> Option<LostFiles> {
    count_status_within(git_status(worktree), GIT_COUNT_TIMEOUT)
}

/// How long the dialog waits for git before it says "count unknown".
const GIT_COUNT_TIMEOUT: Duration = Duration::from_millis(500);

/// `--untracked-files=all` lists each untracked file; `--ignored=matching`
/// lists an ignored folder (`target/`) as one `!!` line, not its files.
/// `--no-optional-locks`: a count must not take `index.lock`, so a git
/// killed at the timeout leaves no lock behind for teardown to trip on.
fn git_status(worktree: &Path) -> Command {
    let mut git = Command::new("git");
    git.args([
        "--no-optional-locks",
        "status",
        "--porcelain",
        "--untracked-files=all",
        "--ignored=matching",
    ])
    .current_dir(worktree)
    .stdin(Stdio::null());
    git
}

/// The `git status --porcelain` lines `command` prints, when it exits with
/// success within `timeout`. Otherwise `None`, and a command still running
/// is killed: the UI never waits longer than `timeout`. The output is
/// counted as it streams, never stored whole.
fn count_status_within(mut command: Command, timeout: Duration) -> Option<LostFiles> {
    let mut child = (command.stdout(Stdio::piped()).stderr(Stdio::null()))
        .spawn()
        .ok()?;
    let stdout = child.stdout.take();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(stdout.and_then(count_status_lines));
    });
    let lost = rx.recv_timeout(timeout).ok().flatten();
    if lost.is_none() {
        let _ = child.kill();
    }
    let status = child.wait().ok()?;
    status.success().then_some(lost).flatten()
}

fn count_status_lines(out: impl Read) -> Option<LostFiles> {
    let mut lost = LostFiles {
        uncommitted: 0,
        ignored: 0,
    };
    for line in BufReader::new(out).split(b'\n') {
        if line.ok()?.starts_with(b"!! ") {
            lost.ignored += 1;
        } else {
            lost.uncommitted += 1;
        }
    }
    Some(lost)
}
