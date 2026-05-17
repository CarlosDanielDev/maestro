# Qwen Agent

Qwen is a subprocess provider. Maestro runs Qwen in non-interactive streaming
mode:

```text
qwen --bare --output-format stream-json --include-partial-messages ...
```

## Install And Authenticate

Install the Qwen CLI and configure the provider credentials it should use. Then
verify:

```bash
qwen --version
```

Run:

```bash
maestro doctor
```

A healthy Qwen check looks like:

```text
agent qwen       OK      <qwen version>
```

## Configuration

```toml
[agents]
default = "qwen"

[agents.qwen]
kind = "qwen"
enabled = true
command = "qwen"
model = "qwen-test"
permission_mode = "bypassPermissions"
extra_args = ["--auth-type", "openai"]

[agents.qwen.env]
OPENAI_BASE_URL = "https://api.example.com/v1"
```

Set API keys in your shell, `.env`, or Qwen's own settings rather than passing
them through `extra_args`; command-line arguments may be visible in process
listings.

## Field Behavior

- `model` is passed as `--model`.
- `permission_mode` maps to Qwen's approval mode when supported by Maestro.
- `extra_args` are appended before `--prompt`.
- `env` values are added to the Qwen subprocess environment.

## Usage

```bash
maestro run --prompt "Review the config loader" --agent qwen
```

## Troubleshooting

- `not installed`: install Qwen or set `command` to the full binary path.
- Authentication errors: run Qwen directly with the same `extra_args` and
  environment.
- Empty or malformed output: confirm your Qwen version supports
  `--output-format stream-json`.

## Using `qwen` in a team binding

Qwen is well-suited to the reviewer or docs role, where you want a second
opinion without spending Claude tokens. The implementer typically stays on
Claude or Codex.

```toml
# ~/.config/maestro/maestro/teams/qwen-reviewer.toml
extends = "default-coder"

reviewer = "qwen"
```

To pair Qwen with a stricter review prompt:

```toml
[role_overrides.reviewer]
agent = "qwen"
prompt_addendum = "List risks only — skip praise and restating diffs."
```

Auth keys, base URLs, and `extra_args` live in `[agents.qwen]`. See
[`docs/teams/`](../teams/README.md) for the full preset schema.
