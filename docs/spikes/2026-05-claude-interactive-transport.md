# Spike: Claude interactive PTY transport feasibility (issue #747)

**Date:** 2026-06-10 (executed; spike filed 2026-05)
**Driver:** maestro orchestrator (overnight batch run)
**Context:** On 2026-06-15 Anthropic withdraws subscription billing from headless
`claude --print` invocations (~40x quota cut). Maestro spawns Claude exclusively
in `--print --output-format stream-json` mode today. This spike answers whether
interactive PTY mode is a viable alternative transport so Claude Pro/Max
subscription users stay inside their quota.

**Environment:** `claude` 2.1.170 (Claude Code), macOS (Darwin 25.6.0),
tmux 3.4, portable-pty 0.9.0.

## Verdict: GREEN

Drive `claude` interactive in a PTY via **portable-pty**, identify the session
with a pre-generated `--session-id <uuid>`, and consume machine-readable output
by **tailing the session transcript JSONL** at
`~/.claude/projects/<munged-cwd>/<session-id>.jsonl`. Round-trip confirmed end
to end twice (including once with `ANTHROPIC_*` scrubbed from the child while
fake keys were present in the parent).

---

## Question 1 — REPL slash command for machine-readable output

**Verdict: rejected.**

`claude --help` (v2.1.170) states `--output-format` "only works with --print".
There is no `/output-format` slash command in the interactive REPL and no
documented REPL equivalent. The interactive REPL renders for humans only; the
machine-readable surface in interactive mode is the transcript file (Q2).

## Question 2 — Structured transcript file

**Verdict: confirmed.**

There is no `--output-dir` flag and `/export` is not needed. Claude Code
persists every interactive session **by default** (see `--no-session-persistence`,
which is itself `--print`-only) to:

```
~/.claude/projects/<munged-cwd>/<session-id>.jsonl
```

- `<munged-cwd>` = absolute cwd with `/` and `.` replaced by `-`
  (e.g. `/Users/carlos/projects/maestro` → `-Users-carlos-projects-maestro`).
- `<session-id>` is controllable: `claude --session-id <uuid>` (must be a valid
  UUID). This makes the transcript path **deterministic before spawn** — no
  directory watching or newest-file heuristics needed.
- Format: line-delimited JSON. Entry types observed in the prototype session:
  `mode`, `permission-mode`, `file-history-snapshot`, `user`, `attachment`,
  `ai-title`, `assistant`, `system`, `last-prompt` (plus `pr-link` in older
  sessions).
- `assistant` entries embed the **full API message** under `message`:
  `content` blocks (`thinking` / `text` / `tool_use`), `model`, `stop_reason`,
  and complete `usage` (input/output/cache tokens). `user` entries carry the
  prompt and `tool_result` blocks. This is a superset of what the headless
  stream-json path provides — token accounting and event mapping both survive.
- Writes are line-buffered appends; polling the file at 500ms intervals
  observed the assistant entry within one poll of the turn completing.

## Question 3 — portable-pty prototype

**Verdict: confirmed.**

Prototype: `docs/spikes/prototypes/portable-pty-prototype.rs` (~150 LOC,
portable-pty **0.9.0** — the issue text guessed 0.8; 0.9.0 is latest stable on
impl day). Flow:

1. `openpty(132x40)`, `CommandBuilder::new("claude")` with
   `--safe-mode --session-id <uuid> --model haiku`, cwd set, `ANTHROPIC_*`
   removed via `env_remove`.
2. Reader thread drains PTY output continuously (the child stalls if the
   master side is not drained — this is mandatory, not optional).
3. Wait for REPL boot (10s flat sleep in the prototype; the real backend
   should poll for the transcript file's `mode` entry instead).
4. Write turn text + `\r` (carriage return submits in the REPL).
5. Poll transcript JSONL for an `assistant` entry containing the sentinel.
6. Write `/exit` + `\r`; child exits cleanly (`ExitStatus 0`) in <3s.

Observed run:

```
session-id: 6364d660-467a-4e24-b566-49f4879e72d6
turn written, polling transcript...
assistant sentinel in transcript: true
child exited: ExitStatus { code: 0, signal: None }
pty bytes drained: 9656
VERDICT: GREEN
```

Notes for #749:
- `--safe-mode` keeps hooks/plugins/MCP out of the child — faster boot,
  fewer surprises. Decide in #749 whether maestro wants user customizations
  active inside interaction sessions; if yes, drop the flag and accept boot cost.
- ANSI screen-scraping was **not needed** — the PTY output is drained and
  discarded; all structure comes from the transcript. `strip-ansi-escapes`
  unnecessary under this approach.
- Submit is `\r`. `\n` alone inserts a newline in the composer.

## Question 4 — tmux fallback

**Verdict: confirmed (works), not recommended as primary.**

`tmux new-session -d -x 132 -y 40 "claude --session-id <uuid>"` +
`send-keys <text> Enter` + the same transcript polling also went GREEN.

Comparison:

| | portable-pty | tmux |
|---|---|---|
| extra runtime dep | none (crate) | external binary |
| child lifecycle | owned (`try_wait`/`kill`) | indirect (`kill-session`); exit status lost |
| collision risk | none | session-name namespace |
| Windows | supported by crate | effectively no |
| ergonomics | in-process reader/writer | shell-out per keystroke |

Recommendation: portable-pty primary; keep the `claude-tmux = []` cargo feature
as an empty stub (per #749 AC) so a tmux backend can be added if portable-pty
misbehaves on some platform.

## Question 5 — Env scrubbing forces subscription billing

**Verdict: confirmed.**

Control state: the host shell has **no** `ANTHROPIC_*` variables and
`~/.claude.json` carries an `oauthAccount` (Pro/Max subscription login).

Test: prototype run with `ANTHROPIC_API_KEY=sk-ant-fake-spike-key` and
`ANTHROPIC_AUTH_TOKEN=fake-token` exported in the parent. The prototype's
`env_remove` scrub reported both vars removed; the child completed the turn
successfully via the OAuth login:

```
scrubbed vars: ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"]
assistant sentinel in transcript: true
VERDICT: GREEN
```

Had the fake key leaked into the child, the API request would have failed with
an auth error (the key is invalid) — the successful turn proves the child
authenticated via OAuth, i.e. subscription billing. Corroborating CLI doc: the
`--bare` flag help explicitly describes the normal (non-bare) auth chain as
OAuth/keychain, with `ANTHROPIC_API_KEY` as the override — removing the
override leaves OAuth.

Scrub list for #749: every var with prefix `ANTHROPIC_` (prefix match, not a
fixed list — `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_API_KEY`, future additions all
caught). Log removed names only, never values.

---

## Recommended implementation approach for #749

1. **Driver:** `portable-pty = "0.9"`; `InteractiveBackend` trait with
   `PortablePty` impl + `MockBackend` for tests; `claude-tmux` feature stub.
2. **Session identity:** generate UUID up front, pass `--session-id`, compute
   transcript path before spawn (munge rule above).
3. **Event source:** tail the transcript JSONL (`transcript_parser.rs` maps
   `user`/`assistant`/`system` entries → `StreamEvent`s; ignore
   `mode`/`ai-title`/`file-history-snapshot`/`last-prompt`/`attachment`).
4. **Readiness:** poll for transcript file existence (its first `mode` line is
   written at boot) instead of a flat sleep; 5s spawn timeout per AC, longer
   readiness budget for first-turn.
5. **PTY reader:** `spawn_blocking` drain loop, output discarded (or retained
   in a small ring buffer for diagnostics); transcript events through bounded
   `mpsc::channel(64)`.
6. **Cancel:** write `\x03` twice with 100ms gap, then `kill()` after 1s
   (prototype confirmed clean `/exit`; Ctrl-C path to be exercised in #749
   tests via MockBackend).
7. **Hybrid:** keep `run_text` and one-shot `stream-json` turns on headless;
   only long-lived interaction turns ride the PTY (per #750's note).

## Cost note

Spike consumed 3 interactive haiku turns (one-line sentinel replies) on the
subscription account: 2 portable-pty runs + 1 tmux run.
