# Claude Agent

Claude is Maestro's default subprocess provider. If your `maestro.toml` omits
`[agents]`, Maestro still runs Claude by using `[sessions].default_model`,
`[sessions].permission_mode`, and `[sessions].allowed_tools`.

## Install

Install and authenticate the Claude CLI, then verify it is on `PATH`:

```bash
claude --version
```

Run:

```bash
maestro doctor
```

A healthy Claude check looks like:

```text
agent claude     OK      <claude version>
```

## Configuration

Minimal explicit Claude agent:

```toml
[agents]
default = "claude"

[agents.claude]
kind = "claude"
enabled = true
command = "claude"
model = "opus"
permission_mode = "bypassPermissions"
allowed_tools = []
```

`model` can also be inherited from `[sessions].default_model` when omitted.
`permission_mode` is passed to Claude as `--permission-mode` unless it is
`default`; `allowed_tools` is passed as `--allowedTools` when non-empty.

## Usage

```bash
maestro run --prompt "Refactor the parser" --agent claude
maestro run --issue 42 --agent claude
```

## Troubleshooting

- `not installed`: install the Claude CLI or set `command` to the full binary
  path.
- Authentication failures: run the Claude CLI directly and complete its login
  flow.
- Permission prompts block automation: choose the project-appropriate
  `permission_mode`, such as `bypassPermissions`, or restrict tools with
  `allowed_tools`.

## Using `claude` in a team binding

Claude is the built-in default for every role in every preset. Override it only
when another provider is genuinely a better fit; otherwise leave the role bound
to `claude`.

Minimal-form binding (top-level role keys):

```toml
# ~/.config/maestro/maestro/teams/claude-pipeline.toml
extends = "default-coder"

implementer = "claude"
reviewer = "claude"
docs = "claude"
```

Rich-form binding with a model override:

```toml
[role_overrides.implementer]
agent = "claude"
model_override = "opus"
prompt_addendum = "Stay terse; do not explain unchanged code."
```

`model_override` is set on the team binding; the underlying `[agents.claude]`
table still owns `command`, `permission_mode`, and `allowed_tools`. See
[`docs/teams/`](../teams/README.md) for the full preset schema.
