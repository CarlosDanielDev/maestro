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
