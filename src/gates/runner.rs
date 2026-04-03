use std::path::Path;
use std::process::Command;

use super::types::{CompletionGate, GateResult};
use crate::util::truncate_at_char_boundary;

/// Trait for running completion gates, enabling mock injection in tests.
pub trait GateCheck: Send {
    fn run_gates(&self, gates: &[CompletionGate], worktree_path: &Path) -> Vec<GateResult>;
}

/// Production gate runner that executes gates in a worktree directory.
pub struct GateRunner;

impl GateCheck for GateRunner {
    fn run_gates(&self, gates: &[CompletionGate], worktree_path: &Path) -> Vec<GateResult> {
        gates
            .iter()
            .map(|gate| run_single_gate(gate, worktree_path))
            .collect()
    }
}

fn run_single_gate(gate: &CompletionGate, worktree_path: &Path) -> GateResult {
    match gate {
        CompletionGate::TestsPass { command } => {
            let parts: Vec<&str> = command.split_whitespace().collect();
            if parts.is_empty() {
                return GateResult::fail("tests_pass", "Empty test command");
            }

            let result = Command::new(parts[0])
                .args(&parts[1..])
                .current_dir(worktree_path)
                .output();

            match result {
                Ok(output) => {
                    if output.status.success() {
                        GateResult::pass("tests_pass", "All tests passed")
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let end = truncate_at_char_boundary(&stderr, 200);
                        let suffix = if end < stderr.len() { "..." } else { "" };
                        GateResult::fail(
                            "tests_pass",
                            format!("Tests failed: {}{}", &stderr[..end], suffix),
                        )
                    }
                }
                Err(e) => GateResult::fail("tests_pass", format!("Failed to run tests: {}", e)),
            }
        }

        CompletionGate::FileExists { path } => {
            let full_path = worktree_path.join(path);
            if full_path.exists() {
                GateResult::pass("file_exists", format!("{} exists", path))
            } else {
                GateResult::fail("file_exists", format!("{} not found", path))
            }
        }

        CompletionGate::FileContains { path, pattern } => {
            let full_path = worktree_path.join(path);
            match std::fs::read_to_string(&full_path) {
                Ok(content) => match regex::Regex::new(pattern) {
                    Ok(re) => {
                        if re.is_match(&content) {
                            GateResult::pass("file_contains", format!("{} contains pattern", path))
                        } else {
                            GateResult::fail(
                                "file_contains",
                                format!("{} does not contain pattern '{}'", path, pattern),
                            )
                        }
                    }
                    Err(e) => GateResult::fail("file_contains", format!("Invalid pattern: {}", e)),
                },
                Err(e) => GateResult::fail("file_contains", format!("Cannot read {}: {}", path, e)),
            }
        }

        CompletionGate::PrCreated => {
            // This gate is checked externally by the PR creation pipeline.
            // If we reach here, it means PR hasn't been verified yet.
            GateResult::pass("pr_created", "PR gate deferred to pipeline")
        }

        CompletionGate::Command { name, command, .. } => {
            if command.trim().is_empty() {
                return GateResult::fail(name, "Empty command");
            }

            let result = Command::new("sh")
                .args(["-c", command])
                .current_dir(worktree_path)
                .output();

            match result {
                Ok(output) => {
                    if output.status.success() {
                        GateResult::pass(name, format!("{} passed", name))
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let end = truncate_at_char_boundary(&stderr, 200);
                        let suffix = if end < stderr.len() { "..." } else { "" };
                        GateResult::fail(
                            name,
                            format!("{} failed: {}{}", name, &stderr[..end], suffix),
                        )
                    }
                }
                Err(e) => GateResult::fail(name, format!("Failed to run {}: {}", name, e)),
            }
        }
    }
}

/// Check if all gate results passed.
pub fn all_gates_passed(results: &[GateResult]) -> bool {
    results.iter().all(|r| r.passed)
}

/// Check if all *required* gate results passed.
/// Non-required gate failures are advisory only.
pub fn all_required_gates_passed(results: &[(GateResult, bool)]) -> bool {
    results.iter().all(|(r, required)| r.passed || !required)
}

#[cfg(test)]
pub struct MockGateRunner {
    pub results: Vec<GateResult>,
}

#[cfg(test)]
impl MockGateRunner {
    pub fn all_pass() -> Self {
        Self {
            results: vec![GateResult::pass("mock", "Mock gate passed")],
        }
    }

    pub fn with_failure() -> Self {
        Self {
            results: vec![GateResult::fail("mock", "Mock gate failed")],
        }
    }
}

#[cfg(test)]
impl GateCheck for MockGateRunner {
    fn run_gates(&self, _gates: &[CompletionGate], _worktree_path: &Path) -> Vec<GateResult> {
        self.results.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn all_gates_passed_true_when_all_pass() {
        let results = vec![GateResult::pass("a", "ok"), GateResult::pass("b", "ok")];
        assert!(all_gates_passed(&results));
    }

    #[test]
    fn all_gates_passed_false_when_any_fail() {
        let results = vec![GateResult::pass("a", "ok"), GateResult::fail("b", "nope")];
        assert!(!all_gates_passed(&results));
    }

    #[test]
    fn all_gates_passed_true_when_empty() {
        assert!(all_gates_passed(&[]));
    }

    #[test]
    fn file_exists_gate_passes_for_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("test.txt"), "hello").unwrap();

        let gate = CompletionGate::FileExists {
            path: "test.txt".into(),
        };
        let result = run_single_gate(&gate, dir.path());
        assert!(result.passed);
    }

    #[test]
    fn file_exists_gate_fails_for_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let gate = CompletionGate::FileExists {
            path: "nonexistent.txt".into(),
        };
        let result = run_single_gate(&gate, dir.path());
        assert!(!result.passed);
    }

    #[test]
    fn file_contains_gate_passes_when_pattern_matches() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let gate = CompletionGate::FileContains {
            path: "main.rs".into(),
            pattern: "fn main".into(),
        };
        let result = run_single_gate(&gate, dir.path());
        assert!(result.passed);
    }

    #[test]
    fn file_contains_gate_fails_when_pattern_not_found() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let gate = CompletionGate::FileContains {
            path: "main.rs".into(),
            pattern: "class Main".into(),
        };
        let result = run_single_gate(&gate, dir.path());
        assert!(!result.passed);
    }

    #[test]
    fn file_contains_gate_fails_for_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let gate = CompletionGate::FileContains {
            path: "nope.rs".into(),
            pattern: "anything".into(),
        };
        let result = run_single_gate(&gate, dir.path());
        assert!(!result.passed);
    }

    #[test]
    fn file_contains_gate_fails_for_invalid_regex() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("test.txt"), "content").unwrap();

        let gate = CompletionGate::FileContains {
            path: "test.txt".into(),
            pattern: "[invalid".into(),
        };
        let result = run_single_gate(&gate, dir.path());
        assert!(!result.passed);
        assert!(result.message.contains("Invalid pattern"));
    }

    #[test]
    fn tests_pass_gate_with_empty_command_fails() {
        let dir = tempfile::tempdir().unwrap();
        let gate = CompletionGate::TestsPass { command: "".into() };
        let result = run_single_gate(&gate, dir.path());
        assert!(!result.passed);
    }

    #[test]
    fn pr_created_gate_always_passes() {
        let dir = tempfile::tempdir().unwrap();
        let gate = CompletionGate::PrCreated;
        let result = run_single_gate(&gate, dir.path());
        assert!(result.passed);
    }

    #[test]
    fn tests_pass_gate_succeeds_with_true_command() {
        let dir = tempfile::tempdir().unwrap();
        let gate = CompletionGate::TestsPass {
            command: "true".into(),
        };
        let result = run_single_gate(&gate, dir.path());
        assert!(result.passed);
    }

    #[test]
    fn tests_pass_gate_fails_with_false_command() {
        let dir = tempfile::tempdir().unwrap();
        let gate = CompletionGate::TestsPass {
            command: "false".into(),
        };
        let result = run_single_gate(&gate, dir.path());
        assert!(!result.passed);
    }

    #[test]
    fn mock_gate_runner_all_pass() {
        let runner = MockGateRunner::all_pass();
        let results = runner.run_gates(&[], Path::new("/tmp"));
        assert!(all_gates_passed(&results));
    }

    #[test]
    fn mock_gate_runner_with_failure() {
        let runner = MockGateRunner::with_failure();
        let results = runner.run_gates(&[], Path::new("/tmp"));
        assert!(!all_gates_passed(&results));
    }

    #[test]
    fn command_gate_passes_when_exit_code_is_zero() {
        let dir = tempfile::tempdir().unwrap();
        let gate = CompletionGate::Command {
            name: "ok".into(),
            command: "true".into(),
            required: true,
        };
        let result = run_single_gate(&gate, dir.path());
        assert!(result.passed);
        assert_eq!(result.gate, "ok");
    }

    #[test]
    fn command_gate_fails_when_exit_code_is_nonzero() {
        let dir = tempfile::tempdir().unwrap();
        let gate = CompletionGate::Command {
            name: "bad".into(),
            command: "false".into(),
            required: true,
        };
        let result = run_single_gate(&gate, dir.path());
        assert!(!result.passed);
    }

    #[test]
    fn command_gate_fails_gracefully_with_empty_command() {
        let dir = tempfile::tempdir().unwrap();
        let gate = CompletionGate::Command {
            name: "empty".into(),
            command: "".into(),
            required: true,
        };
        let result = run_single_gate(&gate, dir.path());
        assert!(!result.passed);
        assert!(result.message.contains("Empty"));
    }

    #[test]
    fn all_required_gates_passed_ignores_optional_failure() {
        let results = vec![
            (GateResult::pass("required-ok", "ok"), true),
            (GateResult::fail("optional-fail", "nope"), false),
        ];
        assert!(all_required_gates_passed(&results));
    }

    #[test]
    fn all_required_gates_passed_fails_when_required_gate_fails() {
        let results = vec![
            (GateResult::pass("optional-ok", "ok"), false),
            (GateResult::fail("required-fail", "nope"), true),
        ];
        assert!(!all_required_gates_passed(&results));
    }

    #[test]
    fn all_required_gates_passed_true_when_all_required_pass() {
        let results = vec![
            (GateResult::pass("required-a", "ok"), true),
            (GateResult::pass("required-b", "ok"), true),
            (GateResult::fail("optional-c", "nope"), false),
        ];
        assert!(all_required_gates_passed(&results));
    }
}
