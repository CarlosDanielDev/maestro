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

## How autogen works

Per-field tables wrapped in `<!-- BEGIN AUTOGEN:NAME --> ... <!-- END AUTOGEN:NAME -->` markers are produced from `src/config/schema/{core,extras}.rs`. The integration test `integration_tests::docs_gen::docs_gen_no_drift` re-renders the markers in-memory and fails CI when the committed file differs from what the schema would emit.

Regenerate after editing a `FieldSchema`:

```
bash scripts/regenerate-docs.sh
```

Hand-written sections outside the markers (location, examples, prose, *Source:* footers, the CLI reference appendix) are preserved verbatim. Sections without markers — `[adapt]`, `[agents]`, `[experimental]`, `[flags]`, `[models]`, `[modes]`, `[[plugins]]`, `[provider]`, `[sessions.completion_gates]`, `[teams]`, `[views]`, plus schema-incomplete tables tracked in a follow-up (see `integration_tests/docs_gen.rs::SCHEMA_BACKFILL_PENDING`) — stay hand-written.

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
| `num_ctx` | Ollama | integer | unset | Context window in tokens passed to Ollama as `num_ctx` (#844). When unset, Ollama's per-model default applies. |

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

**Settings UI:** The **Agents** tab (index 7) in the TUI settings screen (`maestro tui` → Settings → Agents) edits `[agents]` entries interactively via the schema-driven renderer. Each `[agents.<id>]` sub-table appears as a collapsible DynamicMap entry. Saving validates that `agents.default` references an existing entry; a mismatch surfaces as a Save banner error rather than writing a broken config. Implemented in #792. Note: `[agents]` and `[modes]` are intentionally excluded from the AUTOGEN marker system (tracked in `SCHEMA_BACKFILL_PENDING`) because per-provider prose outlives what the schema can emit.

*Source: `src/config/agents.rs`.*

## `[budget]`

<!-- BEGIN AUTOGEN:budget -->
| Field | Type | Default | Description |
|---|---|---|---|
| `per_session_usd` | float (0.1..=100.0, step 0.5) | `5.0` | Hard cap per session before alerts and termination |
| `total_usd` | float (0.1..=1000.0, step 5.0) | `50.0` | Aggregate budget across all sessions |
| `alert_threshold_pct` | int (10..=100, step 5) | `80` | Percentage of budget at which to surface a warning |
<!-- END AUTOGEN:budget -->

```toml
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
```

*Source: `src/config/budget.rs`.*

## `[concurrency]`

<!-- BEGIN AUTOGEN:concurrency -->
| Field | Type | Default | Description |
|---|---|---|---|
| `heavy_task_limit` | int (1..=10) | `2` | Maximum simultaneous heavy tasks |
| `heavy_task_labels` | array of string | `[]` | Labels that mark a task as resource-intensive |
| `team_max_parallel` | int (0..=64) | `0` | Cap on parallel team runs — `0` falls back to the global `maestro team launch --max-parallel` |
<!-- END AUTOGEN:concurrency -->

> `team_max_parallel = 0` falls back to the `maestro team launch --max-parallel` default (currently `3`); the key is honored when set to a positive integer.

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

Documented flags shipped with the binary (defaults shown in source): `continuous_mode`, `auto_fork`, `ci_auto_fix`, `review_council`, `model_routing`, `context_overflow`, `turboquant`. Unknown keys are accepted and stored verbatim; they have no effect until code reads them.

```toml
[flags]
ci_auto_fix = true
auto_fork = false
```

### Built-in flag reference

| Flag | Default | Description |
| --- | --- | --- |
| `continuous_mode` | `true` | Enable automatic issue-to-issue progression (also set via `--continuous`). |
| `auto_fork` | `true` | Automatically fork sessions that exceed the context threshold. |
| `ci_auto_fix` | `false` | Spawn a fix session automatically when CI fails on a maestro-managed PR. |
| `review_council` | `false` | Enable multi-model review council for code review. |
| `model_routing` | `false` | Route tasks to different models based on complexity. |
| `context_overflow` | `false` | Detect and handle context window overflow. |
| `turboquant` | `false` | Enable TurboQuant vector quantization for context compression. |

*Source: `src/config/flags.rs`, `src/flags/store.rs`, `src/flags/mod.rs`.*

## `[gates]`

Completion gates run after a session finishes and before PR creation. See also `[sessions.completion_gates]` for the in-session variant.

<!-- BEGIN AUTOGEN:gates -->
| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | bool | `true` | Run completion gates before creating PRs |
| `test_command` | string | `cargo test` | Command used as the default test gate |
| `ci_poll_interval_secs` | int (5..=300, step 5) | `30` | Seconds between CI status polls |
| `ci_max_wait_secs` | int (60..=7200, step 60) | `1800` | Maximum seconds to wait for CI to finish |
<!-- END AUTOGEN:gates -->

### `[gates.ci_auto_fix]`

<!-- BEGIN AUTOGEN:gates.ci_auto_fix -->
| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | bool | `true` | Spawn a fix session when CI fails on an open PR |
| `max_retries` | int (0..=10) | `3` | How many auto-fix passes to run per PR |
<!-- END AUTOGEN:gates.ci_auto_fix -->

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

<!-- BEGIN AUTOGEN:github -->
| Field | Type | Default | Description |
|---|---|---|---|
| `issue_filter_labels` | array of string | `["maestro:ready"]` | Only fetch issues with at least one of these labels |
| `auto_pr` | bool | `true` | Open a PR automatically when a session finishes cleanly |
| `cache_ttl_secs` | int (30..=3600, step 30) | `300` | How long to cache issue data before refetching |
| `auto_merge` | bool | `false` | Merge PRs automatically once all required checks pass |
| `merge_method` | enum (`merge`, `squash`, `rebase`) | `squash` | Strategy used when merging PRs |
<!-- END AUTOGEN:github -->

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

**Settings UI:** The **Modes** tab (index 8) in the TUI settings screen (`maestro tui` → Settings → Modes) edits `[modes]` entries interactively via the schema-driven renderer. Each `[modes.<id>]` sub-table appears as a collapsible DynamicMap entry. Implemented in #792.

*Source: `src/config/modes.rs`.*

## `[monitoring]`

<!-- BEGIN AUTOGEN:monitoring -->
| Field | Type | Default | Description |
|---|---|---|---|
| `work_tick_interval_secs` | int (1..=120, step 5) | `10` | How often the work assigner ticks |
<!-- END AUTOGEN:monitoring -->

*Source: `src/config/runtime.rs`.*

## `[notifications]`

<!-- BEGIN AUTOGEN:notifications -->
| Field | Type | Default | Description |
|---|---|---|---|
| `desktop` | bool | `true` | Show native desktop notifications on session events |
| `slack` | bool | `false` | Send notifications to Slack via webhook |
| `slack_webhook_url` | string | unset | Incoming-webhook URL — leave empty to disable Slack |
| `slack_rate_limit_per_min` | int (1..=60) | `10` | Cap on Slack messages per minute |
<!-- END AUTOGEN:notifications -->

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

<!-- BEGIN AUTOGEN:project -->
| Field | Type | Default | Description |
|---|---|---|---|
| `repo` | string | unset | GitHub `owner/repo` or Azure DevOps project — used by `gh`/`az` CLI calls |
| `base_branch` | string | `main` | Default branch worktrees fork from |
| `language` | string | unset | Primary detected stack id (e.g. `rust`, `node`, `python`, `go`) — set by `maestro init` |
| `languages` | array of string | `[]` | All detected stack ids when the project is polyglot |
| `build_command` | string | unset | Stack-appropriate build command (e.g. `cargo build`, `npm run build`) |
| `test_command` | string | unset | Stack-appropriate test command (e.g. `cargo test`, `npm test`) |
| `run_command` | string | unset | Stack-appropriate run command (e.g. `cargo run`, `npm start`) |
<!-- END AUTOGEN:project -->

> `language`, `languages`, `build_command`, `test_command`, `run_command` are auto-detected by `maestro init` and may be left unset.

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

<!-- BEGIN AUTOGEN:review -->
| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | bool | `false` | Dispatch the review command on PR creation |
| `command` | string | `gh pr review {pr_number} --comment --body 'Automated review by Maestro'` | Template — `{pr_number}` and `{branch}` are substituted at dispatch |

### `[[review]]` — array-of-tables (order-sensitive)

Each `[[review]]` block defines one entry. The list is **order-sensitive — declaration order is execution order.** Add, remove, or reorder entries via the Settings UI or by hand-editing `maestro.toml`. Each entry has the following fields:

| Field | Type | Default | Description |
|---|---|---|---|
| `name` | string | unset | Display name for the reviewer (e.g. `lint-bot`, `doc-bot`) |
| `command` | string | unset | Command template — `{pr_number}` and `{branch}` are substituted at dispatch |
| `required` | bool | `false` | If true, this reviewer failing blocks merge |
<!-- END AUTOGEN:review -->

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

<!-- BEGIN AUTOGEN:sessions -->
| Field | Type | Default | Description |
|---|---|---|---|
| `max_concurrent` | int (1..=20) | `3` | Maximum simultaneous Claude sessions |
| `stall_timeout_secs` | int (30..=3600, step 30) | `300` | Mark a session stalled after no output for this many seconds |
| `default_mode` | string | `orchestrator` | Session mode used when none is set explicitly |
| `allowed_tools` | array of string | `[]` | Tool whitelist passed to Claude — empty means all tools |
| `max_retries` | int (0..=10) | `2` | Retries for failed or stalled sessions |
| `retry_cooldown_secs` | int (0..=600, step 10) | `60` | Cooldown between retries |
| `max_prompt_history` | int (0..=10000, step 10) | `100` | Maximum prompt-history entries retained per session |
| `session_history_cap` | int (0..=1000) | `10` | How many completed sessions to persist across restarts (0 disables history — top-bar cost resets on relaunch) |
| `guardrail_prompt` | string | unset | Custom guardrail injected into the system prompt — empty falls back to language-based default |
<!-- END AUTOGEN:sessions -->

> `default_model` and `permission_mode` were moved to `[agents.<id>]` in v0.27.0 (per-provider config). The TOML keys still parse for back-compat; new configs set them on the agent entry instead.

### `[sessions.hollow_retry]`

Hollow-completion retry policy.

<!-- BEGIN AUTOGEN:sessions.hollow_retry -->
| Field | Type | Default | Description |
|---|---|---|---|
| `policy` | enum (`always`, `intent-aware`, `never`) | `intent-aware` | When to retry hollow (empty-output) completions |
| `work_max_retries` | int (0..=10) | `2` | Retries for hollow completions in work sessions |
| `consultation_max_retries` | int (0..=10) | `0` | Retries for hollow completions in consultation sessions |
<!-- END AUTOGEN:sessions.hollow_retry -->

Legacy `sessions.hollow_max_retries = N` is auto-merged into this section with a one-shot `tracing::warn` (see `merge_legacy_hollow`, `src/config/sessions.rs`).

### `[sessions.context_overflow]`

<!-- BEGIN AUTOGEN:sessions.context_overflow -->
| Field | Type | Default | Description |
|---|---|---|---|
| `overflow_threshold_pct` | int (10..=100, step 5) | `70` | Context-usage percentage at which auto-fork triggers |
| `auto_fork` | bool | `true` | Automatically fork on overflow instead of stalling |
| `commit_prompt_pct` | int (10..=100, step 5) | `50` | Context percentage at which to prompt a periodic commit |
| `max_fork_depth` | int (1..=20) | `5` | Cap fork chains to prevent runaway forking |
<!-- END AUTOGEN:sessions.context_overflow -->

### `[sessions.conflict]`

<!-- BEGIN AUTOGEN:sessions.conflict -->
| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | bool | `true` | Detect real-time file conflicts between sessions |
| `policy` | enum (`warn`, `pause`, `kill`) | `warn` | What to do when a conflict is detected |
<!-- END AUTOGEN:sessions.conflict -->

### `[sessions.completion_gates]`

In-session gates run before the orchestrator releases the worktree. The **Sessions** tab in the TUI settings screen now exposes `completion_gates` as a `NestedTable` group: the `enabled` toggle and the `commands` row table (backed by `[[sessions.completion_gates.commands]]`) are editable inline without leaving the tab. Use the DynamicRows widget (Add/Remove, Alt+↑/↓ reorder) to manage the command list interactively (#792).

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

<!-- BEGIN AUTOGEN:tui -->
| Field | Type | Default | Description |
|---|---|---|---|
| `ascii_icons` | bool | `false` | Use ASCII-only icons in terminals without emoji support |
| `show_mascot` | bool | `true` | Show the Clawd mascot companion in the TUI |
| `mascot_style` | enum (`sprite`, `ascii`) | `sprite` | Visual style for the mascot — `sprite` is pixel art, `ascii` is Unicode block art |
<!-- END AUTOGEN:tui -->

### `[tui.layout]`

<!-- BEGIN AUTOGEN:tui.layout -->
| Field | Type | Default | Description |
|---|---|---|---|
| `mode` | enum (`vertical`, `horizontal`) | `vertical` | Stack the preview panel below the list or beside it |
| `density` | enum (`default`, `comfortable`, `compact`) | `default` | Information density across all list views |
| `preview_ratio` | int (10..=90, step 5) | `50` | Width or height percentage allocated to the preview panel |
| `activity_log_height` | int (10..=50, step 5) | `25` | Percentage of screen height for the activity log |
<!-- END AUTOGEN:tui.layout -->

### `[tui.theme]`

<!-- BEGIN AUTOGEN:tui.theme -->
| Field | Type | Default | Description |
|---|---|---|---|
| `preset` | enum (`dark`, `light`, `retro`) | `dark` | Color preset applied to the TUI |
<!-- END AUTOGEN:tui.theme -->

### `[tui.theme.overrides]`

Per-field color overrides applied on top of the preset. Each override accepts a named color (`red`, `darkgray`, `lightcyan`, …), a hex string (`#RRGGBB`), or a 256-color index (`0`–`255`). Validation lives in `SerializableColor::deserialize` (`src/tui/theme.rs`); leaving a field `unset` keeps the preset value.

<!-- BEGIN AUTOGEN:tui.theme.overrides -->
| Field | Type | Default | Description |
|---|---|---|---|
| `branding_fg` | string | unset | Foreground color of the maestro branding badge — name, hex, or 256-color index |
| `branding_bg` | string | unset | Background color of the maestro branding badge — name, hex, or 256-color index |
| `text_primary` | string | unset | Primary text color — name, hex, or 256-color index |
| `text_secondary` | string | unset | Secondary text color (subdued labels) — name, hex, or 256-color index |
| `text_muted` | string | unset | Muted text color (deprecated or low-priority text) — name, hex, or 256-color index |
| `border_active` | string | unset | Active panel border color — name, hex, or 256-color index |
| `border_inactive` | string | unset | Inactive panel border color — name, hex, or 256-color index |
| `border_focused` | string | unset | Focused panel border color — name, hex, or 256-color index |
| `accent_success` | string | unset | Success accent color (gates passed, completion) — name, hex, or 256-color index |
| `accent_warning` | string | unset | Warning accent color — name, hex, or 256-color index |
| `accent_error` | string | unset | Error accent color — name, hex, or 256-color index |
| `accent_info` | string | unset | Info accent color — name, hex, or 256-color index |
| `accent_identifier` | string | unset | Identifier accent color (IDs, session keys) — name, hex, or 256-color index |
| `gauge_low` | string | unset | Low-tier gauge color (under 40 percent) — name, hex, or 256-color index |
| `gauge_medium` | string | unset | Medium-tier gauge color — name, hex, or 256-color index |
| `gauge_high` | string | unset | High-tier gauge color — name, hex, or 256-color index |
| `gauge_background` | string | unset | Gauge background color — name, hex, or 256-color index |
| `notification_critical` | string | unset | Critical notification color — name, hex, or 256-color index |
| `notification_blocker` | string | unset | Blocker notification color — name, hex, or 256-color index |
| `notification_default` | string | unset | Default notification color — name, hex, or 256-color index |
| `keybind_key` | string | unset | Keybind hint key color — name, hex, or 256-color index |
| `keybind_label_bg` | string | unset | Keybind hint label background — name, hex, or 256-color index |
| `keybind_label_fg` | string | unset | Keybind hint label foreground — name, hex, or 256-color index |
| `selection_bg` | string | unset | Selected-row background color — name, hex, or 256-color index |
| `selection_fg` | string | unset | Selected-row foreground color — name, hex, or 256-color index |
| `title_accent` | string | unset | Title bar accent color — name, hex, or 256-color index |
| `fkey_badge_bg` | string | unset | F-key badge background color — name, hex, or 256-color index |
| `fkey_badge_fg` | string | unset | F-key badge foreground color — name, hex, or 256-color index |
<!-- END AUTOGEN:tui.theme.overrides -->

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

[tui.theme]
preset = "dark"

[tui.theme.overrides]
text_primary = "magenta"
border_active = "#00ffaa"
```

*Source: `src/config/tui.rs`, `src/config/schema/theme.rs` (autogen), `src/tui/theme.rs`, `src/mascot/mod.rs`.*

## `[turboquant]`

TurboQuant vector-quantization configuration. See [`docs/research/`](research/) for design notes.

<!-- BEGIN AUTOGEN:turboquant -->
| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | bool | `false` | Compress contexts before forking or compacting |
| `bit_width` | int (1..=8) | `4` | Bits per coefficient used by the quantizer |
| `strategy` | enum (`turboquant`, `polarquant`, `qjl`) | `turboquant` | Quantization algorithm |
| `apply_to` | enum (`keys`, `values`, `both`) | `both` | Vector components to compress |
| `auto_on_overflow` | bool | `false` | Enable compression automatically when context overflows |
| `fork_handoff_budget` | int (256..=65536, step 256) | `4096` | Token budget for fork-handoff compression |
| `system_prompt_budget` | int (256..=65536, step 256) | `2048` | Token budget for system-prompt compaction |
| `knowledge_budget` | int (256..=65536, step 256) | `4096` | Token budget for knowledge-base compression |
<!-- END AUTOGEN:turboquant -->

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
