//! Shell-launcher abstraction for `[s] Shell into worktree` (issue #560).
//!
//! The recovery modal needs to suspend the TUI, drop the user into an
//! interactive `$SHELL` rooted at the retained worktree, then re-enter
//! the alt-screen + raw mode when the shell exits. The trait keeps the
//! suspend/resume policy swappable so tests can assert the path was
//! reached without forking a real shell.

use anyhow::Result;
use std::path::Path;

/// Suspend the TUI, open `$SHELL` rooted at `worktree_path`, wait for
/// exit, then restore the TUI. Implementations are responsible for the
/// raw-mode + alternate-screen toggles.
pub trait ShellLauncher: Send + Sync {
    fn open_shell_at(&self, worktree_path: &Path) -> Result<()>;
}

/// Production launcher. Reads `$SHELL` (falls back to `/bin/sh`),
/// suspends crossterm raw mode and the alternate screen, runs the shell
/// synchronously with `cwd = worktree_path`, then re-enters raw mode +
/// alt-screen regardless of the shell exit code.
///
/// The path is passed to `Command::current_dir`, NOT interpolated into a
/// shell string — so a worktree path containing shell metacharacters
/// (`$`, backticks, `;`) cannot escape into the parent shell.
pub struct OsShellLauncher;

impl ShellLauncher for OsShellLauncher {
    fn open_shell_at(&self, worktree_path: &Path) -> Result<()> {
        use std::process::Command;
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let mut stdout = std::io::stdout();

        // Best-effort suspend: do NOT `?` here — restore must run
        // unconditionally even if a partial disable left raw mode
        // half-toggled, otherwise the terminal can be left stuck.
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crate::tui::leave_tui_mode(&mut stdout);

        let status_result = Command::new(&shell).current_dir(worktree_path).status();

        // Restore raw mode + alt-screen + mouse + bracketed-paste so
        // the TUI's input handling is identical after `[s]` returns.
        let _ = crate::tui::enter_tui_mode(&mut stdout);
        let _ = crossterm::terminal::enable_raw_mode();

        let _ = status_result?;
        Ok(())
    }
}

#[cfg(test)]
pub struct CapturingShellLauncher {
    pub calls: std::sync::Arc<std::sync::Mutex<Vec<std::path::PathBuf>>>,
    pub should_fail: bool,
}

#[cfg(test)]
impl CapturingShellLauncher {
    pub fn new() -> Self {
        Self {
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            should_fail: false,
        }
    }

    pub fn failing() -> Self {
        Self {
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            should_fail: true,
        }
    }
}

#[cfg(test)]
impl ShellLauncher for CapturingShellLauncher {
    fn open_shell_at(&self, worktree_path: &Path) -> Result<()> {
        self.calls
            .lock()
            .expect("test mutex poisoned")
            .push(worktree_path.to_path_buf());
        if self.should_fail {
            anyhow::bail!("simulated shell open failure");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn shell_launcher_trait_open_shell_at_is_called_with_correct_path() {
        let launcher = CapturingShellLauncher::new();
        let path = PathBuf::from("/tmp/wt/issue-542");
        let result = launcher.open_shell_at(&path);
        assert!(result.is_ok());
        let calls = launcher.calls.lock().expect("mutex");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], path);
    }

    #[test]
    fn shell_launcher_fake_returns_error_when_should_fail_is_true() {
        let launcher = CapturingShellLauncher::failing();
        let result = launcher.open_shell_at(&PathBuf::from("/tmp/wt/issue-542"));
        assert!(result.is_err());
        // Call is still recorded so handler-level tests can prove the
        // call was attempted before the simulated failure.
        assert_eq!(launcher.calls.lock().expect("mutex").len(), 1);
    }
}
