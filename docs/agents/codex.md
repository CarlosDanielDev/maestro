# Codex Agent

Codex is a subprocess provider. Maestro launches `codex exec`, requests JSON
streaming by default, and parses the stream into normal Maestro session events.

## Install And Authenticate

Install the Codex CLI, sign in using the CLI's supported authentication flow,
and verify it is on `PATH`:

```bash
codex --version
codex exec --json "Respond with OK only."
```

Then run:

```bash
maestro doctor
```

A healthy Codex check looks like:

```text
agent codex      OK      <codex version>
```

## Configuration

```toml
[agents]
default = "codex"

[agents.codex]
kind = "codex"
enabled = true
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

[agents.codex.env]
OPENAI_API_KEY = "set-this-in-your-shell-instead"
```

Prefer setting secrets in your shell or secret manager instead of writing them
into `maestro.toml`.

## Field Behavior

- `sandbox` defaults to `workspace-write` and is passed as `--sandbox`.
- `json` defaults to `true` and enables `--json`.
- `ephemeral = true` adds `--ephemeral`.
- `profile` adds `--profile <name>`.
- `config_overrides` entries become repeated `--config key=value` flags.
- `permission_mode = "yolo"` adds `--yolo`.
- `extra_args` are appended before the prompt.

## Usage

```bash
maestro run --prompt "Add tests for the retry policy" --agent codex
```

## Troubleshooting

- `codex exec --json preflight failed`: run the printed command directly to
  complete login, fix model access, or adjust sandbox settings.
- Model errors: verify the configured `model` is available to your account.
- Sandbox errors: try `sandbox = "workspace-write"` first, then tighten the
  sandbox once the command is working.

## Using `codex` in a team binding

Codex is a strong implementer when you want an OpenAI-backed alternative to
Claude. Pair it with a Claude fallback so a transient Codex outage still leaves
the pipeline runnable:

```toml
# .maestro/teams/codex-pipeline.toml
extends = "default-coder"

[role_overrides.implementer]
agent = "codex"
fallback_agent = "claude"
```

Sandbox, profile, and `config_overrides` come from `[agents.codex]`; team
bindings only carry `agent`, `mode`, `model_override`, `prompt_addendum`, and
`fallback_agent`. See [`docs/teams/`](../teams/README.md) for the full preset
schema.
