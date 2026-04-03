use crate::config::CompletionGateEntry;
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
    /// A named command gate from [sessions.completion_gates.commands].
    Command {
        name: String,
        command: String,
        required: bool,
    },
}

impl CompletionGate {
    pub fn from_config_entry(entry: &CompletionGateEntry) -> Self {
        Self::Command {
            name: entry.name.clone(),
            command: entry.run.clone(),
            required: entry.required,
        }
    }

    pub fn is_required(&self) -> bool {
        match self {
            Self::Command { required, .. } => *required,
            _ => true,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::TestsPass { .. } => "tests",
            Self::FileExists { .. } => "file_exists",
            Self::FileContains { .. } => "file_contains",
            Self::PrCreated => "pr_created",
            Self::Command { name, .. } => name.as_str(),
        }
    }
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

    #[test]
    fn command_gate_is_required_returns_true_when_required() {
        let gate = CompletionGate::Command {
            name: "fmt".into(),
            command: "cargo fmt --check".into(),
            required: true,
        };
        assert!(gate.is_required());
    }

    #[test]
    fn command_gate_is_required_returns_false_when_optional() {
        let gate = CompletionGate::Command {
            name: "clippy".into(),
            command: "cargo clippy".into(),
            required: false,
        };
        assert!(!gate.is_required());
    }

    #[test]
    fn command_gate_display_name_returns_name_field() {
        let gate = CompletionGate::Command {
            name: "lint".into(),
            command: "cargo clippy".into(),
            required: true,
        };
        assert_eq!(gate.display_name(), "lint");
    }

    #[test]
    fn command_gate_from_config_entry_copies_all_fields() {
        let entry = CompletionGateEntry {
            name: "fmt".into(),
            run: "cargo fmt --check".into(),
            required: false,
        };
        let gate = CompletionGate::from_config_entry(&entry);
        match gate {
            CompletionGate::Command {
                name,
                command,
                required,
            } => {
                assert_eq!(name, "fmt");
                assert_eq!(command, "cargo fmt --check");
                assert!(!required);
            }
            _ => panic!("expected Command variant"),
        }
    }

    #[test]
    fn command_gate_serializes_with_type_tag() {
        let gate = CompletionGate::Command {
            name: "fmt".into(),
            command: "cargo fmt --check".into(),
            required: true,
        };
        let json = serde_json::to_string(&gate).unwrap();
        assert!(json.contains("command"));
        assert!(json.contains("fmt"));
    }
}
