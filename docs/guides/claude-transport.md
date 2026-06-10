# Claude transport: headless vs interactive

> **The short version:** if you run maestro against a **Claude Pro/Max
> subscription**, set `transport = "interactive"` on your claude agent in
> `maestro.toml` **before 2026-06-15**. If you pay with **API credits**,
> do nothing — `headless` stays the right choice.

## What changes on 2026-06-15

Anthropic withdraws subscription billing from headless `claude --print`
invocations on **2026-06-15**. After the cutoff, headless calls made by a
Pro/Max subscription account are billed against a much smaller quota
(roughly a 40x cut). Interactive Claude Code sessions keep the full
subscription quota.

> Note: the cutoff date and quota figures come from Anthropic's
> announcement to subscribers. Check
> [Anthropic's pricing docs](https://docs.claude.com/en/docs/claude-code/costs)
> for the current terms — numbers here reflect what was announced at the
> time this guide was written.

Maestro historically drove `claude` exclusively in headless mode
(`--print --output-format stream-json`). Without action, every maestro
session on a subscription account hits the reduced quota after the cutoff.

## The two transports in plain English

- **`headless`** (default) — maestro runs `claude --print` as a one-shot
  subprocess per task and reads machine-readable output from stdout. This
  is today's behaviour, unchanged.
- **`interactive`** — maestro starts the real Claude Code REPL on a hidden
  pseudo-terminal (PTY), types into it like a human would, and reads
  structured output from the session transcript file Claude Code already
  writes (`~/.claude/projects/<project>/<session-id>.jsonl`). Because the
  session is interactive, subscription billing applies.

## Which one should I pick?

```
Are you paying with API credits (ANTHROPIC_API_KEY / console billing)?
└─ yes → transport = "headless"   (default; nothing to do)
└─ no, I'm on a Claude Pro/Max subscription
   └─ transport = "interactive"   (set it before 2026-06-15)
```

## How to flip the flag

In `maestro.toml`, on your claude agent entry:

```toml
[agents.claude]
kind = "claude"
command = "claude"
model = "opus"
transport = "interactive"   # "headless" (default) | "interactive"
```

Or in the TUI: **Settings → Agents → your claude agent → Transport**
(the row only shows for claude-kind agents).

From **2026-05-15**, maestro warns at startup when an enabled claude agent
still runs headless. Silence it with `MAESTRO_SILENCE_TRANSPORT_WARN=1` if
headless is intentional (e.g. API-credit billing).

## What interactive mode does under the hood

- One PTY child per conversation, **kept alive between turns** — follow-up
  turns in an interaction session write into the same REPL instead of
  spawning a new process.
- The child's environment is scrubbed of every `ANTHROPIC_*` variable
  (`ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, anything with the prefix)
  so it cannot silently fall back to API-key billing. Removed variable
  *names* are logged; values never are.
- Events (assistant text, tool use, token/cost updates, turn completion)
  come from tailing the session transcript JSONL — the same data the
  headless stream provides, so the TUI looks identical either way.

## Known limitations of interactive mode

- **One-shot text commands stay headless.** Internal `--print` text calls
  (e.g. quick title generation) are not interactive sessions; they keep
  using the headless path regardless of the flag. They are small, but
  post-cutoff they bill accordingly.
- **Slower first turn.** The REPL has to boot (auth handshake, model
  load) before the first prompt; later turns reuse the warm child and are
  comparable to headless latency.
- **Parked children hold resources.** Each live interaction conversation
  keeps one `claude` process alive until the conversation (or maestro)
  ends.
- **Cancellation is an interrupt, not a kill.** Cancelling a turn sends
  Ctrl-C to the REPL (twice) and keeps the conversation alive; the child
  is killed only if it stops responding.

## Related

- `docs/configuration.md` — the `transport` row in the agents table
- `docs/spikes/2026-05-claude-interactive-transport.md` — the feasibility
  spike behind this design (#747)
- Issues #748–#752 (milestone v0.30.5 — Subscription Transport)
