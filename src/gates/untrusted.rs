//! `UntrustedGateRunner` — workspace trust gate for completion gates
//! (#1045). Split out of `runner.rs` for the 400-LOC file-size cap.

use super::runner::{GateCheck, run_single_gate};
use super::types::{CompletionGate, GateResult};
use std::path::Path;

/// Gate runner for workspaces whose config is not trusted (#1045).
///
/// Gates that would execute a config-supplied program (`TestsPass`,
/// `Command`) fail closed with an explicit message; filesystem-only gates
/// (`FileExists`, `FileContains`) still run because they never execute
/// anything. `PrCreated` is deferred upstream either way.
pub struct UntrustedGateRunner;

impl GateCheck for UntrustedGateRunner {
    fn run_gates(&self, gates: &[CompletionGate], worktree_path: &Path) -> Vec<GateResult> {
        gates
            .iter()
            .map(|gate| match gate {
                CompletionGate::TestsPass { .. } => GateResult::fail(
                    "tests_pass",
                    "skipped: workspace config is not trusted (run `maestro trust`)",
                ),
                CompletionGate::Command { name, .. } => GateResult::fail(
                    name,
                    "skipped: workspace config is not trusted (run `maestro trust`)",
                ),
                _ => run_single_gate(gate, worktree_path),
            })
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::UntrustedGateRunner;
    use crate::config::CompletionGateEntry;
    use crate::gates::runner::GateCheck;
    use crate::gates::types::CompletionGate;
    use std::path::Path;

    fn command_gate(name: &str, command: &str) -> CompletionGate {
        CompletionGate::from_config_entry(&CompletionGateEntry {
            name: name.into(),
            run: command.into(),
            required: true,
        })
    }

    #[test]
    fn workspace_trust_untrusted_runner_fails_executing_gates_without_running_them() {
        let runner = UntrustedGateRunner;
        let gates = vec![command_gate("marker", "echo should-not-run")];
        let results = runner.run_gates(&gates, Path::new("/tmp"));
        assert_eq!(results.len(), 1);
        assert!(!results[0].passed);
        assert!(
            results[0].message.contains("not trusted"),
            "message: {}",
            results[0].message
        );
    }

    #[test]
    fn workspace_trust_untrusted_runner_fails_tests_pass_gate() {
        let runner = UntrustedGateRunner;
        let gates = vec![CompletionGate::TestsPass {
            command: "echo should-not-run".into(),
        }];
        let results = runner.run_gates(&gates, Path::new("/tmp"));
        assert_eq!(results.len(), 1);
        assert!(!results[0].passed);
        assert!(results[0].message.contains("not trusted"));
    }

    #[test]
    fn workspace_trust_untrusted_runner_still_checks_file_gates() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("present.txt");
        std::fs::write(&file, "x").unwrap();
        let runner = UntrustedGateRunner;
        let gates = vec![CompletionGate::FileExists {
            path: file.display().to_string(),
        }];
        let results = runner.run_gates(&gates, dir.path());
        assert_eq!(results.len(), 1);
        assert!(results[0].passed, "filesystem gates must still run");
    }
}
