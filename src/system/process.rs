//! Process liveness checks (#1149).

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// True when a process with this pid currently exists on the host (#1149).
///
/// Used to detect stale sessions: a session whose owning maestro process is
/// gone can never settle its own status. sysinfo (not `kill(pid, 0)`) keeps
/// this safe — the crate-wide `unsafe_code = "deny"` stays intact and a
/// recycled or oversized pid can never turn into a signal aimed at an
/// unrelated process group. Note the inherent PID-reuse race: a dead
/// maestro whose pid got recycled by an unrelated process will not be
/// flagged; acceptable for a reporting/cleanup feature, and `--dry-run`
/// previews every removal.
pub fn is_process_alive(pid: u32) -> bool {
    let pid = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    sys.process(pid).is_some()
}

#[cfg(test)]
pub(crate) mod testing {
    /// Spawn a short-lived child, kill it, reap it — returns a pid that is
    /// no longer alive. `sleep` exists on macOS and Linux CI. Best-effort:
    /// the pid could in principle be recycled by an unrelated process
    /// before the caller checks it; the window is far below test flake
    /// levels (same pattern the #1139 pool tests use).
    pub(crate) fn dead_process_pid() -> u32 {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep child for dead-pid fixture");
        let pid = child.id();
        let _ = child.kill();
        let _ = child.wait();
        pid
    }
}

#[cfg(test)]
mod tests {
    use super::is_process_alive;
    use super::testing::dead_process_pid;

    #[test]
    fn current_process_is_alive() {
        assert!(is_process_alive(std::process::id()));
    }

    #[test]
    fn killed_child_process_is_not_alive() {
        let pid = dead_process_pid();
        assert!(
            !is_process_alive(pid),
            "reaped child pid {pid} must not report alive"
        );
    }
}
