# Configuration Reference

Maestro reads `maestro.toml` from the project root. This page documents the
multi-agent configuration added for Claude, Codex, Qwen, OpenCode, Ollama, and
MiniMax. For a complete working example, see
`examples/multi-agent/maestro.toml`.

## Agents Table

```toml
[agents]
default = "claude"
```

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `default` | string | `"claude"` | Agent id used when `maestro run --agent` is omitted. |

If `[agents]` is absent, Maestro uses an implicit Claude agent built from
`[sessions]`. If `[agents]` is present, `default` must reference an enabled
entry.

## Agent Entry

Each `[agents.<id>]` table has these fields:

| Field | Applies To | Type | Default | Description |
| --- | --- | --- | --- | --- |
| `kind` | all | string | required | One of `claude`, `codex`, `qwen`, `opencode`, `ollama`, `minimax`. |
| `enabled` | all | bool | `true` | Disabled agents are ignored and cannot be selected. |
| `command` | subprocess | string | provider binary for Codex/Qwen/OpenCode; required for Claude | CLI command or full path. Invalid for HTTP agents. |
| `base_url` | HTTP | string | Ollama: `http://localhost:11434`; MiniMax: `https://api.minimax.io/v1` | HTTP endpoint. Invalid for subprocess agents. |
| `model` | all | string | Claude inherits `[sessions].default_model`; MiniMax defaults to `MiniMax-M2.7`; Ollama requires one | Provider model id. |
| `env` | subprocess | table | `{}` | Environment variables added to the subprocess. |
| `extra_args` | subprocess | array of strings | `[]` | Extra CLI arguments appended before the prompt. |
| `permission_mode` | Claude, Codex, Qwen | string | inherits `[sessions].permission_mode` for these kinds when absent | Permission or approval mode mapping. |
| `allowed_tools` | Claude | array of strings | inherits `[sessions].allowed_tools` when absent | Passed to Claude as `--allowedTools` when non-empty. |
| `sandbox` | Codex | string | `workspace-write` | Passed to Codex as `--sandbox`. |
| `json` | Codex | bool | `true` | Adds `--json` for streamed runs. |
| `ephemeral` | Codex | bool | `false` | Adds `--ephemeral`. |
| `profile` | Codex | string | unset | Adds `--profile <name>`. |
| `config_overrides` | Codex | table | `{}` | Each key becomes `--config key=value`. |
| `cli_flags` | reserved | table | `{}` | Parsed and preserved for future provider-specific flags. |
| `request_timeout_secs` | HTTP | integer | `120` | HTTP request timeout. |
| `api_key_env` | HTTP | string | MiniMax: `MINIMAX_API_KEY`; Ollama: unset | Environment variable used for bearer auth when configured. |

Subprocess agents require `command` and reject `base_url`. HTTP agents require
`base_url` and reject `command`.

## Claude

```toml
[agents.claude]
kind = "claude"
enabled = true
command = "claude"
model = "opus"
permission_mode = "bypassPermissions"
allowed_tools = []
```

## Codex

```toml
[agents.codex]
kind = "codex"
enabled = false
command = "codex"
model = "gpt-5.4-codex"
permission_mode = "yolo"
sandbox = "workspace-write"
json = true
ephemeral = false
profile = "work"
extra_args = ["--reasoning-effort", "high"]

[agents.codex.config_overrides]
approval_policy = "never"
```

## Qwen

```toml
[agents.qwen]
kind = "qwen"
enabled = false
command = "qwen"
model = "qwen-test"
extra_args = ["--auth-type", "openai"]

[agents.qwen.env]
OPENAI_BASE_URL = "https://api.example.com/v1"
```

## OpenCode

```toml
[agents.opencode]
kind = "opencode"
enabled = false
command = "opencode"
model = "openrouter/deepseek/deepseek-chat"
extra_args = []
```

OpenCode model ids should use `provider/model` format.

## Ollama

```toml
[agents.ollama]
kind = "ollama"
enabled = false
model = "qwen3"
base_url = "http://localhost:11434"
request_timeout_secs = 120
```

## MiniMax

```toml
[agents.minimax]
kind = "minimax"
enabled = false
model = "MiniMax-M2.7"
base_url = "https://api.minimax.io/v1"
request_timeout_secs = 120
api_key_env = "MINIMAX_API_KEY"
```

## Doctor Behavior

`maestro doctor` validates provider setup:

- With no explicit `[agents]`, it checks the implicit Claude CLI.
- With explicit `[agents]`, it checks every enabled agent.
- The default enabled agent is required.
- Other enabled agents are optional warnings.

Use `enabled = false` for configured examples that are not ready to run.
