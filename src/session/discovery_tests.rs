//! Tests for [`crate::session::discovery`] (#1052). The checked-in fixture
//! tree under `tests/fixtures/transcripts/home/` mirrors a real
//! `~/.claude/projects/` layout: 2 project dirs, 3 transcripts, two with an
//! `ai-title` line and one falling back to its first user message. Tests that
//! need to mutate mtimes or add broken files build their own tree in a
//! tempdir instead.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use tempfile::tempdir;

use super::scan_transcripts;

const SESSION_A: &str = "aaaa1111-1111-1111-1111-111111111111";
const SESSION_B: &str = "bbbb2222-2222-2222-2222-222222222222";
const SESSION_C: &str = "cccc3333-3333-3333-3333-333333333333";

/// Checked-in fixture: 2 projects, 3 transcripts, one without `ai-title`.
fn fixture_home() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transcripts/home")
}

/// One real-shaped transcript: metadata line, a user line carrying `cwd` and
/// `sessionId`, and (optionally) an `ai-title` line.
fn write_transcript(
    home: &Path,
    project_dir: &str,
    session_id: &str,
    cwd: &str,
    title: Option<&str>,
) -> PathBuf {
    let dir = home.join(".claude/projects").join(project_dir);
    fs::create_dir_all(&dir).unwrap();
    let mut body = format!(
        "{{\"type\":\"user\",\"cwd\":\"{cwd}\",\"sessionId\":\"{session_id}\",\
         \"uuid\":\"u1\",\"parentUuid\":null,\"userType\":\"external\",\
         \"timestamp\":\"2026-06-10T05:00:00.000Z\",\
         \"message\":{{\"role\":\"user\",\"content\":\"seed prompt for {session_id}\"}}}}\n"
    );
    if let Some(title) = title {
        body.push_str(&format!(
            "{{\"type\":\"ai-title\",\"aiTitle\":\"{title}\",\
             \"sessionId\":\"{session_id}\"}}\n"
        ));
    }
    let path = dir.join(format!("{session_id}.jsonl"));
    fs::write(&path, body).unwrap();
    path
}

#[test]
fn scan_finds_sessions_with_id_cwd_title_and_mtime() {
    let sessions = scan_transcripts(&fixture_home());
    assert_eq!(sessions.len(), 3, "got: {sessions:?}");

    let by_id: std::collections::HashMap<&str, _> = sessions
        .iter()
        .map(|s| (s.session_id.as_str(), s))
        .collect();

    let a = by_id.get(SESSION_A).unwrap();
    assert_eq!(a.title, "Alpha plan review", "ai-title must win");
    assert_eq!(a.cwd, Path::new("/tmp/proj/alpha"));
    let mtime = fs::metadata(fixture_home().join(format!(
        ".claude/projects/-tmp-proj-alpha/{SESSION_A}.jsonl"
    )))
    .unwrap()
    .modified()
    .unwrap();
    assert_eq!(a.last_active, mtime, "last_active must be the file mtime");

    let b = by_id.get(SESSION_B).unwrap();
    assert_eq!(
        b.title, "Fix the login bug when the password has trailing spaces and…",
        "no ai-title: first user message truncated"
    );

    let c = by_id.get(SESSION_C).unwrap();
    assert_eq!(c.title, "Elsewhere notes");
}

#[test]
fn discovery_scan_roundtrips_cwd_from_lines() {
    // The fixture project dir `-tmp-elsewhere` cannot round-trip: both
    // `/tmp/elsewhere` and `/tmp/else.where` munge to it. The lines are the
    // authority, and they say `/tmp/else.where`.
    let sessions = scan_transcripts(&fixture_home());
    let c = sessions.iter().find(|s| s.session_id == SESSION_C).unwrap();
    assert_eq!(c.cwd, Path::new("/tmp/else.where"));
}

#[test]
fn scan_orders_by_last_active_desc() {
    let home = tempdir().unwrap();
    let old = write_transcript(home.path(), "p-old", SESSION_A, "/tmp/p", Some("Old"));
    let mid = write_transcript(home.path(), "p-mid", SESSION_B, "/tmp/p", Some("Mid"));
    let new = write_transcript(home.path(), "p-new", SESSION_C, "/tmp/p", Some("New"));
    for (path, secs) in [&old, &mid, &new].iter().zip([10u64, 20, 30]) {
        fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(secs)),
            )
            .unwrap();
    }

    let sessions = scan_transcripts(home.path());
    let ids: Vec<&str> = sessions.iter().map(|s| s.session_id.as_str()).collect();
    assert_eq!(ids, vec![SESSION_C, SESSION_B, SESSION_A]);
}

#[test]
fn scan_skips_transcripts_without_session_id_or_cwd() {
    let home = tempdir().unwrap();
    let projects = home.path().join(".claude/projects/broken");
    fs::create_dir_all(&projects).unwrap();
    // No sessionId: cannot be identified or resumed.
    fs::write(
        projects.join("no-session.jsonl"),
        "{\"type\":\"user\",\"cwd\":\"/tmp/x\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n",
    )
    .unwrap();
    // No cwd: #1051 resumes by cwd, so the entry is unusable.
    fs::write(
        projects.join("no-cwd.jsonl"),
        "{\"type\":\"mode\",\"mode\":\"normal\",\"sessionId\":\"dddd4444-4444-4444-4444-444444444444\"}\n",
    )
    .unwrap();
    // Not a transcript.
    fs::write(projects.join("notes.txt"), "not jsonl\n").unwrap();
    // Wrong level: transcripts live under project dirs, not at the root.
    fs::write(
        home.path().join(".claude/projects/stray.jsonl"),
        "{\"type\":\"mode\",\"sessionId\":\"eeee5555-5555-5555-5555-555555555555\",\"cwd\":\"/tmp/x\"}\n",
    )
    .unwrap();

    assert!(
        scan_transcripts(home.path()).is_empty(),
        "incomplete or non-transcript files must be skipped"
    );
}

#[test]
fn scan_missing_transcript_root_yields_empty() {
    let home = tempdir().unwrap();
    assert!(scan_transcripts(home.path()).is_empty());
}

#[test]
fn scan_120_transcripts_completes_under_1s() {
    let home = tempdir().unwrap();
    let mut expected = 0;
    for d in 0..20 {
        for s in 0..6 {
            write_transcript(
                home.path(),
                &format!("proj-{d}"),
                &format!("00000000-0000-4000-8000-{d:012}{s:012}"),
                &format!("/tmp/proj-{d}"),
                if s % 2 == 0 { Some("Perf title") } else { None },
            );
            expected += 1;
        }
    }
    assert_eq!(expected, 120);

    let start = Instant::now();
    let sessions = scan_transcripts(home.path());
    let elapsed = start.elapsed();
    assert_eq!(sessions.len(), 120, "every transcript must be found");
    assert!(
        elapsed < Duration::from_secs(1),
        "scan of 120 transcripts took {elapsed:?}, bound is 1s"
    );
}

/// Gate for issue #1052's "no writes" item: the impl half of `discovery.rs`
/// (everything before the `#[cfg(test)]` block) must not contain any
/// `write`/`create` call — the module only reads.
#[test]
fn module_impl_has_no_write_or_create_calls() {
    let src = include_str!("discovery.rs");
    let impl_src = src.split("#[cfg(test)]").next().unwrap();
    assert!(
        !impl_src.contains("write"),
        "discovery impl must not write: found `write`"
    );
    assert!(
        !impl_src.contains("create"),
        "discovery impl must not write: found `create`"
    );
}
