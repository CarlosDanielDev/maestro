//! Binary-level E2E: `maestro run --once` over the interactive (PTY) transport.
//!
//! #1050: the in-process PTY coverage (`interactive_session_tests.rs`) cannot
//! see the binary around it. This test launches the real binary with the stub
//! agent configured `transport = "interactive"`, so the session's stream-json
//! request rides the same PTY path the Interaction screen uses. The stub
//! speaks ONLY through its transcript file (its stdout stays empty), so a
//! run that falls back to the headless transport completes nothing and this
//! test times out — passing proves the PTY transport ran.
//!
//! Also asserts the completion teardown path: the session's own worktree and
//! branch are removed while an unrelated decoy worktree survives. Unix only
//! (the stub is a shell script and the transport is PTY-backed).

#![cfg(unix)]

use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

const STUB_REPLY: &str = "hello from stub";
const RUN_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Runs `git` in `dir` and returns stdout; panics on failure so setup errors
/// surface with the tool's own message instead of a confusing assert later.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("run git {args:?}: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Session worktree creation is `git worktree add`, so the work dir must be a
/// real repo with at least one commit before maestro starts.
fn init_git_repo(dir: &Path) {
    git(dir, &["init"]);
    git(dir, &["config", "user.email", "e2e@example.invalid"]);
    git(dir, &["config", "user.name", "e2e"]);
    fs::write(dir.join("README.md"), "# e2e\n").expect("write README.md");
    git(dir, &["add", "."]);
    git(dir, &["commit", "-m", "init"]);
}

/// A second `maestro/*` worktree that no session owns. The teardown must
/// remove the session's worktree and branch WITHOUT touching this one.
fn create_decoy_worktree(dir: &Path) {
    git(
        dir,
        &[
            "worktree",
            "add",
            ".maestro/worktrees/decoy",
            "-b",
            "maestro/decoy",
        ],
    );
}

fn install_stub(dir: &Path) -> PathBuf {
    let src =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/claude-transcript/stub-claude.sh");
    let dst = dir.join("stub-claude.sh");
    fs::copy(&src, &dst).expect("copy stub claude fixture");
    fs::set_permissions(&dst, fs::Permissions::from_mode(0o755)).expect("chmod stub claude");
    dst
}

fn write_config(dir: &Path, stub: &Path) {
    // Escape the path as a TOML string; a bare `format!` breaks on quotes or backslashes.
    let command = toml::Value::String(stub.display().to_string()).to_string();
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
command = {command}
transport = "interactive"
"#
    );
    fs::write(dir.join("maestro.toml"), toml).expect("write maestro.toml");
}

/// Runs `maestro run --prompt hello --once --skip-doctor` in `dir`, killing it after `RUN_TIMEOUT`.
///
/// Hermetic: HOME and XDG dirs point under `home` (the interactive transport
/// locates the stub transcript via HOME), and inherited `MAESTRO_*` vars are
/// removed, so the developer shell cannot leak in. stdout/stderr are files,
/// not pipes, so `cargo test` cannot hang on a full pipe.
fn run_maestro_once(dir: &Path, home: &Path) -> ExitStatus {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_maestro"));
    cmd.args(["run", "--prompt", "hello", "--once", "--skip-doctor"])
        .current_dir(dir)
        .env("HOME", home)
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .stdin(Stdio::null())
        .stdout(File::create(dir.join("stdout.txt")).expect("create stdout.txt"))
        .stderr(File::create(dir.join("stderr.txt")).expect("create stderr.txt"));
    for (key, _) in std::env::vars().filter(|(k, _)| k.starts_with("MAESTRO_")) {
        cmd.env_remove(key);
    }
    let mut child = cmd.spawn().expect("spawn maestro binary");

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

/// Finds a completed session in `maestro-state.json`. The app may create a
/// second record (hollow-completion retry), so this accepts any match rather
/// than requiring exactly one. Completion itself is the PTY proof: the stub
/// prints nothing on stdout, so the headless transport can never complete.
fn find_completed_session(dir: &Path) -> serde_json::Value {
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
        .find(|s| s["status"] == "completed")
        .cloned()
        .unwrap_or_else(|| panic!("no completed session:\n{state_text}"))
}

/// Collects the stub transcripts under
/// `$HOME/.claude/projects/<munged-cwd>/<id>.jsonl`, without assuming the
/// munging rule or the bound conversation id (the state file may miss it:
/// once mode can exit before the session-bound notice lands).
fn stub_transcripts(home: &Path) -> Vec<PathBuf> {
    let projects = home.join(".claude").join("projects");
    let mut found = Vec::new();
    for provider_dir in
        fs::read_dir(&projects).unwrap_or_else(|e| panic!("read {}: {e}", projects.display()))
    {
        let provider_dir = provider_dir.expect("read projects entry").path();
        for entry in fs::read_dir(&provider_dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", provider_dir.display()))
        {
            let path = entry.expect("read transcript entry").path();
            if path.is_file() && path.to_string_lossy().ends_with(".jsonl") {
                found.push(path);
            }
        }
    }
    found
}

#[test]
fn run_once_completes_over_pty_transport_and_teardowns_worktree() {
    let work = TempDir::new().expect("temp work dir");
    let home = TempDir::new().expect("temp home dir");
    let work = work.path().canonicalize().expect("canonicalize work dir");
    let home = home.path().canonicalize().expect("canonicalize home dir");

    init_git_repo(&work);
    create_decoy_worktree(&work);
    let stub = install_stub(&work);
    write_config(&work, &stub);

    let status = run_maestro_once(&work, &home);
    let stdout = read_or_empty(&work.join("stdout.txt"));
    let stderr = read_or_empty(&work.join("stderr.txt"));
    assert!(
        status.success(),
        "maestro exited {status}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    // The reply must land in the session's log, exactly as it does on the
    // headless transport — same event funnel, different wire.
    let session = find_completed_session(&work);
    let session_id = session["id"].as_str().expect("session has id");
    let log_path = work.join(".maestro/logs").join(format!("{session_id}.log"));
    let log = read_or_empty(&log_path);
    assert!(
        log.contains(&format!("ASSISTANT: {STUB_REPLY}")),
        "{} has no `ASSISTANT: {STUB_REPLY}` line:\n{log}",
        log_path.display()
    );

    // The PTY stub must have recorded the turn in its transcript (only the
    // interactive transport pins a `--session-id`, so a conversation exists
    // only when the session rode the PTY).
    let transcripts = stub_transcripts(&home);
    assert_eq!(
        transcripts.len(),
        1,
        "exactly one stub conversation expected: {transcripts:?}"
    );
    let transcript_text = read_or_empty(&transcripts[0]);
    assert!(
        transcript_text.contains(STUB_REPLY),
        "{} has no `{STUB_REPLY}` entry:\n{transcript_text}",
        transcripts[0].display()
    );

    // Teardown only means something if the session had a worktree: the
    // activity log must show maestro created one (`git worktree add` ran).
    // Without this guard, a silently skipped worktree (git failure) would let
    // the teardown assertions below pass while exercising nothing.
    let created = session["activity_log"]
        .as_array()
        .expect("session has activity log")
        .iter()
        .any(|e| {
            e["message"]
                .as_str()
                .is_some_and(|m| m.starts_with("Worktree created:"))
        });
    assert!(
        created,
        "session never created a worktree; teardown not exercised"
    );

    // Teardown: the session's worktree and branch are gone; the decoy's
    // remain. Session slugs are `session-<id8>`, so any surviving
    // `maestro/session-*` means teardown skipped the session's worktree.
    let worktree_dir = work.join(".maestro/worktrees");
    let mut remaining: Vec<String> = fs::read_dir(&worktree_dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", worktree_dir.display()))
        .map(|entry| {
            entry
                .expect("read worktrees entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    remaining.sort();
    assert_eq!(
        remaining,
        vec!["decoy".to_string()],
        "worktrees dir must hold only the decoy after teardown"
    );
    let branches = git(&work, &["branch", "--list", "maestro/*"]);
    let branch_names: Vec<String> = branches
        .lines()
        .map(|line| {
            // `+`/`*` mark checked-out branches (the decoy lives in its own
            // worktree); strip the marker, not the name.
            line.trim()
                .trim_start_matches(['+', '*'])
                .trim()
                .to_string()
        })
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        branch_names,
        vec!["maestro/decoy".to_string()],
        "only the decoy branch may survive teardown"
    );
}
