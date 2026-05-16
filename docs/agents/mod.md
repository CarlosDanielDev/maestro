# Agent Providers

Maestro can run sessions through multiple agent providers. Claude remains the
default when `maestro.toml` has no `[agents]` table, so existing projects do not
need to migrate unless they want to use another runtime.

Use `maestro run --agent <id>` to select a configured agent for one run, or set
`[agents].default` to change the project default.

```toml
[agents]
default = "claude"

[agents.claude]
kind = "claude"
enabled = true
command = "claude"
model = "opus"
```

## Provider Types

Maestro treats all providers through the same session lifecycle, but the
transport differs:

- **Subprocess providers** launch a local CLI and parse its streaming output.
  Claude, Codex, Qwen, and OpenCode are subprocess providers. They require a
  `command` and must not set `base_url`.
- **HTTP providers** call an OpenAI-compatible HTTP API directly. Ollama and
  MiniMax are HTTP providers. They require `base_url` and must not set
  `command`.

HTTP providers do not run the Claude CLI and therefore cannot rely on its
built-in template delivery mechanism. When a session originates from a direct
user request (`SessionOrigin::DirectUser`) and carries an `active_command`,
`SessionPool::try_promote` consults a `RenderedTemplateStore` to inject the
pre-rendered template body into the session prompt. This injection happens after
the knowledge-base push and before TurboQuant compaction. Subprocess providers
(Claude, Codex, Qwen, OpenCode) are not affected; they handle template delivery
through their own CLI flags.

## Comparison

| Agent | Transport | Cost | Requires | Notes |
| --- | --- | --- | --- | --- |
| Claude | Subprocess | Paid | Claude CLI | Default |
| Codex | Subprocess | Paid | Codex CLI | OpenAI |
| Qwen | Subprocess | Paid | Qwen CLI | Alibaba |
| OpenCode | Subprocess | Varies | opencode CLI + auth | 75+ AI backends |
| Ollama | HTTP | Free/local | `ollama serve` + model | Runs locally |
| MiniMax | HTTP | Paid/cloud | `MINIMAX_API_KEY` | 204k context window |

## Guides

- [Claude](claude.md)
- [Codex](codex.md)
- [Qwen](qwen.md)
- [OpenCode](opencode.md)
- [Ollama](ollama.md)
- [MiniMax](minimax.md)
- [Configuration Reference](../configuration.md)

## Migration

If `[agents]` is absent, Maestro builds an implicit Claude agent from
`[sessions]`:

```toml
[sessions]
default_model = "opus"
permission_mode = "bypassPermissions"
allowed_tools = []
```

That implicit agent behaves like:

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

Add an explicit `[agents]` table only when you want multiple providers, a
non-Claude default, or provider-specific settings.

## FAQ

**Can I mix providers in one project?**

Yes. Keep the project default stable, then use `maestro run --agent <id>` for
specific runs. `maestro doctor` checks every enabled agent; the default agent is
required and other enabled agents are optional.

**Why does an enabled non-default agent show a doctor warning?**

Non-default enabled agents are optional so they do not block the whole project.
Disable agents you are not actively using with `enabled = false`.

**Which OpenCode model should I pick?**

Use OpenCode's `provider/model` format. `openrouter/deepseek/deepseek-chat` is
a common low-cost cloud option, while `groq/llama-3.1-8b-instant` is useful for
fast, cheap tasks. Availability and pricing depend on the provider account
configured in OpenCode.

**Why is Ollama slow?**

Local speed depends on model size, RAM, CPU, and GPU acceleration. Start with a
small model, confirm `ollama serve` is running, and increase
`request_timeout_secs` for large models.

**Can Ollama run on another machine?**

Yes. Set `base_url` to the remote Ollama server, for example
`http://devbox.local:11434`. Keep the network private or protect it with your
own access controls.

**How do I fix MiniMax 401 errors?**

Confirm the environment variable named by `api_key_env` is set in the shell that
launches Maestro. The default is `MINIMAX_API_KEY`.

**Which MiniMax model should I choose?**

`MiniMax-M2.7` is the default for long-context coding work. Use high-speed
variants only if your MiniMax account and endpoint support them.

**What happens on rate limits?**

Maestro surfaces the provider error. Reduce concurrency, choose a cheaper or
local provider for background work, or wait for the provider quota window to
reset.
