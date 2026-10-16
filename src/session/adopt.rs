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
    modified_within(
        &transcript_path(Path::new(&home), cwd, id),
        now,
        ADOPT_QUIET_PERIOD,
    )
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
}
