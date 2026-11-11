//! Tests for [`crate::session::logger`] (#1031). Extracted into a sibling
//! file via `#[path]` so `logger.rs` stays under the 400-line gate cap
//! (`scripts/check-file-size.sh`).

use super::*;
use tempfile::TempDir;

fn make_logger() -> (SessionLogger, TempDir) {
    let dir = TempDir::new().unwrap();
    let logger = SessionLogger::new(dir.path().to_path_buf());
    (logger, dir)
}

#[test]
fn log_event_creates_file() {
    let (logger, _dir) = make_logger();
    let id = Uuid::new_v4();
    let event = StreamEvent::AssistantMessage {
        text: "hello".into(),
    };
    logger.log_event(id, &event).unwrap();
    assert!(logger.log_path(id).exists());
}

#[test]
fn log_event_appends_entries() {
    let (logger, _dir) = make_logger();
    let id = Uuid::new_v4();
    logger
        .log_event(
            id,
            &StreamEvent::AssistantMessage {
                text: "first".into(),
            },
        )
        .unwrap();
    logger
        .log_event(
            id,
            &StreamEvent::AssistantMessage {
                text: "second".into(),
            },
        )
        .unwrap();
    let content = logger.read_log(id).unwrap();
    assert!(content.contains("first"));
    assert!(content.contains("second"));
}

#[test]
fn log_event_tool_use() {
    let (logger, _dir) = make_logger();
    let id = Uuid::new_v4();
    let event = StreamEvent::ToolUse {
        tool: "Write".into(),
        file_path: Some("src/main.rs".into()),
        command_preview: None,
        subagent_name: None,
    };
    logger.log_event(id, &event).unwrap();
    let content = logger.read_log(id).unwrap();
    assert!(content.contains("TOOL: Write (src/main.rs)"));
}

#[test]
fn log_event_completed() {
    let (logger, _dir) = make_logger();
    let id = Uuid::new_v4();
    let event = StreamEvent::Completed { cost_usd: 1.23 };
    logger.log_event(id, &event).unwrap();
    let content = logger.read_log(id).unwrap();
    assert!(content.contains("COMPLETED: $1.23"));
}

#[test]
fn log_event_error() {
    let (logger, _dir) = make_logger();
    let id = Uuid::new_v4();
    let event = StreamEvent::Error {
        message: "something broke".into(),
    };
    logger.log_event(id, &event).unwrap();
    let content = logger.read_log(id).unwrap();
    assert!(content.contains("ERROR: something broke"));
}

#[test]
fn list_logs_returns_summaries() {
    let (logger, _dir) = make_logger();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    logger
        .log_event(id1, &StreamEvent::AssistantMessage { text: "a".into() })
        .unwrap();
    logger
        .log_event(id2, &StreamEvent::AssistantMessage { text: "b".into() })
        .unwrap();
    let logs = logger.list_logs().unwrap();
    assert_eq!(logs.len(), 2);
}

#[test]
fn list_logs_empty_dir() {
    let (logger, _dir) = make_logger();
    let logs = logger.list_logs().unwrap();
    assert!(logs.is_empty());
}

#[test]
fn read_log_nonexistent_returns_err() {
    let (logger, _dir) = make_logger();
    assert!(logger.read_log(Uuid::new_v4()).is_err());
}

/// Backdate a file's mtime past the default 30-day retention window.
fn make_old(path: &std::path::Path) {
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(31 * 24 * 3600);
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .expect("file must exist to backdate it");
    file.set_times(std::fs::FileTimes::new().set_modified(old))
        .expect("set mtime");
}

#[test]
fn cleanup_removes_only_old_uuid_log_files() {
    let (logger, dir) = make_logger();
    let root = dir.path();

    // Old <uuid>.log — the only thing this module writes — goes.
    let stale_log = root.join(format!("{}.log", Uuid::new_v4()));
    fs::write(&stale_log, "old session").unwrap();
    make_old(&stale_log);

    // Fresh <uuid>.log stays.
    let fresh_log = root.join(format!("{}.log", Uuid::new_v4()));
    fs::write(&fresh_log, "live session").unwrap();

    // Everything else stays, however old: wrong extension, wrong stem,
    // or both.
    let keep_txt = root.join("keep.txt");
    fs::write(&keep_txt, "not a log").unwrap();
    make_old(&keep_txt);
    let keep_stem = root.join("not-a-uuid.log");
    fs::write(&keep_stem, "wrong stem").unwrap();
    make_old(&keep_stem);

    let removed = logger.cleanup_old_logs(30).unwrap();
    assert_eq!(removed, 1, "only the old <uuid>.log is removed");
    assert!(fresh_log.exists());
    assert!(keep_txt.exists());
    assert!(keep_stem.exists());
    assert!(!stale_log.exists());
}

#[test]
fn cleanup_returns_ok_zero_when_log_dir_missing() {
    let (logger, _dir) = make_logger();
    assert_eq!(logger.cleanup_old_logs(30).unwrap(), 0);
}

// #1031: a cloned repo can commit `.maestro/logs` as a symlink; git
// preserves symlinks. The GC must refuse to even walk such a root so a
// hostile checkout cannot point it at, say, `~/.ssh`.
#[cfg(unix)]
#[test]
fn cleanup_refuses_symlinked_log_dir() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    let victim = root.join("victim");
    fs::create_dir(&victim).unwrap();
    let victim_file = victim.join("id_rsa");
    fs::write(&victim_file, "private key").unwrap();
    make_old(&victim_file);

    let logs_link = root.join(".maestro").join("logs");
    fs::create_dir_all(logs_link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&victim, &logs_link).unwrap();

    let logger = SessionLogger::new(logs_link.clone());
    let removed = logger.cleanup_old_logs(30).unwrap();
    assert_eq!(removed, 0, "symlinked log root must clean nothing");
    assert!(
        victim_file.exists(),
        "file behind the symlinked root must survive"
    );
}

#[cfg(unix)]
#[test]
fn cleanup_refuses_non_directory_log_root() {
    let dir = TempDir::new().unwrap();
    let logs_path = dir.path().join(".maestro").join("logs");
    fs::create_dir_all(logs_path.parent().unwrap()).unwrap();
    fs::write(&logs_path, "just a file, not a dir").unwrap();
    make_old(&logs_path);

    let logger = SessionLogger::new(logs_path.clone());
    let removed = logger.cleanup_old_logs(30).unwrap();
    assert_eq!(removed, 0);
    assert!(logs_path.exists(), "non-dir root must survive untouched");
}

// Even inside a real log dir, a symlinked entry must be skipped: its
// DirEntry file type is a symlink, not a regular file, so removing it
// would unlink a path this module did not write.
#[cfg(unix)]
#[test]
fn cleanup_skips_symlinked_entries_inside_log_dir() {
    let (logger, dir) = make_logger();
    let root = dir.path();

    let victim_file = root.join("outside-target.log");
    fs::write(&victim_file, "points outside").unwrap();
    make_old(&victim_file);

    let link = root.join(format!("{}.log", Uuid::new_v4()));
    std::os::unix::fs::symlink(&victim_file, &link).unwrap();

    let removed = logger.cleanup_old_logs(30).unwrap();
    assert_eq!(removed, 0);
    assert!(victim_file.exists());
    assert!(
        link.exists(),
        "symlinked entry must be skipped, not removed"
    );
}
