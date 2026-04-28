//! Render the `maestro.toml` template for a detected stack list. The
//! body of the file (sessions/budget/github/gates/...) carries forward
//! from the legacy `cmd_init` template; only the `[project]` block and
//! `[gates].test_command` change with the detected stack.

use super::DetectedStack;

/// Per-stack default commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackDefaults {
    pub language: &'static str,
    pub build_command: &'static str,
    pub test_command: &'static str,
    pub run_command: &'static str,
}

impl StackDefaults {
    pub fn for_stack(stack: DetectedStack) -> Self {
        match stack {
            DetectedStack::Rust => Self {
                language: "rust",
                build_command: "cargo build",
                test_command: "cargo test",
                run_command: "cargo run",
            },
            DetectedStack::Node => Self {
                language: "node",
                build_command: "npm run build",
                test_command: "npm test",
                run_command: "npm start",
            },
            DetectedStack::Python => Self {
                language: "python",
                build_command: "python -m build",
                test_command: "pytest",
                run_command: "python main.py",
            },
            DetectedStack::Go => Self {
                language: "go",
                build_command: "go build ./...",
                test_command: "go test ./...",
                run_command: "go run .",
            },
        }
    }
}

/// Render the full `maestro.toml` template for the given detected stacks.
/// The first stack in canonical order is the "primary" — its commands
/// drive `[project].build_command` / `test_command` / `run_command` and
/// `[gates].test_command`. Polyglot detection emits a `languages` array
/// listing every detected stack id.
///
/// Empty `stacks` produces a generic template with commented placeholders.
pub fn render(stacks: &[DetectedStack]) -> String {
    let project_block = render_project_block(stacks);
    let gates_test_command = stacks
        .first()
        .map(|s| StackDefaults::for_stack(*s).test_command)
        .unwrap_or("");

    format!(
        r#"{project_block}
[sessions]
max_concurrent = 3
stall_timeout_secs = 300
default_model = "opus"
default_mode = "orchestrator"
permission_mode = "bypassPermissions"  # Options: default, acceptEdits, bypassPermissions, dontAsk, plan, auto
allowed_tools = []                      # Empty = all tools. Example: ["Bash", "Read", "Write", "Edit"]

[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80

[github]
issue_filter_labels = ["maestro:ready"]
auto_pr = true
auto_merge = false                      # Set to true to auto-merge PRs after CI + review pass
merge_method = "squash"                 # Options: merge, squash, rebase
cache_ttl_secs = 300

[gates]
enabled = true
test_command = "{gates_test_command}"
ci_poll_interval_secs = 30
ci_max_wait_secs = 1800

[notifications]
desktop = true
slack = false
# slack_webhook_url = "https://hooks.slack.com/services/T.../B.../xxx"
# slack_rate_limit_per_min = 10

[review]
enabled = false
command = "gh pr review {{pr_number}} --comment --body 'Automated review by Maestro'"

[concurrency]
heavy_task_labels = []
heavy_task_limit = 2

[monitoring]
work_tick_interval_secs = 10

[flags]
# continuous_mode = true   # default: true
# auto_fork = true         # default: true
# ci_auto_fix = false      # default: false
"#
    )
}

fn render_project_block(stacks: &[DetectedStack]) -> String {
    if stacks.is_empty() {
        return String::from(
            r#"[project]
repo = ""
base_branch = "main"
# TODO: no project markers detected — fill these in manually.
# language = "rust" | "node" | "python" | "go"
# build_command = ""
# test_command = ""
# run_command = """#,
        );
    }

    let primary = StackDefaults::for_stack(stacks[0]);
    let language_line = format!("language = \"{}\"", primary.language);

    let languages_line = if stacks.len() > 1 {
        let ids: Vec<String> = stacks.iter().map(|s| format!("\"{}\"", s.id())).collect();
        let comment_ids: Vec<&str> = stacks.iter().map(|s| s.id()).collect();
        format!(
            "languages = [{}]\n# Detected stacks: {}. `language` is the primary; swap commands as needed.",
            ids.join(", "),
            comment_ids.join(", "),
        )
    } else {
        String::new()
    };

    let mut block = String::from("[project]\nrepo = \"\"\nbase_branch = \"main\"\n");
    block.push_str(&language_line);
    block.push('\n');
    if !languages_line.is_empty() {
        block.push_str(&languages_line);
        block.push('\n');
    }
    block.push_str(&format!("build_command = \"{}\"\n", primary.build_command));
    block.push_str(&format!("test_command = \"{}\"\n", primary.test_command));
    block.push_str(&format!("run_command = \"{}\"\n", primary.run_command));
    block
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_defaults_rust() {
        let d = StackDefaults::for_stack(DetectedStack::Rust);
        assert_eq!(d.build_command, "cargo build");
        assert_eq!(d.test_command, "cargo test");
        assert_eq!(d.run_command, "cargo run");
    }

    #[test]
    fn stack_defaults_node() {
        let d = StackDefaults::for_stack(DetectedStack::Node);
        assert_eq!(d.build_command, "npm run build");
        assert_eq!(d.test_command, "npm test");
        assert_eq!(d.run_command, "npm start");
    }

    #[test]
    fn stack_defaults_python() {
        let d = StackDefaults::for_stack(DetectedStack::Python);
        assert_eq!(d.build_command, "python -m build");
        assert_eq!(d.test_command, "pytest");
        assert_eq!(d.run_command, "python main.py");
    }

    #[test]
    fn stack_defaults_go() {
        let d = StackDefaults::for_stack(DetectedStack::Go);
        assert_eq!(d.build_command, "go build ./...");
        assert_eq!(d.test_command, "go test ./...");
        assert_eq!(d.run_command, "go run .");
    }

    #[test]
    fn template_render_rust_contains_cargo_test() {
        let out = render(&[DetectedStack::Rust]);
        assert!(out.contains("language = \"rust\""), "{out}");
        assert!(out.contains("build_command = \"cargo build\""), "{out}");
        assert!(out.contains("test_command = \"cargo test\""), "{out}");
        assert!(out.contains("run_command = \"cargo run\""), "{out}");
    }

    #[test]
    fn template_render_node_contains_npm_test() {
        let out = render(&[DetectedStack::Node]);
        assert!(out.contains("language = \"node\""), "{out}");
        assert!(out.contains("build_command = \"npm run build\""), "{out}");
        assert!(out.contains("test_command = \"npm test\""), "{out}");
        assert!(out.contains("run_command = \"npm start\""), "{out}");
    }

    #[test]
    fn template_render_python_contains_pytest() {
        let out = render(&[DetectedStack::Python]);
        assert!(out.contains("language = \"python\""), "{out}");
        assert!(out.contains("test_command = \"pytest\""), "{out}");
    }

    #[test]
    fn template_render_go_contains_go_test() {
        let out = render(&[DetectedStack::Go]);
        assert!(out.contains("language = \"go\""), "{out}");
        assert!(out.contains("test_command = \"go test ./...\""), "{out}");
    }

    #[test]
    fn template_render_polyglot_lists_languages() {
        let out = render(&[DetectedStack::Rust, DetectedStack::Node]);
        assert!(out.contains("language = \"rust\""), "{out}");
        assert!(out.contains("languages = ["), "{out}");
        assert!(out.contains("\"rust\""), "{out}");
        assert!(out.contains("\"node\""), "{out}");
        assert!(out.contains("build_command = \"cargo build\""), "{out}");
    }

    #[test]
    fn template_render_gates_test_command_matches_primary_stack() {
        let out = render(&[DetectedStack::Node]);
        // Find the [gates] section and confirm its test_command line.
        let gates_idx = out.find("[gates]").expect("gates section");
        let after_gates = &out[gates_idx..];
        assert!(
            after_gates.contains("test_command = \"npm test\""),
            "expected gates.test_command = npm test in:\n{after_gates}"
        );
    }

    #[test]
    fn template_render_empty_stacks_produces_generic_template() {
        let out = render(&[]);
        assert!(!out.contains("\"cargo build\""), "{out}");
        assert!(!out.contains("\"npm run build\""), "{out}");
        assert!(!out.contains("\"go build ./...\""), "{out}");
        assert!(!out.contains("\"pytest\""), "{out}");
        assert!(out.contains("# "), "{out}");
        assert!(out.contains("build_command"), "{out}");
    }
}
