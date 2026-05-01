//! Re-run completion gates against a retained worktree, wired to `[g]`
//! on the failed-gates recovery modal. Synchronous on the TUI thread:
//! gates take seconds and the user is explicitly waiting.

use super::App;
use super::helpers::build_completion_gates;
use crate::gates::runner;
use crate::session::types::GateResultEntry;
use crate::tui::activity_log::LogLevel;
use anyhow::Result;
use uuid::Uuid;

impl App {
    /// Re-run the configured completion gates against the worktree of a
    /// previously-failed session and refresh `session.gate_results`.
    /// Status stays `FailedGates` regardless of retry outcome — terminal
    /// state is permanent; an all-pass retry emits a guidance log entry
    /// pointing at `[r]` instead of transitioning to `Completed`.
    pub fn retry_completion_gates(&mut self, session_id: Uuid) -> Result<()> {
        let gates = build_completion_gates(self.config.as_ref());
        if gates.is_empty() {
            return Ok(());
        }

        let (wt_path, issue_label) = match self.pool.get_session_mut(session_id) {
            Some(s) => match &s.worktree_path {
                Some(p) => (
                    p.clone(),
                    s.issue_number
                        .map(|n| format!("#{}", n))
                        .unwrap_or_else(|| "session".to_string()),
                ),
                None => return Ok(()),
            },
            None => return Ok(()),
        };

        let results = self.gate_runner.run_gates(&gates, &wt_path);
        let paired: Vec<_> = results
            .into_iter()
            .zip(gates.iter().map(|g| g.is_required()))
            .collect();
        let all_pass = runner::all_required_gates_passed(&paired);

        if let Some(s) = self.pool.get_session_mut(session_id) {
            s.gate_results = paired
                .iter()
                .filter(|(r, _)| !r.passed)
                .map(|(r, _)| GateResultEntry {
                    gate: r.gate.clone(),
                    passed: r.passed,
                    message: r.message.clone(),
                })
                .collect();
        }

        if all_pass {
            self.activity_log.push_simple(
                issue_label,
                "Gates now passing on retry — use [r] to resume implement and \
                 finalize the session"
                    .to_string(),
                LogLevel::Info,
            );
        } else {
            self.activity_log.push_simple(
                issue_label,
                "Gates still failing on retry — see modal for details".to_string(),
                LogLevel::Warn,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gates::runner::GateCheck;
    use crate::gates::types::{CompletionGate, GateResult};
    use crate::session::transition::TransitionReason;
    use crate::session::types::{Session, SessionStatus};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Test fake: records the `gates` argument and returns a fixed result.
    pub struct CapturingGateRunner {
        pub call_count: AtomicU32,
        pub gates_received: Mutex<Vec<CompletionGate>>,
        pub results: Vec<GateResult>,
    }

    impl CapturingGateRunner {
        fn new(results: Vec<GateResult>) -> Arc<Self> {
            Arc::new(Self {
                call_count: AtomicU32::new(0),
                gates_received: Mutex::new(Vec::new()),
                results,
            })
        }
    }

    impl GateCheck for Arc<CapturingGateRunner> {
        fn run_gates(
            &self,
            gates: &[CompletionGate],
            _worktree_path: &std::path::Path,
        ) -> Vec<GateResult> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            *self.gates_received.lock().expect("mutex") = gates.to_vec();
            self.results.clone()
        }
    }

    fn make_failed_gates_session(issue: u64, wt: &str) -> Session {
        let mut s = Session::new(
            "task".into(),
            "opus".into(),
            "orchestrator".into(),
            Some(issue),
            None,
        );
        s.worktree_path = Some(PathBuf::from(wt));
        s.gate_results = vec![
            GateResultEntry::fail("clippy", "stale failure 1"),
            GateResultEntry::fail("tests", "stale failure 2"),
        ];
        s.transition_to(SessionStatus::Spawning, TransitionReason::Promoted)
            .expect("Q->Sp");
        s.transition_to(SessionStatus::Running, TransitionReason::Spawned)
            .expect("Sp->Ru");
        s.transition_to(SessionStatus::GatesRunning, TransitionReason::GatesStarted)
            .expect("Ru->GR");
        s.transition_to(SessionStatus::FailedGates, TransitionReason::GatesFailed)
            .expect("GR->FG");
        s
    }

    fn config_with_completion_commands(commands: &[(&str, &str, bool)]) -> crate::config::Config {
        let mut entries = String::new();
        for (name, run, required) in commands {
            entries.push_str(&format!(
                "[[sessions.completion_gates.commands]]\n\
                 name = \"{}\"\nrun = \"{}\"\nrequired = {}\n",
                name, run, required
            ));
        }
        let toml = format!(
            "[project]\nrepo = \"owner/repo\"\n\
             [sessions]\n\
             [sessions.completion_gates]\nenabled = true\n{entries}\n\
             [budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
             [github]\n[notifications]\n\
             [gates]\nenabled = false\ntest_command = \"\"\n"
        );
        toml::from_str(&toml).expect("config parse")
    }

    fn config_with_legacy_test_gate(cmd: &str) -> crate::config::Config {
        let toml = format!(
            "[project]\nrepo = \"owner/repo\"\n\
             [sessions]\n\
             [budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n\
             [github]\n[notifications]\n\
             [gates]\nenabled = true\ntest_command = \"{}\"\n",
            cmd
        );
        toml::from_str(&toml).expect("config parse")
    }

    #[test]
    fn retry_completion_gates_builds_gates_from_sessions_completion_gates_commands() {
        let runner = CapturingGateRunner::new(vec![GateResult::fail("clippy", "still failing")]);
        let mut app = crate::tui::make_test_app("issue-560-retry-cmd")
            .with_gate_runner(Box::new(runner.clone()));
        app.config = Some(config_with_completion_commands(&[
            ("clippy", "echo c", true),
            ("fmt", "echo f", true),
        ]));

        let session = make_failed_gates_session(560, "/tmp/wt");
        let id = session.id;
        // Push directly into finished bucket (FailedGates is terminal —
        // pool.finalize would normally land it there).
        app.pool.enqueue(session);
        app.pool.try_promote();
        app.pool.finalize(id);

        app.retry_completion_gates(id).expect("retry");

        assert_eq!(runner.call_count.load(Ordering::SeqCst), 1);
        let gates = runner.gates_received.lock().expect("mutex").clone();
        assert_eq!(gates.len(), 2);
    }

    #[test]
    fn retry_completion_gates_falls_back_to_legacy_gates_config_when_commands_empty() {
        let runner = CapturingGateRunner::new(vec![GateResult::fail("tests_pass", "fail")]);
        let mut app = crate::tui::make_test_app("issue-560-retry-legacy")
            .with_gate_runner(Box::new(runner.clone()));
        app.config = Some(config_with_legacy_test_gate("cargo test"));

        let session = make_failed_gates_session(560, "/tmp/wt");
        let id = session.id;
        app.pool.enqueue(session);
        app.pool.try_promote();
        app.pool.finalize(id);

        app.retry_completion_gates(id).expect("retry");

        let gates = runner.gates_received.lock().expect("mutex").clone();
        assert_eq!(gates.len(), 1);
        assert!(matches!(gates[0], CompletionGate::TestsPass { .. }));
    }

    #[test]
    fn retry_completion_gates_refreshes_gate_results_on_session_when_all_fail() {
        let runner = CapturingGateRunner::new(vec![GateResult::fail("clippy", "fresh failure")]);
        let mut app =
            crate::tui::make_test_app("issue-560-retry-refresh").with_gate_runner(Box::new(runner));
        app.config = Some(config_with_completion_commands(&[(
            "clippy", "echo c", true,
        )]));

        let session = make_failed_gates_session(560, "/tmp/wt");
        let id = session.id;
        app.pool.enqueue(session);
        app.pool.try_promote();
        app.pool.finalize(id);

        app.retry_completion_gates(id).expect("retry");

        let s = app.pool.get_session_mut(id).expect("session");
        // Stale entries replaced, not appended.
        assert_eq!(s.gate_results.len(), 1);
        assert_eq!(s.gate_results[0].gate, "clippy");
        assert_eq!(s.gate_results[0].message, "fresh failure");
    }

    #[test]
    fn retry_completion_gates_emits_activity_log_entry_when_gates_now_pass() {
        let runner = CapturingGateRunner::new(vec![GateResult::pass("clippy", "ok")]);
        let mut app =
            crate::tui::make_test_app("issue-560-retry-pass").with_gate_runner(Box::new(runner));
        app.config = Some(config_with_completion_commands(&[(
            "clippy", "echo c", true,
        )]));

        let session = make_failed_gates_session(560, "/tmp/wt");
        let id = session.id;
        app.pool.enqueue(session);
        app.pool.try_promote();
        app.pool.finalize(id);

        app.retry_completion_gates(id).expect("retry");

        assert!(
            app.activity_log.entries().iter().any(|e| {
                let msg = e.message.to_lowercase();
                msg.contains("passing") && msg.contains("resume")
            }),
            "all-pass retry must emit a guidance entry pointing the user to [r]"
        );
    }

    #[test]
    fn retry_completion_gates_does_not_transition_session_status_when_gates_pass() {
        let runner = CapturingGateRunner::new(vec![GateResult::pass("clippy", "ok")]);
        let mut app = crate::tui::make_test_app("issue-560-retry-no-transition")
            .with_gate_runner(Box::new(runner));
        app.config = Some(config_with_completion_commands(&[(
            "clippy", "echo c", true,
        )]));

        let session = make_failed_gates_session(560, "/tmp/wt");
        let id = session.id;
        app.pool.enqueue(session);
        app.pool.try_promote();
        app.pool.finalize(id);

        app.retry_completion_gates(id).expect("retry");

        let s = app.pool.get_session_mut(id).expect("session");
        assert_eq!(
            s.status,
            SessionStatus::FailedGates,
            "FailedGates is terminal — retry must NOT transition to Completed"
        );
    }

    #[test]
    fn retry_completion_gates_finds_session_in_finished_bucket() {
        let runner = CapturingGateRunner::new(vec![GateResult::fail("clippy", "fail")]);
        let mut app = crate::tui::make_test_app("issue-560-retry-finished")
            .with_gate_runner(Box::new(runner.clone()));
        app.config = Some(config_with_completion_commands(&[(
            "clippy", "echo c", true,
        )]));

        let session = make_failed_gates_session(560, "/tmp/wt");
        let id = session.id;
        app.pool.enqueue(session);
        app.pool.try_promote();
        app.pool.finalize(id);

        // Sanity: the session is now in the `finished` bucket. retry must
        // still be able to find it via pool.get_session_mut.
        let result = app.retry_completion_gates(id);
        assert!(result.is_ok());
        assert_eq!(runner.call_count.load(Ordering::SeqCst), 1);
    }
}
