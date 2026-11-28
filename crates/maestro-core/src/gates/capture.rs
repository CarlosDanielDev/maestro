//! Run a gate command and keep what it printed (#1291).
//!
//! Gates used to keep 200 bytes of stderr and drop stdout, so a failing
//! `cargo test` never said which tests failed. Now stdout + stderr are
//! secret-masked, the last 64 KiB stay on the result, and the full text is
//! written to `.maestro/logs/gates/<worktree>/<gate>.log`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use super::types::GateResult;
use crate::provider::github::ci::truncate_log;
use crate::provider::github::redaction::redact_secrets;
use crate::util::truncate_at_char_boundary;

/// Bytes of output kept on a failing gate result (and in the state file).
pub const OUTPUT_TAIL_BYTES: usize = 64 * 1024;

/// Default root for full gate logs, relative to the repo root like
/// `SessionLogger::default_dir`.
pub fn default_log_dir() -> PathBuf {
    PathBuf::from(".maestro").join("logs").join("gates")
}

/// What one gate command printed.
pub(super) struct Run {
    pub success: bool,
    /// Masked stderr, for the one-line `message`.
    pub stderr: String,
    /// Masked stdout + stderr, untruncated.
    pub text: String,
    pub duration_ms: u64,
}

pub(super) fn run(program: &str, args: &[String], cwd: &Path) -> std::io::Result<Run> {
    let started = Instant::now();
    let output = Command::new(program).args(args).current_dir(cwd).output()?;
    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let sep = if stdout.is_empty() || stdout.ends_with('\n') || stderr.is_empty() {
        ""
    } else {
        "\n"
    };
    Ok(Run {
        success: output.status.success(),
        stderr: redact_secrets(&stderr),
        text: redact_secrets(&format!("{stdout}{sep}{stderr}")),
        duration_ms,
    })
}

/// The first 200 bytes of stderr, for the activity-log line.
pub(super) fn short(stderr: &str) -> String {
    let end = truncate_at_char_boundary(stderr, 200);
    let suffix = if end < stderr.len() { "..." } else { "" };
    format!("{}{}", &stderr[..end], suffix)
}

/// Attach command, duration, output tail (failures only) and the full log.
pub(super) fn attach(
    mut result: GateResult,
    command: &str,
    run: &Run,
    log: Option<(&Path, &Path)>,
) -> GateResult {
    result.command = command.to_string();
    result.duration_ms = run.duration_ms;
    if !result.passed {
        result.output = truncate_log(&run.text, OUTPUT_TAIL_BYTES);
    }
    result.log_path = log.and_then(|(dir, wt)| write_log(dir, wt, &result.gate, &run.text));
    result
}

/// Write `text` to `<dir>/<worktree name>/<gate>.log`. Both names are
/// reduced to `[A-Za-z0-9_-]` so a gate name from config can't climb out
/// of `dir`. A failed write only means no log path.
fn write_log(dir: &Path, worktree: &Path, gate: &str, text: &str) -> Option<PathBuf> {
    let wt = worktree
        .file_name()
        .map(|n| file_safe(&n.to_string_lossy()))
        .unwrap_or_else(|| "worktree".into());
    let sub = dir.join(wt);
    let path = sub.join(format!("{}.log", file_safe(gate)));
    let written = std::fs::create_dir_all(&sub).and_then(|_| std::fs::write(&path, text));
    match written {
        Ok(()) => Some(path),
        Err(e) => {
            tracing::warn!("gate log not written to {}: {}", path.display(), e);
            None
        }
    }
}

fn file_safe(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() { "gate".into() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gates::runner::run_gate;
    use crate::gates::types::CompletionGate;

    fn sh(name: &str, script: &str) -> CompletionGate {
        CompletionGate::Command {
            name: name.into(),
            command: format!("sh -c '{script}'"),
            required: true,
        }
    }

    #[test]
    fn gate_result_keeps_both_streams() {
        let wt = tempfile::tempdir().unwrap();
        let logs = tempfile::tempdir().unwrap();
        // 100 KB of stdout, then a stdout tail line and a stderr line.
        let script = "yes x | head -c 100000; echo; echo OUT-TAIL; echo ERR-LINE >&2; exit 1";
        let gate = sh("test", script);
        let r = run_gate(&gate, wt.path(), Some(logs.path()));
        assert!(!r.passed);
        assert!(r.output.contains("OUT-TAIL"), "stdout kept: {}", r.message);
        assert!(r.output.contains("ERR-LINE"), "stderr kept");
        assert!(
            r.output.len() <= OUTPUT_TAIL_BYTES + "...(truncated)\n".len(),
            "tail capped at 64 KiB, got {}",
            r.output.len()
        );
        assert!(r.output.starts_with("...(truncated)"));
        assert_eq!(r.command, format!("sh -c '{script}'"));
        let log = r.log_path.expect("full log written");
        let full = std::fs::read_to_string(&log).unwrap();
        assert!(full.len() > 100_000, "the log file is not truncated");
        assert!(log.starts_with(logs.path()));
    }

    #[test]
    fn passing_gate_keeps_command_and_duration_but_no_output() {
        let wt = tempfile::tempdir().unwrap();
        let r = run_gate(&sh("fmt", "echo fine"), wt.path(), None);
        assert!(r.passed);
        assert!(r.output.is_empty(), "state keeps output for failures only");
        assert_eq!(r.command, "sh -c 'echo fine'");
        assert!(r.log_path.is_none(), "no log dir, no log");
    }

    #[test]
    fn gate_output_masks_secrets() {
        let wt = tempfile::tempdir().unwrap();
        let logs = tempfile::tempdir().unwrap();
        let gate = sh("leak", "echo token ghp_abcDEF123; exit 1");
        let r = run_gate(&gate, wt.path(), Some(logs.path()));
        assert!(r.output.contains("[REDACTED]"));
        assert!(!r.output.contains("ghp_abcDEF123"));
        let full = std::fs::read_to_string(r.log_path.unwrap()).unwrap();
        assert!(
            !full.contains("ghp_abcDEF123"),
            "the disk log is masked too"
        );
    }

    /// New secret shapes (#1445) must be masked both in the state tail
    /// (`GateResult::output`) and in the on-disk gate log.
    #[test]
    fn gate_masks_new_secret_shapes_in_state_tail_and_disk_log() {
        let wt = tempfile::tempdir().unwrap();
        let logs = tempfile::tempdir().unwrap();
        let gate = sh(
            "leak2",
            "echo ANTHROPIC_API_KEY=sk-ant-api03-1234abcdefABCDEF-ghijklmnopq; echo token AKIAIOSFODNN7EXAMPLE; exit 1",
        );
        let r = run_gate(&gate, wt.path(), Some(logs.path()));
        assert!(!r.passed);
        assert!(r.output.contains("[REDACTED]"));
        assert!(
            !r.output
                .contains("sk-ant-api03-1234abcdefABCDEF-ghijklmnopq")
        );
        assert!(!r.output.contains("AKIAIOSFODNN7EXAMPLE"));
        let full = std::fs::read_to_string(r.log_path.unwrap()).unwrap();
        assert!(!full.contains("sk-ant-api03-1234abcdefABCDEF-ghijklmnopq"));
        assert!(
            !full.contains("AKIAIOSFODNN7EXAMPLE"),
            "disk log masked too"
        );
    }

    #[test]
    fn gate_log_name_cannot_leave_the_log_dir() {
        let wt = tempfile::tempdir().unwrap();
        let logs = tempfile::tempdir().unwrap();
        let r = run_gate(&sh("../../escape", "exit 1"), wt.path(), Some(logs.path()));
        let log = r.log_path.expect("log written");
        assert_eq!(log.parent().unwrap().parent().unwrap(), logs.path());
        assert!(!log.to_string_lossy().contains(".."));
    }
}
