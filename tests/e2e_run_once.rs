//! Binary-level E2E: `maestro run --once` against a stub agent.
//!
//! Spawns the real binary in a temp dir with a stub agent script. Offline:
//! `--prompt` + `--skip-doctor`, no `gh`, no network. Unix only (the stub is a shell script).
#![cfg(unix)]

use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

const STUB_SESSION_ID: &str = "e2e00000-0000-0000-0000-000000000001";
const RUN_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

fn install_stub(dir: &Path) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/e2e_stub_agent.sh");
    let dst = dir.join("stub-agent.sh");
    fs::copy(&src, &dst).expect("copy stub agent fixture");
    fs::set_permissions(&dst, fs::Permissions::from_mode(0o755)).expect("chmod stub agent");
    dst
}

fn write_config(dir: &Path, stub: &Path) {
    let toml = format!(
        r#"[project]
repo = "owner/repo"

[sessions]

[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80

[github]

[notifications]

[gates]
enabled = false

[agents]
default = "stub"

[agents.stub]
kind = "claude"
command = "{}"
"#,
        stub.display()
    );
    fs::write(dir.join("maestro.toml"), toml).expect("write maestro.toml");
}

/// Runs `maestro run --prompt hello --once --skip-doctor` in `dir`, killing it after `RUN_TIMEOUT`.
fn run_maestro_once(dir: &Path, home: &Path) -> ExitStatus {
    let mut child = Command::new(env!("CARGO_BIN_EXE_maestro"))
        .args(["run", "--prompt", "hello", "--once", "--skip-doctor"])
        .current_dir(dir)
        .env("HOME", home)
        .stdin(Stdio::null())
        .stdout(File::create(dir.join("stdout.txt")).expect("create stdout.txt"))
        .stderr(File::create(dir.join("stderr.txt")).expect("create stderr.txt"))
        .spawn()
        .expect("spawn maestro binary");

    let deadline = Instant::now() + RUN_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().expect("poll maestro child") {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "maestro run --once exceeded {RUN_TIMEOUT:?}; see {}",
                dir.display()
            );
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn read_or_empty(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

/// Finds a completed session bound to the stub's agent session id.
/// The app may create a second record (hollow-completion retry), so this
/// accepts any match rather than requiring exactly one session.
fn find_completed_stub_session(dir: &Path) -> serde_json::Value {
    let state_path = dir.join("maestro-state.json");
    let state_text = fs::read_to_string(&state_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", state_path.display()));
    let state: serde_json::Value =
        serde_json::from_str(&state_text).expect("maestro-state.json is valid JSON");
    let sessions = state["sessions"]
        .as_array()
        .expect("state has sessions array");

    sessions
        .iter()
        .find(|s| s["status"] == "completed" && s["agent_session_id"] == STUB_SESSION_ID)
        .cloned()
        .unwrap_or_else(|| {
            panic!("no completed session with agent_session_id {STUB_SESSION_ID}:\n{state_text}")
        })
}

#[test]
fn run_once_completes_offline_against_stub_agent() {
    let work = TempDir::new().expect("temp work dir");
    let home = TempDir::new().expect("temp home dir");
    let stub = install_stub(work.path());
    write_config(work.path(), &stub);

    let status = run_maestro_once(work.path(), home.path());
    let stdout = read_or_empty(&work.path().join("stdout.txt"));
    let stderr = read_or_empty(&work.path().join("stderr.txt"));
    assert!(
        status.success(),
        "maestro exited {status}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    let session = find_completed_stub_session(work.path());
    let session_id = session["id"].as_str().expect("session has id");

    let log_path = work
        .path()
        .join(".maestro/logs")
        .join(format!("{session_id}.log"));
    let log = read_or_empty(&log_path);
    assert!(
        log.contains("ASSISTANT:"),
        "{} has no ASSISTANT line:\n{log}",
        log_path.display()
    );
}
