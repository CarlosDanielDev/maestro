//! Adoption of external Claude Code conversations (#1053).
//!
//! An adopted session starts already bound to a provider conversation id
//! and runs in the transcript's original cwd — never in a worktree
//! (`--resume` locates the transcript by cwd, so the wrong cwd means the
//! conversation is unreachable). The first turn resumes the external
//! thread via the pre-bound delegation in
//! `ManagedSession::build_request`; the constructor lives next to its
//! sibling `SessionPool::create_interaction_session`.

use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::agent_provider::claude::transcript_path;

/// Transcript quiet period (#1053, council verdict 2026-10-14): a
/// transcript modified within this window may belong to a still-live
/// session — adoption proceeds, but the UI layer warns. Claim-once stays
/// the source session's job (Claude Code refuses concurrent resumes);
/// maestro only warns.
pub const ADOPT_QUIET_PERIOD: Duration = Duration::from_secs(600);

/// Result of adopting an external conversation (#1053).
#[derive(Debug)]
pub struct AdoptOutcome {
    pub session_id: uuid::Uuid,
    /// True when the transcript's mtime is inside [`ADOPT_QUIET_PERIOD`] —
    /// the source session may still be live somewhere else. Advisory only;
    /// the adopt proceeds either way.
    pub live_warning: bool,
}

/// Whether `path` was modified within `quiet` of `now`. A missing or
/// unreadable-mtime file is never "recent"; a future mtime (clock skew)
/// counts as maximally fresh.
pub(crate) fn modified_within(path: &Path, now: SystemTime, quiet: Duration) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    let Ok(mtime) = metadata.modified() else {
        return false;
    };
    now.duration_since(mtime).unwrap_or(Duration::ZERO) < quiet
}

/// Live-session guard for adoption (#1053): stat the conversation's
/// transcript the same way the interactive transport will locate it
/// (`$HOME`-rooted, cwd-munged). `false` when `HOME` is unset or the
/// transcript is absent — nothing live to warn about.
pub(crate) fn transcript_is_live(id: &str, cwd: &Path, now: SystemTime) -> bool {
    let Ok(home) = std::env::var("HOME") else {
        return false;
    };
    transcript_is_live_in(Path::new(&home), id, cwd, now)
}

/// [`transcript_is_live`] with the transcript root injected (tests pass a
/// tempdir; production resolves `$HOME`).
///
/// Security guard (the #1031-#1033 path-traversal class): the id must
/// pass the same allowlist that gates `--resume` argv BEFORE any path is
/// built. Without it, an absolute id makes `PathBuf::join` replace the
/// whole base and the stat would probe an attacker-chosen file.
pub(crate) fn transcript_is_live_in(home: &Path, id: &str, cwd: &Path, now: SystemTime) -> bool {
    if !crate::session::parser::is_valid_session_id(id) {
        return false;
    }
    modified_within(&transcript_path(home, cwd, id), now, ADOPT_QUIET_PERIOD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn adopt_live_guard_warns_on_recent_mtime() {
        // #1053: fresh (warn) and stale (no warn) fixtures. Freshness is
        // controlled via `now` so the test never sleeps or fakes mtimes.
        let dir = tempfile::tempdir().expect("tempdir");
        let fresh = dir.path().join("fresh.jsonl");
        std::fs::write(&fresh, "{}").expect("write fixture");
        let now = SystemTime::now();

        assert!(
            modified_within(&fresh, now, ADOPT_QUIET_PERIOD),
            "fresh transcript must warn"
        );
        assert!(
            !modified_within(&fresh, now + Duration::from_secs(1200), ADOPT_QUIET_PERIOD),
            "stale transcript must not warn"
        );
    }

    #[test]
    fn adopt_live_guard_treats_missing_transcript_and_clock_skew_sanely() {
        let dir = tempfile::tempdir().expect("tempdir");
        let now = SystemTime::now();

        assert!(
            !modified_within(&dir.path().join("missing.jsonl"), now, ADOPT_QUIET_PERIOD),
            "missing transcript must not warn"
        );

        let fresh = dir.path().join("fresh.jsonl");
        std::fs::write(&fresh, "{}").expect("write fixture");
        assert!(
            modified_within(&fresh, now - Duration::from_secs(60), ADOPT_QUIET_PERIOD),
            "future mtime (clock skew) must count as fresh"
        );
    }

    #[test]
    fn transcript_is_live_positive_and_negative_paths_with_injected_home() {
        // Judge review of #1071: the live guard needs positive-path
        // coverage on a controlled base — never the real `$HOME`.
        let home = tempfile::tempdir().expect("tempdir home");
        let cwd = std::path::Path::new("/repo/original");
        let transcript = transcript_path(home.path(), cwd, "fresh-id");
        std::fs::create_dir_all(transcript.parent().expect("parent dir")).expect("mkdir");
        std::fs::write(&transcript, "{}").expect("write fixture");
        let now = SystemTime::now();

        assert!(
            transcript_is_live_in(home.path(), "fresh-id", cwd, now),
            "fresh transcript at the munged location must be live"
        );
        assert!(
            !transcript_is_live_in(
                home.path(),
                "fresh-id",
                cwd,
                now + Duration::from_secs(1200)
            ),
            "stale transcript must not be live"
        );
        assert!(
            !transcript_is_live_in(home.path(), "absent-id", cwd, now),
            "missing transcript must not be live"
        );
    }

    #[test]
    fn transcript_is_live_refuses_path_escaping_ids() {
        // Security guard (the #1031-#1033 class): an id may never steer
        // the stat outside the transcript root. Without the allowlist
        // check, an absolute id makes `PathBuf::join` replace the base
        // entirely and probes an attacker-chosen file's mtime.
        let home = tempfile::tempdir().expect("tempdir home");
        let victim_dir = tempfile::tempdir().expect("victim tempdir");
        let victim = victim_dir.path().join("victim.jsonl");
        std::fs::write(&victim, "{}").expect("write fresh victim fixture");

        let escaping_id = victim_dir
            .path()
            .join("victim")
            .to_str()
            .expect("utf-8 tempdir")
            .to_string();

        assert!(
            !transcript_is_live_in(
                home.path(),
                &escaping_id,
                std::path::Path::new("/repo/original"),
                SystemTime::now()
            ),
            "a path-escaping id must never be probed, even when the target file is fresh"
        );
    }
}
