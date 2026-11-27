//! A git call nobody waits on for longer than a timeout (#1471, #1481).
//! The Kill dialog counts files and commits on the UI thread when it
//! opens, and teardown reads the same counts: a stuck git (a lock, a huge
//! history) must not freeze either. One copy of the rule: wait, then kill.

use std::process::{ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// How long a count waits for git before it gives up ("count unknown").
pub const GIT_COUNT_TIMEOUT: Duration = Duration::from_millis(500);

/// What `read` makes of `command`'s stdout, when the command exits with
/// success within `timeout`. Otherwise `None`, and a command still running
/// is killed: the caller never waits longer than `timeout`. `read` runs on
/// its own thread, so it can stream the output instead of storing it.
pub fn output_within<T: Send + 'static>(
    mut command: Command,
    timeout: Duration,
    read: impl FnOnce(ChildStdout) -> Option<T> + Send + 'static,
) -> Option<T> {
    let mut child = (command.stdin(Stdio::null()))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(stdout.and_then(read));
    });
    let out = rx.recv_timeout(timeout).ok().flatten();
    if out.is_none() {
        let _ = child.kill();
    }
    let status = child.wait().ok()?;
    status.success().then_some(out).flatten()
}

/// Test guard for a FIFO-blocked git (#1481): `f`'s result within `limit`,
/// else the test fails instead of hanging CI. On that failure a writer
/// opens `fifo` and closes it, so a git still blocked on its `open`
/// reads EOF and exits.
#[cfg(test)]
pub(crate) fn within_or_fail<T: Send + 'static>(
    limit: Duration,
    fifo: &std::path::Path,
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(limit) {
        Ok(out) => out,
        Err(_) => {
            let fifo = fifo.to_path_buf();
            std::thread::spawn(move || {
                let _ = std::fs::OpenOptions::new().write(true).open(fifo);
            });
            panic!("git still blocked after {limit:?}: the timeout did not fire");
        }
    }
}
