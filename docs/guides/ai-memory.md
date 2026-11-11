# Persistent memory for agents: the ai-memory shim

> **The short version:** point `[agents.<id>] command` at a two-line shim
> script that execs `ai-memory run --no-autowire --executable <real-harness>
> <harness> "$@"`. Maestro code does not change. The ai-memory server runs
> separately; its install and serving docs are authoritative, never this one.

Every claim below cites the spike it came from:
[`docs/spikes/ai-memory.md`](../spikes/ai-memory.md) (#1084) — scenarios are
numbered there (1a, 2b, 4g, …), and the "isolation incident" section records
the autowire hazard that shaped this guide.

## What this gives you

- Durable per-project workstream memory across sessions, with sanitized
  capture (a planted secret is stored as `[REDACTED:api_key]` everywhere —
  scenario 5c).
- Cross-harness handoff: a codex leg can join a claude workstream and receive
  the portable context packet (scenario 6).
- Lease-protected launches: sibling worktrees run concurrently on separate
  workstreams, and a second launcher on the same workstream waits or fails
  fast instead of corrupting state (scenario 4).

## How the seam works

Maestro spawns whatever your agent's `command` says —
`provider_for_agent` (`src/commands/run.rs`) passes the
`[agents.<id>] command` value verbatim as the spawned binary. A small shim
script is therefore the whole integration; scenario 1 measured the properties
maestro needs and they all hold through the wrapper.

## Prerequisite: run the ai-memory server

Maestro docs do not install ai-memory for you. Follow ai-memory's own
documentation (`github.com/akitaonrails/ai-memory`) for:

- **Install** — the spike used the release asset for `ai-memory` 3.4.5 with a
  verified checksum (`shasum -a 256 -c` → OK) before unpacking (spike
  preamble).
- **Serve** — the spike ran the server in the foreground as
  `ai-memory --data-dir <dir> --config <dir>/config.toml serve --transport
  http`; `ai-memory init` wrote the data layout and the bearer token (spike,
  Sandbox section). Auth is a bearer token and the default bind is a
  localhost port (49374 was the machine default observed in the spike;
  scenario 5a shows `auth: bearer token`).
- **Point the client at it** — `AI_MEMORY_SERVER_URL` selects the server the
  wrapper talks to; the spike pinned it on every call so the sandbox never
  touched the owner's real server (spike, Sandbox section).

One deliberate omission: these docs never tell you to run `ai-memory setup`.
That command installs a host-level service outside maestro's scope (the spike
kept it out of its sandbox for the same reason).

## The shim script

One POSIX-sh script per agent, somewhere on your `PATH`:

```sh
#!/bin/sh
# ~/bin/maestro-ai-memory-claude
exec ai-memory run --no-autowire --executable "$(command -v claude)" claude "$@"
```

```sh
#!/bin/sh
# ~/bin/maestro-ai-memory-codex
exec ai-memory run --no-autowire --executable "$(command -v codex)" codex "$@"
```

Make it executable (`chmod +x`), then wire it in `maestro.toml`:

```toml
[agents.claude]
kind = "claude"
command = "/Users/me/bin/maestro-ai-memory-claude"
```

### Flag contract

- **`--no-autowire` is mandatory.** See the autowire hazard below. The
  `AI_MEMORY_RUN_AUTOWIRE=false` environment variable is the documented
  alternative (scenario 1a stderr); the shim passes the flag instead so the
  guarantee survives any environment.
- **`--executable` resolves the real harness.** With it, the wrapper never
  shows its interactive chooser menu (scenario 3a) — which maestro's
  interactive transport relies on, since it passes native arguments like
  `--session-id <uuid>` (scenario 3g).
- **Never add `--fresh` to the shim.** It cannot be combined with a native
  session selector — the exact error is in the troubleshooting table below
  (scenario 3g).

## Why `--no-autowire` is mandatory (the autowire hazard)

This is the spike's isolation incident, and the reason the shim exists in
this shape:

- **First launch without `--no-autowire` rewrites the harness config home.**
  Autowire writes hooks into `settings.json` (10 hook events, one
  `<ai-memory> hook --event <event> …` command each) and an MCP entry into
  `.claude.json` (`mcpServers.ai-memory`) — scenarios 1a and 2a. Those
  banner lines (`✓ created …settings.json (new file)`) also pollute stdout
  in print mode, which breaks stream-json parsing (scenario 1a vs 1d).
- **Autowire scope follows `CLAUDE_CONFIG_DIR`-style resolution, not
  `HOME`.** The spike's very first wrapped run ran with `HOME` sandboxed but
  `CLAUDE_CONFIG_DIR` inherited — the wrapper followed the harness's own
  config-home resolution and rewired the owner's real `~/.claude/settings.json`
  through a symlinked config dir (spike, isolation incident).
- **`--no-autowire` is byte-clean.** With it, stdout is byte-identical to the
  harness's own (scenario 1d) and the config-home tree is left byte-identical
  (scenario 2b).
- Maestro's shim therefore **always** passes `--no-autowire`: maestro never
  lets a third party edit your harness config on launch.

## What the wrapper preserves (spike evidence)

- stdout is byte-identical to the harness's own stream-json (scenario 1d) —
  maestro's parser sees no difference.
- `--version` passes through untouched, so maestro's health check works
  through the shim (scenario 1b).
- Exit codes are preserved — a harness exiting 3 still exits 3 (scenario 1c).
- The post-exit chooser menu never appears on maestro paths: with
  `--executable` it is suppressed (scenario 3a), and in headless mode maestro
  spawns the child with null stdin so the wrapper's stdin-EOF wait resolves
  immediately (scenarios 3c/3d and the scenario 3 matrix answers).
- All wrapper narration goes to stderr; maestro never parses it
  (scenario 1 matrix answers).

## Troubleshooting

Errors below are verbatim from the spike's captured output.

| What you see | Why | What to do |
| --- | --- | --- |
| `Error: opening managed workstream; the agent was not started` followed by `Caused by: POST /workstream/runs: server returned 409 Conflict: {"error":"workstream is already active: owned by <host>:<pid> until <timestamp>"}` (scenario 4g) | Another live child holds the lease on that workstream past the retry window. A *short* overlap waits and succeeds instead (scenario 4c); the lease releases when the child exits, not when stdin closes (scenarios 4e/4f). | Wait for the holder's agent to finish, or give concurrent agents distinct workstreams (scenario 4a). |
| A loud stderr warning at launch, then the session proceeds and memory hooks spool locally (`ai-memory status` shows `spool: pending: N`, scenario 5a) | The ai-memory server is unreachable. The wrapper degrades to a local spool instead of failing the launch (spike, "Matrix → seam options" (a); the exact warning text is ai-memory's, per its own docs, along with the stricter `--require-server` flag — the spike did not capture it verbatim). | Start the server (`serve`, see Prerequisite) and check `AI_MEMORY_SERVER_URL`. |
| `Error: --fresh cannot be combined with a native session, resume, continue, or fork selector` (scenario 3g) | `--fresh` was added to the shim while maestro passes native-session arguments (e.g. interactive `--session-id`). | Remove `--fresh` from the shim script. |
| ``Error: linked claude session <uuid> was launched under <old-config-home>/projects but this launch resolves <new-config-home>/projects; relaunch with the same CLAUDE_CONFIG_DIR (or the same `ai-memory run --profile`) to resume it, or pass --fresh to start a new session`` (spike, isolation incident) | `CLAUDE_CONFIG_DIR` changed between the original launch and this one, so the linked native session cannot be found. | Relaunch with the same `CLAUDE_CONFIG_DIR` the session was created under, or pass `--fresh` once to start a new session. |

## Scope

- The spike exercised the `claude` and `codex` harnesses through the wrapper
  (scenarios 1–6). Qwen has no ai-memory harness — leave its `command` alone
  (epic #1110).
- Codex can join a claude workstream for handoff (`ai-memory run codex` on
  the same workstream, scenario 6).

## Related

- [`docs/spikes/ai-memory.md`](../spikes/ai-memory.md) — the evidence matrix
  behind every claim in this guide (#1084).
- [Agent providers](../agents/mod.md) — `[agents.<id>]` configuration.
- [Configuration reference](../configuration.md) — full agents schema.
- Epic #1110 — maestro adopts ai-memory through documented shims.
