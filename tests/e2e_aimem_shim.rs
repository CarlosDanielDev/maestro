//! Binary-level opt-in E2E: maestro through an ai-memory shim script (#1112).
//!
//! The epic seam (#1110, spike `docs/spikes/ai-memory.md`) is a per-agent shim
//! `[agents.<id>] command` that execs `ai-memory run --no-autowire
//! --executable <harness> <harness-name> "$@"`. These tests keep the
//! load-bearing properties of that seam honest as maestro and ai-memory
//! evolve, driving maestro's real transports over
//! `tests/fixtures/e2e_stub_agent.sh`:
//!
//! 1. health check: `<shim> --version` relays the stub's version line, exit 0
//!    — what `src/doctor.rs` runs against every configured agent command;
//! 2. headless turn: the shim's stdout stays byte-identical to the stub's own
//!    stream-json (`--no-autowire` keeps autowire banners off stdout), maestro
//!    extracts the stub's session id, and a stub exiting 3 keeps exit code 3;
//! 3. PTY-interactive: with `--executable` in the shim, a full interactive
//!    turn over the repo PTY backend completes and tears down within a bounded
//!    timeout — the post-exit chooser menu never blocks (spike scenario 3a/3g).
//!
//! Opt-in (`#[ignore]`): every run needs the `ai-memory` binary on PATH and a
//! local server; CI stays green without it. Run with:
//!
//! ```sh
//! MAESTRO_AIMEM_E2E=1 cargo test --test e2e_aimem_shim aimem_shim -- --ignored
//! ```
//!
//! Hermetic sandbox per test (no real harness, no provider cost, no host
//! writes): temp `HOME` and `CLAUDE_CONFIG_DIR`, temp data dir via
//! `AI_MEMORY_DATA_DIR`, an `ai-memory serve` bound to a random free port,
//! and the shim pointing `--executable` at the stub. `--no-autowire` is the
//! guard measured by spike scenario 2b: with it, the harness config home is
//! never written — the tests assert that, too.
#![cfg(unix)]

use std::fs::{self, File};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

const STUB_SESSION_ID: &str = "e2e00000-0000-0000-0000-000000000001";
const STUB_VERSION_LINE: &str = "stub-agent 1.0.0";
const RUN_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const SERVER_BOOT_TIMEOUT: Duration = Duration::from_secs(15);
/// The exact headless argv maestro builds (`build_stream_args`) for a plain
/// prompt; the byte-identity check compares stub-direct vs shim-relayed
/// output for it.
const HEADLESS_ARGS: &[&str] = &[
    "--print",
    "--verbose",
    "--output-format",
    "stream-json",
    "--model",
    "stub-model",
    "hello",
];

/// Panics with the install hint when `ai-memory` is not on PATH — the clear
/// failure the issue asks for instead of a confusing wrapper error later.
fn require_ai_memory() {
    let absent = Command::new("ai-memory")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .is_err();
    assert!(
        !absent,
        "ai-memory binary not found on PATH — this opt-in E2E needs it \
         (install per docs/spikes/ai-memory.md, then run: \
         cargo test --test e2e_aimem_shim aimem_shim -- --ignored)"
    );
}

/// One hermetic sandbox: git repo work dir, temp HOME (with the ai-memory
/// data dir inside), shim over the stub fixture, and a live ai-memory server
/// on a random free port. The server dies with the [`ServerGuard`] drop.
struct Sandbox {
    dir: PathBuf,
    home: PathBuf,
    shim: PathBuf,
    data_dir: PathBuf,
    server_url: String,
    _work: TempDir,
    _home: TempDir,
    _server: ServerGuard,
}

struct ServerGuard(std::process::Child);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Exit-3 variant of the stub fixture: same output, different final exit.
/// The last line of the fixture is the headless `exit 0`.
fn stub_script_for(kind: &str) -> String {
    let text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/e2e_stub_agent.sh"),
    )
    .expect("read stub agent fixture");
    match kind {
        "exit3" => {
            let idx = text.rfind("exit 0").expect("fixture ends with exit 0");
            let mut flipped = text;
            flipped.replace_range(idx..idx + "exit 0".len(), "exit 3");
            flipped
        }
        _ => text,
    }
}

fn install_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).expect("write script");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod script");
    path
}

fn init_git_repo(dir: &Path) {
    let run = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("run git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    run(&["init"]);
    run(&["config", "user.email", "e2e@example.invalid"]);
    run(&["config", "user.name", "e2e"]);
    fs::write(dir.join("README.md"), "# e2e\n").expect("write README.md");
    run(&["add", "."]);
    run(&["commit", "-m", "init"]);
}

impl Sandbox {
    /// `agent_kind` selects the stub behind the shim: `"success"` (fixture as
    /// is) or `"exit3"` (flipped final exit).
    fn new(agent_kind: &str) -> Sandbox {
        let work = TempDir::new().expect("temp work dir");
        let home_dir = TempDir::new().expect("temp home dir");
        let dir = work.path().canonicalize().expect("canonicalize work dir");
        let home = home_dir
            .path()
            .canonicalize()
            .expect("canonicalize home dir");
        init_git_repo(&dir);

        let stub = install_script(&dir, "stub-agent.sh", &stub_script_for(agent_kind));
        // The epic's shim recipe (#1110): maestro's `command` execs the
        // wrapper with autowire off and the real harness pinned to the stub.
        let shim = install_script(
            &dir,
            "aimem-shim.sh",
            &format!(
                "#!/bin/sh\nexec ai-memory run --no-autowire --executable {} claude \"$@\"\n",
                stub.display()
            ),
        );

        let data_dir = home.join("aimem-data");
        let port = free_port();
        let server_url = format!("http://127.0.0.1:{port}");
        let server = start_server(&data_dir, port, &home);

        Sandbox {
            dir,
            home,
            shim,
            data_dir,
            server_url,
            _work: work,
            _home: home_dir,
            _server: server,
        }
    }

    /// Env for every ai-memory-touching child: maestro, the shim, the doctor.
    /// Removals first (they dominate), then the sandbox values — never let
    /// the developer shell's own maestro/ai-memory state leak in.
    fn child_env(&self, cmd: &mut Command) {
        for prefix in ["MAESTRO_", "AI_MEMORY_"] {
            let inherited: Vec<String> = std::env::vars()
                .map(|(k, _)| k)
                .filter(|k| k.starts_with(prefix))
                .collect();
            for key in inherited {
                cmd.env_remove(key);
            }
        }
        cmd.env("HOME", &self.home)
            .env("CLAUDE_CONFIG_DIR", self.home.join(".claude"))
            .env("XDG_CACHE_HOME", self.home.join("cache"))
            .env("XDG_DATA_HOME", self.home.join("data"))
            .env("XDG_CONFIG_HOME", self.home.join("config"))
            .env("AI_MEMORY_DATA_DIR", &self.data_dir)
            .env("AI_MEMORY_SERVER_URL", &self.server_url)
            // No model download, no egress: embeddings off for the sandbox.
            .env("AI_MEMORY_EMBEDDING_PROVIDER", "none");
    }
}

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local addr")
        .port()
}

/// Boots the sandbox server the way the spike did: `init` writes the data
/// layout and token, `serve` binds the chosen loopback port. Readiness is a
/// TCP connect poll, not a flat sleep.
fn start_server(data_dir: &Path, port: u16, home: &Path) -> ServerGuard {
    let mut init = Command::new("ai-memory");
    init.args(["init", "--data-dir"])
        .arg(data_dir)
        .arg("--config")
        .arg(data_dir.join("config.toml"))
        .env("HOME", home)
        .env("AI_MEMORY_DATA_DIR", data_dir)
        .env("AI_MEMORY_EMBEDDING_PROVIDER", "none")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let out = init.output().expect("spawn ai-memory init");
    assert!(
        out.status.success(),
        "ai-memory init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mut child = Command::new("ai-memory")
        .arg("--data-dir")
        .arg(data_dir)
        .args(["serve", "--transport", "http", "--bind"])
        .arg(format!("127.0.0.1:{port}"))
        .env("HOME", home)
        .env("AI_MEMORY_DATA_DIR", data_dir)
        .env("AI_MEMORY_EMBEDDING_PROVIDER", "none")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn ai-memory serve");

    let deadline = Instant::now() + SERVER_BOOT_TIMEOUT;
    loop {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("ai-memory serve never accepted on 127.0.0.1:{port}");
        }
        thread::sleep(POLL_INTERVAL);
    }
    ServerGuard(child)
}

/// Workspace trust gate (#1045): the fixture config declares a shim
/// `command`, so the workspace must be trusted before maestro executes it.
fn trust_workspace(sbx: &Sandbox) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_maestro"));
    cmd.args(["trust", "--yes"]).current_dir(&sbx.dir);
    sbx.child_env(&mut cmd);
    let output = cmd.output().expect("spawn maestro trust");
    assert!(
        output.status.success(),
        "maestro trust failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_config(sbx: &Sandbox, transport: Option<&str>) {
    // Escape the path as a TOML string; a bare `format!` breaks on quotes or backslashes.
    let command = toml::Value::String(sbx.shim.display().to_string()).to_string();
    let transport_line = transport
        .map(|t| format!("transport = \"{t}\"\n"))
        .unwrap_or_default();
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
{transport_line}"#
    );
    fs::write(sbx.dir.join("maestro.toml"), toml).expect("write maestro.toml");
}

/// Runs maestro with the given args in the sandbox, killing it after
/// [`RUN_TIMEOUT`]. stdout/stderr are files, not pipes, so `cargo test`
/// cannot hang on a full pipe. The bounded wait IS the teardown assertion:
/// a shim whose post-exit menu blocked teardown would trip it.
fn run_maestro(sbx: &Sandbox, args: &[&str]) -> ExitStatus {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_maestro"));
    cmd.args(args)
        .current_dir(&sbx.dir)
        .stdin(Stdio::null())
        .stdout(File::create(sbx.dir.join("stdout.txt")).expect("create stdout.txt"))
        .stderr(File::create(sbx.dir.join("stderr.txt")).expect("create stderr.txt"));
    sbx.child_env(&mut cmd);
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
                "maestro {args:?} exceeded {RUN_TIMEOUT:?}; see {}",
                sbx.dir.display()
            );
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn read_or_empty(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

/// Every `.jsonl` transcript under the sandbox config home, without assuming
/// the munging rule or the bound conversation id.
fn stub_transcripts(home: &Path) -> Vec<PathBuf> {
    let projects = home.join(".claude").join("projects");
    let mut found = Vec::new();
    let walk = |dir: &Path| -> Vec<PathBuf> {
        fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    for provider_dir in walk(&projects) {
        for path in walk(&provider_dir) {
            if path.is_file() && path.to_string_lossy().ends_with(".jsonl") {
                found.push(path);
            }
        }
    }
    found
}

/// Autowire stayed off: the harness config home gained neither of the two
/// files the one-time wiring writes (spike scenario 2b — byte-identical tree).
fn assert_no_autowire_writes(home: &Path) {
    for name in ["settings.json", ".claude.json"] {
        let path = home.join(".claude").join(name);
        assert!(
            !path.exists(),
            "autowire wrote {name} into the sandbox config home; \
             the shim must keep --no-autowire"
        );
    }
}

fn strip_ansi(text: &str) -> String {
    text.replace("\u{1b}[32m", "")
        .replace("\u{1b}[31m", "")
        .replace("\u{1b}[33m", "")
        .replace("\u{1b}[0m", "")
}

/// The health check path: maestro's doctor runs `<command> --version` against
/// every configured agent (`health_check_blocking`). Assert the shim relays
/// the stub's version line with exit 0 both directly (the exact probe) and
/// through `maestro doctor` (the real consumer).
#[test]
#[ignore = "needs ai-memory on PATH; run with --ignored (see module docs)"]
fn aimem_shim_version_health_check_relays_stub_version() {
    require_ai_memory();
    let sbx = Sandbox::new("success");

    let mut direct = Command::new(&sbx.shim);
    direct.arg("--version").current_dir(&sbx.dir);
    sbx.child_env(&mut direct);
    let out = direct.output().expect("spawn shim --version");
    assert!(
        out.status.success(),
        "shim --version exited {:?}",
        out.status.code()
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("{STUB_VERSION_LINE}\n")
    );

    write_config(&sbx, None);
    trust_workspace(&sbx);
    let status = run_maestro(&sbx, &["doctor"]);
    let stdout = strip_ansi(&read_or_empty(&sbx.dir.join("stdout.txt")));
    let line = stdout
        .lines()
        .find(|l| l.contains("agent stub"))
        .expect("doctor report lists the configured agent")
        .to_string();
    assert!(
        line.contains("OK") && line.contains(STUB_VERSION_LINE),
        "doctor's agent line should pass with the stub version, got: {line:?} \
         (maestro exit {status})"
    );
}

/// The headless turn: shim stdout byte-identical to the stub's own
/// stream-json, and maestro's real headless transport completes a session
/// with the stub's session id extracted and the reply logged.
#[test]
#[ignore = "needs ai-memory on PATH; run with --ignored (see module docs)"]
fn aimem_shim_headless_turn_is_byte_identical_and_extracts_session() {
    require_ai_memory();
    let sbx = Sandbox::new("success");

    // Seam property (spike scenario 1d): shim relay == stub's own stdout.
    let mut base = Command::new(sbx.dir.join("stub-agent.sh"));
    base.args(HEADLESS_ARGS).current_dir(&sbx.dir);
    sbx.child_env(&mut base);
    let base_out = base.output().expect("spawn stub directly");
    let mut wrapped = Command::new(&sbx.shim);
    wrapped.args(HEADLESS_ARGS).current_dir(&sbx.dir);
    sbx.child_env(&mut wrapped);
    let wrapped_out = wrapped.output().expect("spawn shim headless");
    assert!(
        base_out.status.success(),
        "stub direct run failed: {}\n--- stderr ---\n{}",
        base_out.status,
        String::from_utf8_lossy(&base_out.stderr)
    );
    assert!(
        wrapped_out.status.success(),
        "shim relayed run failed: {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        wrapped_out.status,
        String::from_utf8_lossy(&wrapped_out.stdout),
        String::from_utf8_lossy(&wrapped_out.stderr)
    );
    assert_eq!(
        base_out.stdout, wrapped_out.stdout,
        "shim stdout must be byte-identical to the stub's own stream-json"
    );
    // The session id maestro's parser extracts rides the init line of those
    // bytes — pin it explicitly so this property survives fixture edits.
    assert!(
        String::from_utf8_lossy(&wrapped_out.stdout).contains(STUB_SESSION_ID),
        "relayed stream must carry the stub's session id"
    );

    // The transport property: maestro completes a session through the shim.
    write_config(&sbx, None);
    trust_workspace(&sbx);
    let status = run_maestro(
        &sbx,
        &["run", "--prompt", "hello", "--once", "--skip-doctor"],
    );
    let stdout = read_or_empty(&sbx.dir.join("stdout.txt"));
    let stderr = read_or_empty(&sbx.dir.join("stderr.txt"));
    assert!(
        status.success(),
        "maestro exited {status}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    let state: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(sbx.dir.join("maestro-state.json")).expect("read maestro-state.json"),
    )
    .expect("maestro-state.json is valid JSON");
    let session = state["sessions"]
        .as_array()
        .expect("state has sessions array")
        .iter()
        .find(|s| s["status"] == "completed")
        .cloned()
        .unwrap_or_else(|| panic!("no completed session:\n{state}"));
    // The stub's session id must be the first stream line maestro's parser
    // sees through the shim — asserted on the relayed bytes above. It is
    // deliberately NOT asserted on `agent_session_id` in the state file: in
    // `--once` mode maestro exits as soon as sessions are terminal
    // (`completion_ready`, src/tui/mod.rs), while the wrapper outlives the
    // stub saving workstream events, so the late `session_bound` notice
    // cannot land there — for any post-exit-active wrapper, not just this one.
    let session_id = session["id"].as_str().expect("session has id");
    let log = read_or_empty(
        &sbx.dir
            .join(".maestro/logs")
            .join(format!("{session_id}.log")),
    );
    assert!(
        log.contains("ASSISTANT: stub reply"),
        "session log lost the stub reply:\n{log}"
    );

    assert_no_autowire_writes(&sbx.home);
}

/// Exit codes survive the shim: a stub exiting 3 yields shim exit 3 with the
/// stream still relayed byte-identically (spike scenario 1c measures exactly
/// this — wrapped exit vs the stub's own exit). Through maestro the run still
/// finishes cleanly: maestro drops a failing harness's exit in `--once` mode,
/// because the wrapper outlives the stub and its stderr narration lands after
/// once-mode stopped listening (same late-event mechanism as the headless
/// test's session-binding note) — the swallow `e2e_run_once.rs` documents.
#[test]
#[ignore = "needs ai-memory on PATH; run with --ignored (see module docs)"]
fn aimem_shim_preserves_stub_exit_code_3() {
    require_ai_memory();
    let sbx = Sandbox::new("exit3");

    let args: Vec<String> = HEADLESS_ARGS.iter().map(|s| s.to_string()).collect();
    let mut base = Command::new(sbx.dir.join("stub-agent.sh"));
    base.args(&args).current_dir(&sbx.dir);
    sbx.child_env(&mut base);
    let base_out = base.output().expect("spawn exit-3 stub directly");
    let mut wrapped = Command::new(&sbx.shim);
    wrapped.args(&args).current_dir(&sbx.dir);
    sbx.child_env(&mut wrapped);
    let out = wrapped.output().expect("spawn shim over exit-3 stub");
    assert_eq!(
        out.status.code(),
        Some(3),
        "shim must preserve the stub's exit 3"
    );
    assert_eq!(
        base_out.status.code(),
        Some(3),
        "the exit-3 stub variant must itself exit 3"
    );
    assert_eq!(
        base_out.stdout, out.stdout,
        "exit-3 stub's stream-json must still be relayed byte-identically"
    );

    write_config(&sbx, None);
    trust_workspace(&sbx);
    let status = run_maestro(
        &sbx,
        &["run", "--prompt", "hello", "--once", "--skip-doctor"],
    );
    assert!(status.success(), "maestro itself must not crash: {status}");

    assert_no_autowire_writes(&sbx.home);
}

/// The PTY-interactive invariant (spike scenarios 3a/3g): with `--executable`
/// in the shim, a full interactive turn over the repo PTY backend completes
/// and tears down within the bounded timeout — the post-exit chooser menu
/// never blocks. A menu (or any wrapper stall) trips [`RUN_TIMEOUT`].
#[test]
#[ignore = "needs ai-memory on PATH; run with --ignored (see module docs)"]
fn aimem_shim_interactive_pty_teardown_is_not_blocked_by_chooser() {
    require_ai_memory();
    let sbx = Sandbox::new("success");
    write_config(&sbx, Some("interactive"));
    trust_workspace(&sbx);

    let status = run_maestro(
        &sbx,
        &["run", "--prompt", "hello", "--once", "--skip-doctor"],
    );
    let stdout = read_or_empty(&sbx.dir.join("stdout.txt"));
    let stderr = read_or_empty(&sbx.dir.join("stderr.txt"));
    assert!(
        status.success(),
        "interactive run through the shim exited {status}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    let state: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(sbx.dir.join("maestro-state.json")).expect("read maestro-state.json"),
    )
    .expect("maestro-state.json is valid JSON");
    let session = state["sessions"]
        .as_array()
        .expect("state has sessions array")
        .iter()
        .find(|s| s["status"] == "completed")
        .cloned()
        .unwrap_or_else(|| panic!("no completed session:\n{state}"));
    let session_id = session["id"].as_str().expect("session has id");
    let log = read_or_empty(
        &sbx.dir
            .join(".maestro/logs")
            .join(format!("{session_id}.log")),
    );
    assert!(
        log.contains("ASSISTANT: stub reply"),
        "PTY turn lost the stub reply:\n{log}"
    );

    // The turn rode the PTY: the stub recorded the conversation only the
    // interactive transport pins (`--session-id`) in the sandbox config home.
    let transcripts = stub_transcripts(&sbx.home);
    assert_eq!(
        transcripts.len(),
        1,
        "exactly one stub transcript: {transcripts:?}"
    );
    let transcript = read_or_empty(&transcripts[0]);
    assert!(
        transcript.contains("stub reply"),
        "{} lost the reply:\n{transcript}",
        transcripts[0].display()
    );

    // Teardown completed inside the run: the session's worktree is gone.
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
    let worktrees = sbx.dir.join(".maestro/worktrees");
    let remaining: Vec<String> = fs::read_dir(&worktrees)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        remaining.iter().all(|n| !n.starts_with("session-")),
        "session worktree survived teardown: {remaining:?}"
    );

    assert_no_autowire_writes(&sbx.home);
}
