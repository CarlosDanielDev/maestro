//! Throwaway spike prototype for issue #747.
//!
//! Drives `claude` (interactive REPL, no `--print`) inside a portable-pty
//! pseudo-terminal, sends one turn, and confirms the response lands in the
//! session transcript JSONL at
//! `~/.claude/projects/<munged-cwd>/<session-id>.jsonl`.
//!
//! Flow:
//!  1. generate a session UUID, compute the transcript path up front
//!  2. spawn `claude --session-id <uuid>` in a 132x40 pty (env scrubbed of
//!     `ANTHROPIC_*`)
//!  3. drain pty output on a reader thread (child blocks if we don't)
//!  4. wait for the REPL to come up, write a sentinel prompt + `\r`
//!  5. poll the transcript JSONL until an assistant entry contains the
//!     sentinel reply (timeout 120s)
//!  6. send `/exit` + `\r`, wait for child exit, hard-kill on timeout

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SENTINEL: &str = "MAESTRO-PTY-OK";
const CWD: &str = "/Users/carlos/projects/maestro";
const SCRUBBED_PREFIX: &str = "ANTHROPIC_";

fn munge_cwd(cwd: &str) -> String {
    cwd.replace('/', "-").replace('.', "-")
}

fn main() {
    let uuid = String::from_utf8(
        Command::new("uuidgen").output().expect("uuidgen").stdout,
    )
    .expect("utf8")
    .trim()
    .to_lowercase();

    let home = std::env::var("HOME").expect("HOME");
    let transcript: PathBuf = [
        home.as_str(),
        ".claude",
        "projects",
        &munge_cwd(CWD),
        &format!("{uuid}.jsonl"),
    ]
    .iter()
    .collect();
    println!("session-id: {uuid}");
    println!("transcript: {}", transcript.display());

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 40,
            cols: 132,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");

    let mut cmd = CommandBuilder::new("claude");
    cmd.args(["--safe-mode", "--session-id", &uuid, "--model", "haiku"]);
    cmd.cwd(CWD);
    // Env scrub: remove every ANTHROPIC_* var so the child cannot fall back
    // to API-key billing (spike question 5).
    let mut scrubbed = Vec::new();
    for (k, _) in std::env::vars() {
        if k.starts_with(SCRUBBED_PREFIX) {
            cmd.env_remove(&k);
            scrubbed.push(k);
        }
    }
    println!("scrubbed vars: {scrubbed:?}");

    let mut child = pair.slave.spawn_command(cmd).expect("spawn claude");
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().expect("reader");
    let mut writer = pair.master.take_writer().expect("writer");
    let stop = Arc::new(AtomicBool::new(false));
    let stop_reader = stop.clone();
    let drain = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut total = 0usize;
        while !stop_reader.load(Ordering::Relaxed) {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => total += n,
            }
        }
        total
    });

    // Give the REPL time to boot (safe-mode skips plugins/hooks; still needs
    // model + auth handshake).
    std::thread::sleep(Duration::from_secs(10));

    let turn = format!("Reply with exactly: {SENTINEL}\r");
    writer.write_all(turn.as_bytes()).expect("write turn");
    writer.flush().expect("flush");
    println!("turn written, polling transcript...");

    let deadline = Instant::now() + Duration::from_secs(120);
    let mut confirmed = false;
    while Instant::now() < deadline {
        if let Ok(body) = std::fs::read_to_string(&transcript) {
            if body
                .lines()
                .filter(|l| l.contains("\"type\":\"assistant\""))
                .any(|l| l.contains(SENTINEL))
            {
                confirmed = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    println!("assistant sentinel in transcript: {confirmed}");

    writer.write_all(b"/exit\r").expect("write exit");
    writer.flush().expect("flush exit");

    let exit_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            println!("child exited: {status:?}");
            break;
        }
        if Instant::now() > exit_deadline {
            println!("child did not exit, killing");
            let _ = child.kill();
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    stop.store(true, Ordering::Relaxed);
    drop(pair.master);
    let drained = drain.join().unwrap_or(0);
    println!("pty bytes drained: {drained}");
    println!("VERDICT: {}", if confirmed { "GREEN" } else { "RED" });
}
