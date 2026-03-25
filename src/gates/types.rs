use serde::{Deserialize, Serialize};

/// A gate that must pass before a session is considered truly complete.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CompletionGate {
    /// Run a test command (e.g., "cargo test") — exit 0 means pass.
    TestsPass { command: String },
    /// Check that a specific file exists in the worktree.
    FileExists { path: String },
    /// Check that a file contains a specific pattern (regex).
    FileContains { path: String, pattern: String },
    /// Check that a PR was created (verified externally).
    PrCreated,
}

/// Result of running a single gate.
#[derive(Debug, Clone)]
pub struct GateResult {
    pub gate: String,
    pub passed: bool,
    pub message: String,
}

impl GateResult {
    pub fn pass(gate: &str, message: impl Into<String>) -> Self {
        Self {
            gate: gate.to_string(),
            passed: true,
            message: message.into(),
        }
    }

    pub fn fail(gate: &str, message: impl Into<String>) -> Self {
        Self {
            gate: gate.to_string(),
            passed: false,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_result_pass_sets_passed_true() {
        let r = GateResult::pass("tests", "All tests passed");
        assert!(r.passed);
        assert_eq!(r.gate, "tests");
    }

    #[test]
    fn gate_result_fail_sets_passed_false() {
        let r = GateResult::fail("tests", "3 tests failed");
        assert!(!r.passed);
    }

    #[test]
    fn completion_gate_tests_pass_serializes() {
        let gate = CompletionGate::TestsPass {
            command: "cargo test".into(),
        };
        let json = serde_json::to_string(&gate).unwrap();
        assert!(json.contains("tests_pass"));
        assert!(json.contains("cargo test"));
    }

    #[test]
    fn completion_gate_file_exists_serializes() {
        let gate = CompletionGate::FileExists {
            path: "README.md".into(),
        };
        let json = serde_json::to_string(&gate).unwrap();
        assert!(json.contains("file_exists"));
    }

    #[test]
    fn completion_gate_file_contains_serializes() {
        let gate = CompletionGate::FileContains {
            path: "src/main.rs".into(),
            pattern: "fn main".into(),
        };
        let json = serde_json::to_string(&gate).unwrap();
        assert!(json.contains("file_contains"));
    }

    #[test]
    fn completion_gate_deserializes_tests_pass() {
        let json = r#"{"type":"tests_pass","command":"cargo test"}"#;
        let gate: CompletionGate = serde_json::from_str(json).unwrap();
        match gate {
            CompletionGate::TestsPass { command } => assert_eq!(command, "cargo test"),
            _ => panic!("expected TestsPass"),
        }
    }
}
