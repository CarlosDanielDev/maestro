//! Binary-level E2E: the headless team-launch workspace trust gate (#1108).
//!
//! Spawns the real binary in a temp workspace whose `maestro.toml` declares a
//! stub agent `command` (an executable-bearing key) and a project-tier preset
//! whose reviewer role routes to that stub. The trust refusal must fire on the
//! real dispatch chain — `launch_headless` → `ProductionSchedulerRunner::
//! run_issue` → `ensure_launch_config_trusted` — BEFORE the configured command
//! is ever spawned; the trusted counterpart must proceed into dispatch.
//!
//! Existing tests pin the seam (`ensure_launch_config_trusted`) only; this
//! file pins the wiring, so dropping the call at `src/commands/team_launch.rs`
//!'s `run_issue` flips `team_launch_refuses_untrusted_config_before_spawning_
//! command` red (the stub runs and the launch succeeds).
//!
//! Offline: no `gh`, no network; the stub emits canned stream-json. Unix only
//! (the stub is a shell script). Pattern mirrors `tests/e2e_run_once.rs`.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

/// Valid-UUID-shaped session id the stream-json parser accepts.
const STUB_SESSION_ID: &str = "e2e00000-0000-0000-0000-000000000001";

/// Installs the stub agent `command`. Touches `marker` when executed, so
/// "marker exists" is direct evidence the config-built provider was built and
/// spawned. Emits the stream-json a Claude-style provider produces, with an
/// assistant text that is a `generic` `SubagentResult` (allowed for the
/// Reviewer role), so a dispatched `single-pass` machine reaches Done.
fn install_stub(dir: &Path) -> PathBuf {
    let dst = dir.join("marker-stub.sh");
    // TempDir paths never contain single quotes on macOS/Linux; still, keep
    // the marker reference quoted so a relocated temp root cannot break it.
    let script = format!(
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "marker-stub 1.0.0"
  exit 0
fi
touch '{marker}'
echo '{{"type":"system","subtype":"init","session_id":"{session}"}}'
echo '{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"{{\\"kind\\":\\"generic\\",\\"json\\":{{}}}}"}}]}}}}'
echo '{{"type":"result","subtype":"success","result":"done","session_id":"{session}"}}'
exit 0
"#,
        marker = dir.join("marker").display(),
        session = STUB_SESSION_ID,
    );
    fs::write(&dst, script).expect("write stub agent script");
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&dst, fs::Permissions::from_mode(0o755)).expect("chmod stub agent");
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
"#
    );
    fs::write(dir.join("maestro.toml"), toml).expect("write maestro.toml");
}

/// Project-tier preset (highest loader priority, co-located with the
/// workspace) dispatching a single `single-pass` Reviewer call through the
/// stub agent. `single-pass` is the smallest machine that reaches dispatch,
/// so the gate is the only thing standing between launch and spawn.
fn write_preset(dir: &Path) {
    let teams = dir.join(".maestro").join("teams");
    fs::create_dir_all(&teams).expect("create project-tier teams dir");
    fs::write(
        teams.join("trust-e2e.toml"),
        r#"extends = ""
primitive = "single-pass"
reviewer = "stub"
"#,
    )
    .expect("write project-tier preset");
}

/// Workspace trust gate (#1045): runs `maestro trust --yes` with the same
/// hermetic HOME so the record lands where the launch below reads it.
fn trust_workspace(dir: &Path, home: &Path) {
    let output = Command::new(env!("CARGO_BIN_EXE_maestro"))
        .args(["trust", "--yes"])
        .current_dir(dir)
        .env("HOME", home)
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .output()
        .expect("spawn maestro trust");
    assert!(
        output.status.success(),
        "maestro trust failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Runs `maestro team launch --yes` in `dir` with a hermetic HOME/XDG set and
/// inherited `MAESTRO_*` vars removed, so the developer shell cannot leak in.
/// The stub is referenced by absolute path, so no real agent binary is used.
fn run_launch(dir: &Path, home: &Path) -> (std::process::ExitStatus, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_maestro"));
    cmd.args(["team", "launch", "--yes", "trust-e2e", "--issue", "7"])
        .current_dir(dir)
        .env("HOME", home)
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .stdin(std::process::Stdio::null());
    for (key, _) in std::env::vars().filter(|(k, _)| k.starts_with("MAESTRO_")) {
        cmd.env_remove(key);
    }
    let output = cmd.output().expect("spawn maestro binary");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (output.status, stdout, stderr)
}

fn setup_workspace() -> (TempDir, TempDir) {
    let work = TempDir::new().expect("temp work dir");
    let home = TempDir::new().expect("temp home dir");
    let stub = install_stub(work.path());
    write_config(work.path(), &stub);
    write_preset(work.path());
    (work, home)
}

/// The wiring regression (#1108): an untrusted workspace with an
/// executable-bearing config must be refused on the real dispatch chain
/// before the configured command is ever spawned. Mutation check: dropping
/// the `ensure_launch_config_trusted` call in `run_issue` makes this fail —
/// the stub runs (marker appears) and the launch succeeds.
#[test]
fn team_launch_refuses_untrusted_config_before_spawning_command() {
    let (work, home) = setup_workspace();
    let (status, stdout, stderr) = run_launch(work.path(), home.path());
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        !status.success(),
        "untrusted launch must exit nonzero:\n{combined}"
    );
    assert!(
        combined.contains("not trusted"),
        "refusal must name the trust gate:\n{combined}"
    );
    assert!(
        !work.path().join("marker").exists(),
        "stub command must never be executed on refusal:\n{combined}"
    );
}

/// Trusted counterpart: same workspace and config, recorded via
/// `maestro trust --yes`. The launch must get past the gate and actually
/// dispatch the configured command (marker appears) and reach Succeeded.
#[test]
fn team_launch_trusted_config_proceeds_to_dispatch() {
    let (work, home) = setup_workspace();
    trust_workspace(work.path(), home.path());
    let (status, stdout, stderr) = run_launch(work.path(), home.path());
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        work.path().join("marker").exists(),
        "trusted launch must dispatch the configured command:\n{combined}"
    );
    assert!(status.success(), "trusted launch must succeed:\n{combined}");
    assert!(
        stdout.contains("1 succeeded"),
        "single-pass issue must reach Succeeded:\n{combined}"
    );
}
