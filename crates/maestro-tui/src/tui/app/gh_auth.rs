//! gh auth recovery (#1241). `gh_auth_ok` turns false on the first gh
//! auth error; this module turns it back on. Two ways back: any gh call
//! that succeeds (`note_gh_auth_ok`), and one `gh auth status` recheck
//! before a PR action is refused (`recheck_gh_auth`). The recheck is a
//! subprocess, so it runs on the blocking pool and never on the draw
//! path.

use super::App;
use super::auto_pr_phase::PrPhaseInput;
use crate::tui::activity_log::LogLevel;
use std::sync::Arc;
use std::time::Duration;

/// `gh auth status` calls the network; a stuck network must not hold
/// the completion job. Past this, the recheck says "signed out".
const GH_AUTH_RECHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Answers "is gh signed in right now?". Injected so tests never depend
/// on whether `gh` is on PATH or logged in.
pub(crate) type GhAuthProbe = Arc<dyn Fn() -> bool + Send + Sync>;

/// The probe `App::new` installs. Tests get a probe that always says
/// "signed out", so the pre-#1241 auth-lost paths keep their behavior.
pub(super) fn default_gh_auth_probe() -> GhAuthProbe {
    if cfg!(test) {
        Arc::new(|| false)
    } else {
        Arc::new(gh_auth_status)
    }
}

/// One `gh auth status` run. A missing `gh` counts as signed out; a run
/// past `GH_AUTH_RECHECK_TIMEOUT` is killed, so no stuck child is left
/// behind on the blocking pool.
fn gh_auth_status() -> bool {
    let Ok(mut child) = std::process::Command::new("gh")
        .args(["auth", "status"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return false;
    };
    let deadline = std::time::Instant::now() + GH_AUTH_RECHECK_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// Run the probe once, off the async worker. A join error or a probe
/// slower than `timeout` counts as signed out.
pub(super) async fn recheck_gh_auth(probe: &GhAuthProbe, timeout: Duration) -> bool {
    let probe = Arc::clone(probe);
    let run = tokio::task::spawn_blocking(move || probe());
    matches!(tokio::time::timeout(timeout, run).await, Ok(Ok(true)))
}

/// Recheck once before the PR phase refuses for lost auth. Only a
/// successful session with a branch is a real PR action, so nothing
/// else pays for the subprocess. Returns `true` when gh is signed in
/// again; the input then runs the normal PR flow.
pub(super) async fn recheck_before_pr(input: &mut PrPhaseInput) -> bool {
    if input.gh_auth_ok || !input.success || input.branch.is_none() {
        return false;
    }
    input.gh_auth_ok = recheck_gh_auth(&input.gh_auth_probe, GH_AUTH_RECHECK_TIMEOUT).await;
    input.gh_auth_ok
}

impl App {
    /// A gh call succeeded, so auth works again. Logs once, on the
    /// false → true edge; the warning stops drawing with the flag.
    pub(super) fn note_gh_auth_ok(&mut self) {
        if self.gh_auth_ok {
            return;
        }
        self.gh_auth_ok = true;
        self.activity_log.push_simple(
            "AUTH".into(),
            "GitHub authentication restored.".into(),
            LogLevel::Info,
        );
    }
}
