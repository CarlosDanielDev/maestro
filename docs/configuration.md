# Configuration Reference

Single source of truth for every key Maestro reads from `maestro.toml`.

- For onboarding (`maestro init`), see [`docs/agents/mod.md`](agents/mod.md).
- For team presets and the `maestro team` subcommands, see [`docs/teams/README.md`](teams/README.md).
- For per-provider deep dives (Claude, Codex, Qwen, OpenCode, Ollama, MiniMax), see [`docs/agents/`](agents/).
- For the canonical-template render system that produces `.claude/commands/`, see [`docs/templates.md`](templates.md).

## File location

Maestro looks for the config under the **current working directory only**, in this order, stopping at the first match:

1. `./maestro.toml`
2. `./.maestro/config.toml`

If neither exists, Maestro aborts with:

```
No maestro.toml found under <cwd>. Run `maestro init` to create one.
```

Source: `Config::find_and_load_in_with_path` (`src/config/mod.rs`). There is no XDG / `$HOME` / `--config` fallback today. Team presets and built-in subagents resolve from additional paths (`~/.config/maestro/teams/`, `<repo>/.maestro/teams/`) — those paths are documented in [`docs/teams/README.md`](teams/README.md), not here.

## Startup migration

On every invocation other than `init`, `completions`, and `mangen`, Maestro runs a single-step migrator (`config::run_startup_migration`, `src/config/migrate.rs`) that backfills `views.agent_graph_enabled = true` when missing. Explicit `false` is preserved.

## Conventions used below

- **Type** uses TOML names (`string`, `integer`, `bool`, `float`, `array`, `table`, `array of table`, `string enum`).
- **Default** of `—` means the field is required (no `Default` impl and no `#[serde(default)]`).
- All section headings match the literal `[table]` name in `maestro.toml`. Sub-tables appear as their own H3 in the parent section.
- Sections are in alphabetical order of the table heading.
- For every section the parenthetical *Source:* footer points at the Rust definition. If the source changes and this doc does not, the doc is wrong — update both.

## Minimal example

```toml
[project]
repo = "owner/repo"
base_branch = "main"

[sessions]

[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80

[notifications]
```

This is the smallest config that `Config::load` accepts. Every other table is `#[serde(default)]`.

---

## `[adapt]`

Controls `maestro adapt`, the project-onboarding command.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `milestone_naming` | string enum | `"ai"` | One of `standard` (infer from existing semver milestones), `ai` (let the model choose), `custom` (use `milestone_template`). |
| `milestone_template` | string | unset | Used only when `milestone_naming = "custom"`. Supports `{n}` (index) and `{title}` (description) placeholders. |

```toml
[adapt]
milestone_naming = "custom"
milestone_template = "M{n}: {title}"
```

*Source: `src/config/adapt.rs`.*

## `[agents]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `default` | string | `"claude"` | Agent id used when `maestro run --agent` is omitted. Must reference an enabled `[agents.<id>]` entry when the table is present. |

If `[agents]` is absent, Maestro uses an implicit Claude agent built from `[sessions]`. If `[agents]` is present, `default` must reference an enabled entry.

### `[agents.<id>]` entries

Each `[agents.<id>]` table has these fields:

| Field | Applies To | Type | Default | Description |
| --- | --- | --- | --- | --- |
| `kind` | all | string enum | — | One of `claude`, `codex`, `qwen`, `opencode`, `ollama`, `minimax`. |
| `enabled` | all | bool | `true` | Disabled agents are ignored and cannot be selected. |
| `command` | subprocess | string | provider binary for Codex/Qwen/OpenCode; required for Claude | CLI command or full path. Rejected for HTTP agents. |
| `base_url` | HTTP | string | Ollama: `http://localhost:11434`; MiniMax: `https://api.minimax.io/v1` | HTTP endpoint. Rejected for subprocess agents. |
| `model` | all | string | Claude inherits `[sessions].default_model`; MiniMax defaults to `MiniMax-M2.7`; Ollama requires one | Provider model id. |
| `env` | subprocess | table | `{}` | Environment variables added to the subprocess. |
| `extra_args` | subprocess | array of string | `[]` | Extra CLI arguments appended before the prompt. |
| `permission_mode` | Claude, Codex, Qwen | string | inherits `[sessions].permission_mode` when absent | Permission/approval mode mapping. |
| `allowed_tools` | Claude | array of string | inherits `[sessions].allowed_tools` when absent | Passed to Claude as `--allowedTools` when non-empty. |
| `sandbox` | Codex | string | `"workspace-write"` | Passed to Codex as `--sandbox`. |
| `json` | Codex | bool | `true` | Adds `--json` for streamed runs. |
| `ephemeral` | Codex | bool | `false` | Adds `--ephemeral`. |
| `profile` | Codex | string | unset | Adds `--profile <name>`. |
| `config_overrides` | Codex | table | `{}` | Each key becomes `--config key=value`. |
| `cli_flags` | reserved | table | `{}` | Parsed and preserved for future provider-specific flags. |
| `request_timeout_secs` | HTTP | integer | `120` | HTTP request timeout. |
| `api_key_env` | HTTP | string | MiniMax: `MINIMAX_API_KEY`; Ollama: unset | Environment variable used for bearer auth. |

Subprocess agents (`claude`, `codex`, `qwen`, `opencode`) require `command` and reject `base_url`. HTTP agents (`ollama`, `minimax`) require `base_url` and reject `command`.

Per-provider walkthroughs:

- [Claude](agents/claude.md)
- [Codex](agents/codex.md)
- [Qwen](agents/qwen.md)
- [OpenCode](agents/opencode.md)
- [Ollama](agents/ollama.md)
- [MiniMax](agents/minimax.md)

Minimal multi-agent block:

```toml
[agents]
default = "claude"

[agents.claude]
kind = "claude"
command = "claude"
model = "opus"
permission_mode = "bypassPermissions"

[agents.codex]
kind = "codex"
enabled = false
command = "codex"
model = "gpt-5.4-codex"
sandbox = "workspace-write"

[agents.ollama]
kind = "ollama"
enabled = false
base_url = "http://localhost:11434"
model = "qwen3"
```

`maestro doctor` validates provider setup: with no explicit `[agents]`, it checks the implicit Claude CLI; with explicit `[agents]`, it checks every enabled agent; the default agent is required, others are optional warnings. Use `enabled = false` for configured examples that are not ready to run.

*Source: `src/config/agents.rs`.*

## `[budget]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `per_session_usd` | float | `5.0` | Soft cap per session. |
| `total_usd` | float | `50.0` | Soft cap across all sessions in the run. |
| `alert_threshold_pct` | integer | `80` | Percentage of either cap at which the budget warning fires. |

```toml
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
```

*Source: `src/config/budget.rs`.*

## `[concurrency]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `heavy_task_labels` | array of string | `[]` | Issue labels that mark a task as resource-intensive. |
| `heavy_task_limit` | integer | `2` | Max concurrent "heavy" tasks (independent of `[sessions].max_concurrent`). |
| `team_max_parallel` | integer | unset | Optional cap on parallel team runs. Today `maestro team launch --max-parallel` defaults to `3` and ignores this key; tracked as a follow-up. |

```toml
[concurrency]
heavy_task_labels = ["heavy", "migration"]
heavy_task_limit = 2
```

*Source: `src/config/runtime.rs`.*

## `[experimental]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `azure_devops` | bool | `true` | Retained for backwards compatibility with pre-v0.24.0 configs. Azure DevOps is stable; this flag no longer gates anything. Explicit `false` is accepted and logged at `debug`. |

*Source: `src/config/experimental.rs`.*

## `[flags]`

Arbitrary `key = bool` entries that merge with the built-in flag defaults (see `src/flags/store.rs`). The `maestro run --enable-flag <FLAG>` and `--disable-flag <FLAG>` options layer on top with **disable wins** semantics when both are supplied for the same flag in the same invocation.

Documented flags shipped with the binary (defaults shown in source): `continuous_mode`, `auto_fork`, `ci_auto_fix`. Unknown keys are accepted and stored verbatim; they have no effect until code reads them.

```toml
[flags]
ci_auto_fix = true
auto_fork = false
```

*Source: `src/config/flags.rs`, `src/flags/store.rs`.*

## `[gates]`

Completion gates run after a session finishes and before PR creation. See also `[sessions.completion_gates]` for the in-session variant.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | Master switch for gates. |
| `test_command` | string | `"cargo test"` | Default test command. `maestro init` rewrites this for non-Rust stacks. |
| `ci_poll_interval_secs` | integer | `30` | Seconds between CI status polls. |
| `ci_max_wait_secs` | integer | `1800` | Hard timeout (30 min) waiting for CI. |

### `[gates.ci_auto_fix]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | Whether the CI auto-fix loop is active. |
| `max_retries` | integer | `3` | Maximum auto-fix attempts per PR. |

```toml
[gates]
enabled = true
test_command = "cargo test"
ci_poll_interval_secs = 30
ci_max_wait_secs = 1800

[gates.ci_auto_fix]
enabled = true
max_retries = 3
```

*Source: `src/config/gates.rs`.*

## `[github]`

Legacy block for GitHub-only configs. Prefer `[provider]` with `kind = "github"` for new configs — `[github]` exists so pre-v0.22.0 files keep parsing without edits.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `issue_filter_labels` | array of string | `["maestro:ready"]` | Labels Maestro pulls from the work queue. |
| `auto_pr` | bool | `true` | Auto-open a PR on session completion. |
| `cache_ttl_secs` | integer | `300` | Issue-cache TTL in seconds. |
| `auto_merge` | bool | `false` | Auto-merge PRs once gates pass. |
| `merge_method` | string enum | `"squash"` | One of `merge`, `squash`, `rebase`. |

```toml
[github]
issue_filter_labels = ["maestro:ready"]
auto_pr = true
cache_ttl_secs = 300
auto_merge = false
merge_method = "squash"
```

*Source: `src/config/github.rs`.*

## `[models]`

Label-pattern → model-name routing. First match wins. Used by `maestro run` and the worker dispatcher to pick a model based on the issue's labels.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `routing` | table of string → string | `{}` | Label pattern to model id. |

```toml
[models]
routing = { "priority:P0" = "opus", "type:docs" = "haiku" }
```

*Source: `src/config/models.rs`.*

## `[modes]`

A free-form map of mode id to per-mode overrides. Each `[modes.<id>]` registers a custom session mode that `maestro run --mode <id>` can select.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `system_prompt` | string | `""` | Prompt prefix injected into every session in this mode. |
| `allowed_tools` | array of string | `[]` | Tool whitelist; empty = all. |
| `permission_mode` | string | unset | Overrides `[sessions].permission_mode` for this mode. |

```toml
[modes.review]
system_prompt = "You are reviewing a pull request."
allowed_tools = ["Read", "Grep"]
permission_mode = "default"
```

*Source: `src/config/modes.rs`.*

## `[monitoring]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `work_tick_interval_secs` | integer | `10` | Cadence of work-assigner ticks. |

*Source: `src/config/runtime.rs`.*

## `[notifications]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `desktop` | bool | `true` | OS-native desktop notifications. |
| `slack` | bool | `false` | Slack webhook delivery. |
| `slack_webhook_url` | string | unset | Webhook URL. Required when `slack = true`. |
| `slack_rate_limit_per_min` | integer | `10` | Slack messages-per-minute cap. |

```toml
[notifications]
desktop = true
slack = false
```

*Source: `src/config/notifications.rs`.*

## `[[plugins]]`

Array of plugin entries. Each runs a shell command on a hook point.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `name` | string | — | Display name. |
| `on` | string | — | Hook point (e.g. `session_completed`, `pr_created`). |
| `run` | string | — | Shell command to execute. |
| `timeout_secs` | integer | unset | Per-plugin timeout in seconds. |

```toml
[[plugins]]
name = "notify-team"
on = "pr_created"
run = "scripts/notify.sh"
timeout_secs = 30
```

*Source: `src/config/plugins.rs`.*

## `[project]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `repo` | string | `""` | `owner/repo` slug. Threaded into `gh --repo` shellouts. |
| `base_branch` | string | `"main"` | Default branch for PRs and worktree base. |
| `language` | string | unset (auto-detected by `maestro init`) | Primary stack id (`rust`, `node`, `python`, `go`). |
| `languages` | array of string | unset | All detected stacks when polyglot. |
| `build_command` | string | unset | Stack-appropriate build command (e.g. `cargo build`). |
| `test_command` | string | unset | Stack-appropriate test command (e.g. `cargo test`). |
| `run_command` | string | unset | Stack-appropriate run command (e.g. `cargo run`). |

```toml
[project]
repo = "myorg/myrepo"
base_branch = "main"
language = "rust"
build_command = "cargo build"
test_command = "cargo test"
run_command = "cargo run"
```

*Source: `src/config/project.rs`.*

## `[provider]`

Provider-neutral block. Use this instead of `[github]` for new configs.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `kind` | string enum | `"github"` | `github` or `azure_devops`. |
| `repo` | string | unset (falls back to `[project].repo`) | `owner/repo` slug. |
| `issue_filter_labels` | array of string | `["maestro:ready"]` | Labels pulled from the queue. |
| `auto_pr` | bool | `true` | Auto-open a PR on completion. |
| `auto_merge` | bool | `false` | Auto-merge once gates pass. |
| `merge_method` | string enum | `"squash"` | `merge`, `squash`, `rebase`. |
| `cache_ttl_secs` | integer | `300` | Issue-cache TTL. |
| `organization` | string | unset (required for `azure_devops`) | `https://dev.azure.com/<org>` or `https://<org>.visualstudio.com`. |
| `az_project` | string | unset (required for `azure_devops`) | Azure DevOps project name. |

`Config::validate` (`src/config/mod.rs`) rejects Azure DevOps configs whose `organization` does not match the URL regex above, contains control characters, or has an empty `az_project`.

```toml
[provider]
kind = "github"
repo = "myorg/myrepo"
auto_pr = true
auto_merge = false
merge_method = "squash"
```

```toml
[provider]
kind = "azure_devops"
organization = "https://dev.azure.com/myorg"
az_project = "myproject"
repo = "myorg/myrepo"
```

*Source: `src/config/github.rs`, `src/provider/types.rs`.*

## `[review]`

Automated review-dispatch configuration. Triggered after PR creation when `enabled = true`.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | Whether review dispatch runs. |
| `command` | string | `"gh pr review {pr_number} --comment --body 'Automated review by Maestro'"` | Template with `{pr_number}` and `{branch}` placeholders. Used only when `reviewers` is empty. |
| `reviewers` | array of table | `[]` | If non-empty, overrides `command` with a multi-reviewer council. |

### `[[review.reviewers]]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `name` | string | — | Display name. |
| `command` | string | — | Command template (same placeholders as `command`). |
| `required` | bool | `false` | If true, this reviewer's failure blocks merge. |

```toml
[review]
enabled = true

[[review.reviewers]]
name = "lint-bot"
command = "scripts/lint-review.sh {pr_number}"
required = true

[[review.reviewers]]
name = "doc-bot"
command = "scripts/doc-review.sh {pr_number}"
```

*Source: `src/config/review.rs`.*

## `[sessions]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `max_concurrent` | integer | `3` | Concurrent sessions cap. |
| `stall_timeout_secs` | integer | `300` | Seconds before a silent session is declared stalled. |
| `default_model` | string | `"opus"` | Falls through to `[agents.claude].model` when the Claude agent inherits. |
| `default_mode` | string | `"orchestrator"` | Mode used when `maestro run --mode` is omitted. |
| `permission_mode` | string enum | `"default"` | Claude permission flow: `default`, `acceptEdits`, `bypassPermissions`, `dontAsk`, `plan`, `auto`. |
| `allowed_tools` | array of string | `[]` | Tool whitelist passed to Claude. Empty = all. |
| `max_retries` | integer | `2` | Retry cap on failed/stalled sessions. |
| `retry_cooldown_secs` | integer | `60` | Cooldown between retries. |
| `max_prompt_history` | integer | `100` | Prompt-history ring size. |
| `guardrail_prompt` | string | unset (auto-detected from `[project].language`) | Custom guardrail injected into the system prompt. |

### `[sessions.hollow_retry]`

Hollow-completion retry policy.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `policy` | string enum | `"intent-aware"` | `always`, `intent-aware`, `never`. |
| `work_max_retries` | integer | `2` | Retries for work sessions. |
| `consultation_max_retries` | integer | `0` | Retries for consultation sessions. |

Legacy `sessions.hollow_max_retries = N` is auto-merged into this section with a one-shot `tracing::warn` (see `merge_legacy_hollow`, `src/config/sessions.rs`).

### `[sessions.context_overflow]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `overflow_threshold_pct` | integer (0–100) | `70` | Context % at which auto-fork triggers. |
| `auto_fork` | bool | `true` | Whether auto-fork runs at the threshold. |
| `commit_prompt_pct` | integer (0–100) | `50` | Context % at which a periodic-commit prompt fires. |
| `max_fork_depth` | integer | `5` | Hard cap on fork-chain depth. |

### `[sessions.conflict]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | Real-time conflict detection. |
| `policy` | string enum | `"warn"` | `warn`, `pause`, `kill`. |

### `[sessions.completion_gates]`

In-session gates run before the orchestrator releases the worktree.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `true` | Master switch for in-session gates. |
| `commands` | array of table | `[]` | Ordered list of `[[sessions.completion_gates.commands]]` entries. |

### `[[sessions.completion_gates.commands]]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `name` | string | — | Display name (e.g. `fmt`, `clippy`). |
| `run` | string | — | Shell command; exit 0 = pass. |
| `required` | bool | `true` | If false, failure is logged but does not block PR creation. |

```toml
[sessions]
max_concurrent = 3
stall_timeout_secs = 300
default_model = "opus"
default_mode = "orchestrator"
permission_mode = "default"

[sessions.hollow_retry]
policy = "intent-aware"
work_max_retries = 2
consultation_max_retries = 0

[sessions.context_overflow]
overflow_threshold_pct = 70
auto_fork = true
commit_prompt_pct = 50
max_fork_depth = 5

[sessions.conflict]
enabled = true
policy = "warn"

[sessions.completion_gates]
enabled = true

[[sessions.completion_gates.commands]]
name = "fmt"
run = "cargo fmt --check"
required = true

[[sessions.completion_gates.commands]]
name = "clippy"
run = "cargo clippy -- -D warnings"
required = true
```

*Source: `src/config/sessions.rs`.*

## `[teams]`

Per-preset overrides keyed by team name. Built-in presets ship inside the binary and resolve from `~/.config/maestro/teams/` (user tier) and `<repo>/.maestro/teams/` (project tier); the `[teams.<name>]` entries here layer project-tier overrides on top.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `extends` | string | `""` | Parent preset name. Empty means root (built-in). |
| `primitive` | string enum | unset (inherited) | `pipeline`, `fan-out`, `single-pass`, `verdict-only`. Required at root. |
| `min_agents` | array of string | unset (inherited) | Roles that must resolve to an enabled agent. |
| (top-level keys) | string | — | Minimal-form role bindings (e.g. `implementer = "opencode"`). |
| `role_overrides` | table | `{}` | Rich-form bindings: see `[teams.<id>.role_overrides.<role>]`. |

### `[teams.<id>.role_overrides.<role>]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `agent` | string | unset | Agent id for the role. |
| `mode` | string | unset | Session mode override. |
| `model_override` | string | unset | Model id override. |
| `prompt_addendum` | string | unset | Text appended to the role's system prompt. |
| `fallback_agent` | string | unset | Agent id used when the primary fails. |

```toml
[teams.cheap-coder]
extends = "default-coder"
implementer = "opencode"
reviewer = "claude"

[teams.cheap-coder.role_overrides.reviewer]
mode = "review-strict"
prompt_addendum = "Be terse."
fallback_agent = "claude"
```

For tier-resolution rules, the `maestro team` CLI surface, and cookbook walkthroughs, see [`docs/teams/README.md`](teams/README.md) and [`docs/teams/cookbook/`](teams/cookbook/).

*Source: `src/orchestration/team.rs`, `src/orchestration/types.rs`.*

## `[tui]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `ascii_icons` | bool | `false` | Force ASCII glyphs (no Nerd Font). |
| `show_mascot` | bool | `true` | Show the Clawd mascot. |
| `mascot_style` | string enum | `"sprite"` | `sprite` (pixel-art) or `ascii` (Unicode block art). |

### `[tui.layout]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `mode` | string enum | `"vertical"` | `vertical` or `horizontal`. |
| `density` | string enum | `"default"` | `default`, `comfortable`, `compact`. |
| `preview_ratio` | integer (0–100) | `50` | Width % (horizontal mode) or height % (vertical mode) for the preview panel. |
| `activity_log_height` | integer (0–100) | `25` | Activity-log panel height %. |

### `[tui.theme]`

Owned by `crate::tui::theme::ThemeConfig` (`src/tui/theme.rs`). Schema is open for evolution; defaults are stable. See the source for the current shape until a dedicated theme reference exists.

```toml
[tui]
ascii_icons = false
show_mascot = true
mascot_style = "sprite"

[tui.layout]
mode = "vertical"
density = "default"
preview_ratio = 50
activity_log_height = 25
```

*Source: `src/config/tui.rs`, `src/tui/theme.rs`, `src/mascot/mod.rs`.*

## `[turboquant]`

TurboQuant vector-quantization configuration. See [`docs/research/`](research/) for design notes.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | Master switch. |
| `bit_width` | integer (1–8) | `4` | Quantization bit width. |
| `strategy` | string enum | `"turboquant"` | `turboquant`, `polarquant`, `qjl`. |
| `apply_to` | string enum | `"both"` | `keys`, `values`, `both`. |
| `auto_on_overflow` | bool | `false` | Auto-enable on context overflow. |
| `fork_handoff_budget` | integer | `4096` | Token budget for fork-handoff compression. |
| `system_prompt_budget` | integer | `2048` | Token budget for system-prompt compaction. |
| `knowledge_budget` | integer | `4096` | Token budget for knowledge-base compression. |

```toml
[turboquant]
enabled = false
bit_width = 4
strategy = "turboquant"
apply_to = "both"
```

> Note: the `maestro turbo-quant benchmark --bits N` CLI flag and the `bit_width` config key control the same value; the flag wins for benchmark runs. Naming unification is tracked as a follow-up.

*Source: `src/config/turboquant.rs`, `src/turboquant/types.rs`.*

## `[views]`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `agent_graph_enabled` | bool | `true` | Show the agent-graph view (concentric bipartite layout) instead of the default panel grid when two or more sessions are active. Set to `false` to revert to the panel layout. |

```toml
[views]
agent_graph_enabled = true
```

`agent_graph_enabled` defaults to `true` as of v0.25.1 (#710). Older `maestro.toml` files that omit `[views]` or the key are migrated automatically on first startup — Maestro appends the key with value `true` to the existing file. The migration is skipped if the key is already present (regardless of its value), so any explicit `agent_graph_enabled = false` is preserved.

*Source: `src/config/views.rs`, `src/config/migrate.rs`.*

---

## CLI reference

`maestro --help` and each `maestro <subcommand> --help` produce help text. The table below maps each flag to its `maestro.toml` equivalent (if any). Anything in the **Drift** column is a follow-up — file a separate issue, do not paper over it in this doc.

### Global

| Flag | Config equivalent | Notes |
| --- | --- | --- |
| `--bypass-review` | none (session-only by design) | Auto-accepts review corrections. DANGER: edits, commits, and pushes apply without per-suggestion review. Cannot be persisted — set per-invocation only. See #328. |

### `maestro run`

Run sessions from GitHub issues or a prompt.

| Flag | Config equivalent | Notes |
| --- | --- | --- |
| `-p, --prompt <PROMPT>` | — | Prompt sent to the agent when no `--issue` / `--milestone` is given. |
| `-i, --issue <ISSUE>` | — | Issue numbers, comma-separated. |
| `-M, --milestone <MILESTONE>` | — | Pull all issues from this milestone. |
| `-m, --model <MODEL>` | `[sessions].default_model` | Overrides for this invocation. |
| `--agent <AGENT>` | `[agents].default` | Named agent from `[agents]`. |
| `--mode <MODE>` | `[sessions].default_mode`, `[modes.<id>]` | `orchestrator`, `vibe`, `review`, or a custom mode id. |
| `--max-concurrent <N>` | `[sessions].max_concurrent` | Overrides the config for this invocation. |
| `--resume` | — | Resume from saved state after a crash. |
| `--skip-doctor` | — | Skip preflight `doctor` checks. |
| `--image <IMAGES>` | — | Attach an image as visual context. Repeatable. |
| `--once` | — | Exit after all sessions complete (CI/scripting mode). |
| `-C, --continuous` | `[flags].continuous_mode` (when wired) | Auto-advance to the next ready issue after each completion. Pair with `--milestone`. |
| `--enable-flag <FLAG>` | `[flags].<flag> = true` | Repeatable. |
| `--disable-flag <FLAG>` | `[flags].<flag> = false` | Repeatable. Disable wins over enable. |
| `--role <ROLE>` | — | Override role classification: `implementer`, `orchestrator`, `reviewer`, `docs`, `dev_ops`. |
| `--no-splash` | — | Skip the startup splash screen. |

### `maestro queue`, `status`, `cost`, `doctor`, `test-slack`

No flags beyond the global `--bypass-review`. Read state, no config equivalents.

`test-slack` exercises `[notifications].slack_webhook_url` and `[notifications].slack_rate_limit_per_min`.

### `maestro add <ISSUE_NUMBER>`

Add an issue to the work queue manually. Positional `ISSUE_NUMBER` only.

### `maestro init`

Initialize `maestro.toml` in the current directory.

| Flag | Config equivalent | Notes |
| --- | --- | --- |
| `--reset` | rewrites detected fields | Re-runs technology detection on an existing file, preserving custom keys. |
| `--non-interactive` | writes GitHub defaults | Skips provider prompts and remote detection. |

### `maestro clean`

| Flag | Config equivalent | Notes |
| --- | --- | --- |
| `--dry-run` | — | Show what would be cleaned without acting. |

### `maestro logs`

| Flag | Config equivalent | Notes |
| --- | --- | --- |
| `--session <ID>` | — | Full log for a specific session. |
| `--export <PATH>` | — | Export as JSON. |

### `maestro resume`

| Flag | Config equivalent | Notes |
| --- | --- | --- |
| `--session <ID>` | — | Resume a specific session by ID. |

`--role` is intentionally absent on `resume`. The role of the resumed session is recovered from its saved state.

### `maestro completions <SHELL>`

Generate shell completions. `SHELL` is one of `bash`, `elvish`, `fish`, `powershell`, `zsh`.

### `maestro adapt`

Onboard an existing project to the Maestro workflow.

| Flag | Config equivalent | Notes |
| --- | --- | --- |
| `-p, --path <PATH>` | — | Project path (default `.`). |
| `--dry-run` | — | Preview without changes. |
| `--no-issues` | — | Analyze and plan, but do not create issues. |
| `--scan-only` | — | Run Phase 1 only; output project profile as JSON. |
| `-m, --model <MODEL>` | `[sessions].default_model` | Model for analysis and planning. |
| `--source <SOURCE>` | — | Where the PRD lives: `local`, `github`, `azure`, or `both` (default `local`). |

### `maestro prd`

Generate a Product Requirements Document.

| Flag | Config equivalent | Notes |
| --- | --- | --- |
| `-p, --path <PATH>` | — | Project path (default `.`). |
| `-m, --model <MODEL>` | `[sessions].default_model` | — |
| `--force` | — | Overwrite an existing PRD without confirmation. |
| `--source <SOURCE>` | — | `local`, `github`, `azure`, `both` (default `local`). |

### `maestro sanitize`

Analyze codebase for dead code and code smells.

| Flag | Config equivalent | Notes |
| --- | --- | --- |
| `-p, --path <PATH>` | — | Scan root (default `.`). |
| `-o, --output <FORMAT>` | — | `text`, `json`, `markdown` (default `text`). |
| `-s, --severity <LEVEL>` | — | `critical`, `warning`, `info` (default `info`). |
| `--skip-ai` | — | Skip Phase 2 (AI analysis). |
| `-m, --model <MODEL>` | `[sessions].default_model` | — |

### `maestro turbo-quant benchmark`

Run compression benchmarks.

| Flag | Config equivalent | Notes |
| --- | --- | --- |
| `--dim <DIM>` | — | Vector dimensionality (default `768`). |
| `--count <COUNT>` | — | Number of vectors (default `10000`). |
| `--bits <BITS>` | `[turboquant].bit_width` | Default `4`. |
| `--output <FORMAT>` | — | `text`, `json` (default `text`). |

### `maestro team`

Manage and launch team orchestration presets. See [`docs/teams/README.md`](teams/README.md) for the full surface.

| Subcommand | Notes |
| --- | --- |
| `list [--json]` | List built-in, user, and project presets. |
| `new <NAME> --extends <PARENT> [--tier user\|project] [--implementer …] [--reviewer …] [--docs …]` | Create a new preset by extending an existing one. |
| `launch <PRESET> [--issue N \| --issues N,N] [--yes] [--max-parallel N]` | Launch a team on one or more issues. `--max-parallel` defaults to `3`. |
| `manage [--list]` | Manage user-tier presets. |
| `explain <NAME> [--json]` | Print resolved bindings with provenance per field. |

### `maestro sync-templates`

Render canonical command templates per provider and track drift. See [`docs/templates.md`](templates.md).

| Flag | Config equivalent | Notes |
| --- | --- | --- |
| `--provider <ID>` | — | Filter to a single provider id (default: all configured). |
| `--check` | — | CI mode: exit 1 on drift, print unified diff to stderr. |
| `--dry-run` | — | Print intended writes without touching the filesystem. |

---

## Reading this alongside source

Every section above includes a *Source:* footer pointing at the Rust file that owns the type. If the source changes, this doc is wrong — update both.
