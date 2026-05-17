# MiniMax Agent

MiniMax is an HTTP provider. Maestro uses an OpenAI-compatible chat stream,
reads the API key from an environment variable, and defaults to `MiniMax-M2.7`.

## API Key

Create an API key in the MiniMax developer platform, then export it before
launching Maestro:

```bash
export MINIMAX_API_KEY="..."
```

Use a different environment variable name only if you also set `api_key_env`.

## Configuration

```toml
[agents]
default = "minimax"

[agents.minimax]
kind = "minimax"
enabled = true
model = "MiniMax-M2.7"
base_url = "https://api.minimax.io/v1"
request_timeout_secs = 120
api_key_env = "MINIMAX_API_KEY"
```

Defaults:

- `model = "MiniMax-M2.7"`
- `base_url = "https://api.minimax.io/v1"`
- `request_timeout_secs = 120`
- `api_key_env = "MINIMAX_API_KEY"`

## Model Choice

| Model | Use When |
| --- | --- |
| `MiniMax-M2.7` | Default long-context coding and agent work |
| `MiniMax-M2.7-highspeed` | Lower latency is more important than default routing |
| `MiniMax-M2.5` | Your account or workload is pinned to the older M2.5 family |
| `MiniMax-M2.1` | Compatibility with older MiniMax deployments matters |

Only configure models that your MiniMax account and endpoint support.

## Doctor Output

Healthy:

```text
agent minimax    OK      MiniMax models endpoint reachable; model `MiniMax-M2.7` configured
```

Missing or invalid key:

```text
agent minimax    FAIL    invalid MINIMAX_API_KEY - check your key at platform.minimax.io
```

## Usage

```bash
maestro run --prompt "Implement a focused bug fix" --agent minimax
```

## Troubleshooting

- **Missing key**: export the variable named by `api_key_env` in the same shell
  that starts Maestro.
- **401 unauthorized**: create a new key, confirm the account has API access,
  and verify there are no extra quotes or spaces in the environment variable.
- **Model rejected**: switch back to `MiniMax-M2.7` and confirm other model ids
  in MiniMax's platform docs before changing the config.
- **Rate limits**: reduce `sessions.max_concurrent`, wait for the quota window,
  or use Ollama/OpenCode for lower-priority work.

## Using `minimax` in a team binding

MiniMax's 204k-token context window makes it useful for the docs and researcher
roles, where the prompt may carry a large repo slice. Like Ollama, it is an
HTTP provider — Maestro sends the role prompt as a chat-completions user
message, so MiniMax is not appropriate for roles that need CLI-only behavior
(interactive prompts, subprocess sandboxes).

```toml
# ~/.config/maestro/maestro/teams/longctx-researcher.toml
extends = "default-researcher"

implementer = "minimax"
```

To bind MiniMax to the docs role with a stricter prompt:

```toml
[role_overrides.docs]
agent = "minimax"
prompt_addendum = "Quote source paths inline; never invent file names."
fallback_agent = "claude"
```

`api_key_env`, `base_url`, and `request_timeout_secs` come from
`[agents.minimax]`. See [`docs/teams/`](../teams/README.md) for the full preset
schema.
