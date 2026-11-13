//! Render the `maestro.toml` template for a detected stack list. The
//! body of the file (sessions/budget/github/gates/...) carries forward
//! from the legacy `cmd_init` template; only the `[project]` block and
//! `[gates].test_command` change with the detected stack.

use super::DetectedStack;
use crate::provider::types::ProviderKind;

/// Per-stack default commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackDefaults {
    pub language: &'static str,
    pub build_command: &'static str,
    pub test_command: &'static str,
    pub run_command: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderTemplate {
    pub kind: ProviderKind,
    pub organization: Option<String>,
    pub az_project: Option<String>,
}

impl ProviderTemplate {
    pub fn github() -> Self {
        Self {
            kind: ProviderKind::Github,
            organization: None,
            az_project: None,
        }
    }

    pub fn azure_devops(organization: String, az_project: String) -> Self {
        Self {
            kind: ProviderKind::AzureDevops,
            organization: Some(organization),
            az_project: Some(az_project),
        }
    }
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
    render_with_primary(stacks, None, &ProviderTemplate::github(), None)
}

pub fn render_with_provider(stacks: &[DetectedStack], provider: &ProviderTemplate) -> String {
    render_with_primary(stacks, None, provider, None)
}

/// Like [`render_with_provider`], but lets the caller override the
/// primary stack's defaults and fill `project.repo`. `primary` replaces
/// `StackDefaults::for_stack(stacks[0])` when present (used by
/// `render_or_merge` to carry repo-refined Python commands); `repo` is
/// the `owner/name` slug parsed from the `origin` remote, or `None` to
/// leave the key empty for manual fill. An empty `run_command` on the
/// resolved defaults emits the default as a comment instead of a key
/// the first run would trip over.
pub fn render_with_primary(
    stacks: &[DetectedStack],
    primary: Option<&StackDefaults>,
    provider: &ProviderTemplate,
    repo: Option<&str>,
) -> String {
    let project_block = render_project_block(stacks, primary, repo);
    let gates_test_command = primary.map(|d| d.test_command).unwrap_or_else(|| {
        stacks
            .first()
            .map(|s| StackDefaults::for_stack(*s).test_command)
            .unwrap_or("")
    });
    let provider_block = render_provider_block(provider);

    format!(
        r#"{project_block}
[sessions]
max_concurrent = 3
stall_timeout_secs = 300
default_model = "opus"
default_mode = "orchestrator"
permission_mode = "default"  # Options: default, acceptEdits, bypassPermissions, dontAsk, plan, auto
allowed_tools = []                      # Empty = all tools. Example: ["Bash", "Read", "Write", "Edit"]
[agents]
default = "claude"
[agents.claude]
kind = "claude"
enabled = true
command = "claude"
# model, permission_mode and allowed_tools fall back to [sessions].
# model = "opus"
# permission_mode = "bypassPermissions"
# allowed_tools = []

# [agents.codex]
# kind = "codex"
# enabled = false
# command = "codex"
# model = "gpt-5.4-codex"
# permission_mode = "yolo"
# sandbox = "workspace-write"
# json = true
# ephemeral = false
# extra_args = []
# [agents.opencode]
# kind = "opencode"
# enabled = false
# command = "opencode"
# model = "anthropic/claude-sonnet-4-5"
# extra_args = []
# [agents.ollama]
# kind = "ollama"
# enabled = false
# model = "qwen3"
# base_url = "http://localhost:11434"
# request_timeout_secs = 120
# [agents.minimax]
# kind = "minimax"
# enabled = false
# model = "MiniMax-M2.7"
# base_url = "https://api.minimax.io/v1"
# request_timeout_secs = 120
# api_key_env = "MINIMAX_API_KEY"
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

{provider_block}

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

[views]
agent_graph_enabled = true

[flags]
# continuous_mode = true   # default: true
# auto_fork = true         # default: true
# ci_auto_fix = false      # default: false
"#
    )
}

fn render_provider_block(provider: &ProviderTemplate) -> String {
    let kind = match provider.kind {
        ProviderKind::Github => "github",
        ProviderKind::AzureDevops => "azure_devops",
    };
    let mut block = format!(
        "[provider]\n\
         kind = \"{kind}\"\n\
         issue_filter_labels = [\"maestro:ready\"]\n\
         auto_pr = true\n\
         auto_merge = false\n\
         merge_method = \"squash\"\n\
         cache_ttl_secs = 300\n"
    );
    if let Some(org) = provider.organization.as_deref() {
        block.push_str(&format!("organization = {}\n", toml_basic_string(org)));
    }
    if let Some(project) = provider.az_project.as_deref() {
        block.push_str(&format!("az_project = {}\n", toml_basic_string(project)));
    }
    block
}

fn toml_basic_string(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn render_project_block(
    stacks: &[DetectedStack],
    primary: Option<&StackDefaults>,
    repo: Option<&str>,
) -> String {
    let repo_line = match repo {
        Some(slug) => format!("repo = {}", toml_basic_string(slug)),
        None => String::from("repo = \"\""),
    };

    if stacks.is_empty() {
        return format!(
            r#"[project]
{repo_line}
base_branch = "main"
# TODO: no project markers detected — fill these in manually.
# language = "rust" | "node" | "python" | "go"
# build_command = ""
# test_command = ""
# run_command = """#,
        );
    }

    let fallback = StackDefaults::for_stack(stacks[0]);
    let resolved = primary.unwrap_or(&fallback);
    let language_line = format!("language = \"{}\"", resolved.language);

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

    let mut block = format!("[project]\n{repo_line}\nbase_branch = \"main\"\n");
    block.push_str(&language_line);
    block.push('\n');
    if !languages_line.is_empty() {
        block.push_str(&languages_line);
        block.push('\n');
    }
    block.push_str(&format!("build_command = \"{}\"\n", resolved.build_command));
    block.push_str(&format!("test_command = \"{}\"\n", resolved.test_command));
    if resolved.run_command.is_empty() {
        // Refined-away default (#1150): no entry point detected, so the
        // command is written as a comment the user must resolve instead
        // of a key whose first run fails.
        block.push_str(
            "# run_command = \"python main.py\"  # TODO: no main.py detected — fill in manually.\n",
        );
    } else {
        block.push_str(&format!("run_command = \"{}\"\n", resolved.run_command));
    }
    block
}

#[cfg(test)]
#[path = "template_tests.rs"]
mod tests;
