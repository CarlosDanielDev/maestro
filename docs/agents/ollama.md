# Ollama Agent

Ollama is an HTTP provider. Maestro calls Ollama's local OpenAI-compatible API
and streams responses into normal Maestro session events.

## Install And Start

```bash
brew install ollama
ollama serve
```

In another terminal, pull and verify a model:

```bash
ollama pull qwen3
ollama ls
```

## Configuration

```toml
[agents]
default = "ollama"

[agents.ollama]
kind = "ollama"
enabled = true
model = "qwen3"
base_url = "http://localhost:11434"
request_timeout_secs = 120
```

`base_url` defaults to `http://localhost:11434` and
`request_timeout_secs` defaults to `120`. Increase the timeout for large local
models, slow CPUs, or remote Ollama hosts.

Remote Ollama example:

```toml
[agents.ollama-remote]
kind = "ollama"
enabled = true
model = "llama3.2"
base_url = "http://devbox.local:11434"
request_timeout_secs = 240
```

## Doctor Output

Healthy:

```text
agent ollama     OK      Ollama <version>; model `qwen3` available
```

Model missing:

```text
agent ollama     FAIL    model `qwen3` is not available; run 'ollama pull qwen3'
```

Server not running:

```text
agent ollama     FAIL    <connection error>
```

## Usage

```bash
maestro run --prompt "Summarize the module boundaries" --agent ollama
```

## Troubleshooting

- **Connection refused**: start `ollama serve` or fix `base_url`.
- **Model not pulled**: run `ollama pull <model>` and confirm it appears in
  `ollama ls`.
- **Slow responses**: use a smaller model, enable GPU acceleration if available,
  or raise `request_timeout_secs`.
- **Remote host fails**: confirm the remote Ollama server is reachable from the
  machine running Maestro.

## Using `ollama` in a team binding

Ollama is best for offline iteration or free-tier review/docs roles. Because
it is an HTTP provider, Maestro sends the role prompt as a single user message
to Ollama's `/v1/chat/completions` endpoint — there is no CLI subprocess, no
stdin, and no tool-use plumbing. That makes Ollama a poor fit for roles that need to call CLI-only
features (interactive permission prompts, Claude/Codex sandboxes) and a good
fit for roles that only read and write text.

```toml
# .maestro/teams/offline-coder.toml
extends = "default-coder"

implementer = "ollama"
reviewer = "ollama"
```

To pair a fast local reviewer with a Claude fallback for failure cases:

```toml
[role_overrides.reviewer]
agent = "ollama"
model_override = "qwen3"
fallback_agent = "claude"
```

`base_url` and `request_timeout_secs` come from `[agents.ollama]`. See
[`docs/teams/`](../teams/README.md) for the full preset schema.
