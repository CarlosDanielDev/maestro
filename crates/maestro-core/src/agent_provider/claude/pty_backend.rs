//! `portable-pty` production backend for the interactive Claude transport
//! (issue #749). Split from `interactive.rs` to honor the 400-LOC module cap.
//!
//! The PTY master is drained continuously on a dedicated thread — the child
//! stalls if the master side backs up. Output is rendered for humans only;
//! all structure comes from the transcript tail (spike #747), so the drained
//! bytes are dropped.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::agent_provider::types::AgentError;

use super::interactive::{InteractiveBackend, InteractiveChild, SpawnSpec};

/// Production backend: spawns the child on a real PTY via `portable-pty`.
pub(super) struct PortablePtyBackend;

impl InteractiveBackend for PortablePtyBackend {
    fn spawn(&self, spec: &SpawnSpec) -> Result<Box<dyn InteractiveChild>, AgentError> {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};

        let spawn_err = |msg: String| AgentError::Spawn {
            provider_id: "claude".to_string(),
            source: std::io::Error::other(msg),
        };

        let pair = native_pty_system()
            .openpty(PtySize {
                rows: spec.rows,
                cols: spec.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| spawn_err(format!("openpty failed: {e}")))?;

        let mut cmd = CommandBuilder::new(&spec.binary);
        cmd.args(&spec.args);
        if let Some(cwd) = spec.cwd.as_ref() {
            cmd.cwd(cwd);
        }
        for name in &spec.env_remove {
            cmd.env_remove(name);
        }
        for (name, value) in &spec.env_set {
            cmd.env(name, value);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| spawn_err(format!("pty spawn failed: {e}")))?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| spawn_err(format!("pty reader: {e}")))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| spawn_err(format!("pty writer: {e}")))?;

        // Drain the PTY continuously; bytes are counted and dropped.
        let stop = Arc::new(AtomicBool::new(false));
        let drain_stop = Arc::clone(&stop);
        let drain = std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while !drain_stop.load(Ordering::Relaxed) {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });

        Ok(Box::new(PtyChild {
            child,
            writer,
            _master: pair.master,
            stop,
            drain: Some(drain),
        }))
    }
}

struct PtyChild {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Box<dyn std::io::Write + Send>,
    _master: Box<dyn portable_pty::MasterPty + Send>,
    stop: Arc<AtomicBool>,
    drain: Option<std::thread::JoinHandle<()>>,
}

impl InteractiveChild for PtyChild {
    fn write_text(&mut self, text: &str) -> Result<(), AgentError> {
        self.writer
            .write_all(text.as_bytes())
            .and_then(|()| self.writer.flush())
            .map_err(|e| AgentError::Stream(format!("pty write failed: {e}")))
    }

    fn try_wait(&mut self) -> Result<Option<i32>, AgentError> {
        match self.child.try_wait() {
            Ok(Some(status)) => Ok(Some(status.exit_code() as i32)),
            Ok(None) => Ok(None),
            Err(e) => Err(AgentError::Stream(format!("pty try_wait failed: {e}"))),
        }
    }

    fn kill(&mut self) -> Result<(), AgentError> {
        self.child
            .kill()
            .map_err(|e| AgentError::Stream(format!("pty kill failed: {e}")))
    }

    fn process_id(&self) -> Option<u32> {
        self.child.process_id()
    }
}

impl Drop for PtyChild {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Reap if still running so no orphan PTY child outlives the session.
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
        }
        if let Some(handle) = self.drain.take() {
            let _ = handle.join();
        }
    }
}
