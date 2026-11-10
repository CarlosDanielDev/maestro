# Spike #1084 — maestro agent sessions under `ai-memory run`

Date: 2026-11-10. Binary: `ai-memory` 3.4.5, release asset
`ai-memory-macos-aarch64.tar.gz` from
`github.com/akitaonrails/ai-memory/releases` tag `v3.4.5`, checksum verified
(`shasum -a 256 -c` → OK) before unpacking. Host: macOS (Darwin 27.2.0, arm64).

Every scenario ran against the real binary inside an isolated sandbox. All
output below is RAW pasted from the captured `.out`/`.err` files, with one
declared transformation: ANSI color codes stripped, text lines verbatim.

## Sandbox

```sh
# env.sh, sourced by every command below
export AIMEM=/tmp/aimem-bin-1084/ai-memory
export SBX=/tmp/aimem-sbx-1084
export HOME=$SBX/home
export XDG_CONFIG_HOME=$SBX/xdg-config
export XDG_DATA_HOME=$SBX/xdg-data
export XDG_CACHE_HOME=$SBX/xdg-cache
export AI_MEMORY_DATA_DIR=$SBX/data
export AI_MEMORY_SERVER_URL=http://127.0.0.1:49731
export PATH=$SBX/bin:/tmp/aimem-bin-1084:/usr/bin:/bin:/usr/sbin:/sbin
export CLAUDE_CONFIG_DIR=$SBX/home/.claude   # added AFTER the incident below
```

- Server: `ai-memory --data-dir $SBX/data --config $SBX/data/config.toml serve
  --transport http`, foreground process, log to file. `ai-memory init` wrote
  the data layout and the bearer token. `ai-memory setup` was NOT run (out of
  scope: it installs a launchd service a temp HOME does not isolate).
- Port: `127.0.0.1:49731` (config `bind`). The machine default port 49374 was
  already occupied by the owner's real ai-memory server; the sandbox never
  talked to it (`AI_MEMORY_SERVER_URL` pinned on every call).
- Stubs: `tests/fixtures/e2e_stub_agent.sh` copied as `claude-stub`; variants
  generated from it: `claude-stub-exit3` (same output, `exit 3`),
  `claude-stub-secret` (reply text carries a fake key), `claude-stub-slow`
  (`sleep 20` before output), `codex-stub` (copy of
  `tests/fixtures/mock-codex-cli.sh`), and `claude` (copy of `claude-stub`,
  used only in scenario 3e so the wrapper resolves a harness named `claude`
  from PATH without `--executable`).
- Repo: `/tmp/aimem-sbx-1084/repo` (branch `main`) with sibling worktrees
  `repo-wt-a` (branch `wt-a`) and `repo-wt-b` (branch `wt-b`).
- No provider cost: every harness is a stub; the server runs with LLM and
  embedding providers disabled (`ai-memory status` below shows `llm:
  disabled`, `embedding: disabled`).

## Isolation incident (found and fixed; shaped the ADR)

The very first wrapped run was made WITHOUT `CLAUDE_CONFIG_DIR` pinned. The
autowire does not follow `HOME` — it follows the harness's own config-home
resolution, which honors the inherited `CLAUDE_CONFIG_DIR`, so it rewrote the
owner's real `~/.claude/settings.json` (via the `~/.claude-glm` symlink)
pointing every ai-memory hook at the sandbox binary, data dir, and server
port. The wrapper wrote a timestamped backup next to the file; the backup was
diffed (only the hook commands + one added hook group differed), restored
byte-identical, and re-verified (`diff` clean; `grep` for sandbox paths in
`~/.claude/settings.json` and `~/.claude.json` finds nothing). All later runs
pin `CLAUDE_CONFIG_DIR` into the sandbox.

Consequence recorded for the ADR: `--no-autowire` is not optional for maestro
use, and any wrapper user must know that autowire scope follows
`CLAUDE_CONFIG_DIR` (and friends), not `HOME`.

With `CLAUDE_CONFIG_DIR` later switched inside the sandbox, the workstream
that had linked a session under the real store refused to resume — recorded
verbatim under scenario 1 because it is real wrapper behavior:

```text
Error: linked claude session ef8fecf4-bfc9-4294-bb18-c9be5cce0b10 was launched under /Users/carlos/.claude-glm/projects but this launch resolves /private/tmp/aimem-sbx-1084/home/.claude/projects; relaunch with the same CLAUDE_CONFIG_DIR (or the same `ai-memory run --profile`) to resume it, or pass --fresh to start a new session
```

---

### Scenario 1 — headless stream-json passthrough

Baseline (stub run directly), captured once:

```sh
$ "$SBX/bin/claude-stub" --print --verbose --output-format stream-json --model x
```

```text
{"type":"system","subtype":"init","session_id":"e2e00000-0000-0000-0000-000000000001"}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"stub reply"}]}}
{"type":"result","subtype":"success","result":"stub reply","session_id":"e2e00000-0000-0000-0000-000000000001"}
```

1a — the issue's argv, wrapper's first launch for this harness (autowire on,
fresh sandbox home):

```sh
$ "$AIMEM" run --executable "$SBX/bin/claude-stub" claude --print --verbose \
    --output-format stream-json --model x
# exit: 0
```

stdout (diff against baseline, `diff baseline.out wrapped.out`):

```text
0a1,3
> ✓ created /tmp/aimem-sbx-1084/home/.claude/.claude.json (new file)
> capture mode: denylist
> ✓ created /tmp/aimem-sbx-1084/home/.claude/settings.json (new file)
```

stderr:

```text
INFO ai_memory_cli: ai-memory starting version="3.4.5" server_url=http://127.0.0.1:49731/ data_dir=/private/tmp/aimem-sbx-1084/data bind=127.0.0.1:49731
ai-memory: starting a fresh claude session in workstream 'default'
ai-memory: first managed launch of claude here — wiring its ai-memory hooks + MCP so capture and recall work (disable with --no-autowire or AI_MEMORY_RUN_AUTOWIRE=false).
[ai-memory] capture-policy capability v1 enforced by this selected integration; re-run --apply to refresh existing installs.
✓ staged 18 hook script(s) → /private/tmp/aimem-sbx-1084/data/hooks/claude-code
ai-memory: could not prepare the previous claude-code session history (the linked session is unavailable in this client's native stores: no checkout-local native session matched); continuing with the portable workstream packet
ai-memory: workstream 'default' saved 2 new event(s)
```

1d — same argv with `--no-autowire`:

```sh
$ "$AIMEM" run --fresh --no-autowire --executable "$SBX/bin/claude-stub" claude \
    --print --verbose --output-format stream-json --model x
# exit: 0
```

stdout: `diff baseline.out noautowire.out` printed nothing →

```text
NOAUTOWIRE-STDOUT-IDENTICAL
```

stderr:

```text
INFO ai_memory_cli: ai-memory starting version="3.4.5" server_url=http://127.0.0.1:49731/ data_dir=/private/tmp/aimem-sbx-1084/data bind=127.0.0.1:49731
ai-memory: starting a fresh claude session in workstream 'default'
ai-memory: could not prepare the previous claude-code session history (the linked session is unavailable in this client's native stores: no checkout-local native session matched); continuing with the portable workstream packet
ai-memory: this harness did not acknowledge its managed context packet; refresh its ai-memory hooks before the next run
ai-memory: workstream 'default' saved 2 new event(s)
```

1b — `--version` passthrough (what maestro's health check runs through a shim):

```sh
$ "$AIMEM" run --no-autowire --executable "$SBX/bin/claude-stub" claude --version
# exit: 0
```

stdout:

```text
stub-agent 1.0.0
```

1c — stub that exits 3:

```sh
$ "$AIMEM" run --fresh --no-autowire --executable "$SBX/bin/claude-stub-exit3" claude \
    --print --verbose --output-format stream-json --model x
# wrapped exit: 3   (stub run directly also exits 3)
```

Matrix answers (scenario 1):

- stdout is byte-identical to the stub's own stream-json ONLY with
  `--no-autowire`. With autowire on (first launch), three banner lines are
  printed on stdout — two `✓ created …` lines and `capture mode: denylist` —
  which are non-JSON lines in print mode.
- `--version` passes through: the stub's version line is the only stdout, exit 0.
- A stub exiting 3 keeps exit code 3 through the wrapper.
- All wrapper narration (version log line, workstream notices, packet
  warnings) goes to stderr; nothing but the banner lines above ever touches
  stdout in print mode.

---

### Scenario 2 — autowire footprint

Method: empty reference dir + fresh sandbox `HOME` (and fresh
`CLAUDE_CONFIG_DIR` inside it), autowire sentinel cleared, one wrapped run,
then `diff -r empty-ref home`. The sentinel lives in the DATA dir:
`$SBX/data/autowire-state/claude-code-3.4.5-<hash>` (two hashes observed for
the same binary+version — per install location). Deleting the sentinel files
re-triggers the one-time wiring; otherwise autowire silently skips.

2a — WITH autowire, `diff -r "$SBX/empty-ref" "$SBX/home"`:

```text
Only in /tmp/aimem-sbx-1084/home: .claude
```

Files created (complete list, `find "$SBX/home" -type f`):

```text
/tmp/aimem-sbx-1084/home/.claude/.claude.json
/tmp/aimem-sbx-1084/home/.claude/settings.json
```

`settings.json` content, summarized from the parsed JSON (file contains ONLY
the `hooks` key): 10 hook events — `SessionStart`, `UserPromptSubmit`,
`PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `PreCompact`, `Stop`,
`SessionEnd`, `SubagentStart`, `SubagentStop` — each with one command of the
shape:

```text
<ai-memory binary> --data-dir <data dir> hook --event <event> --agent claude-code --server-url <server url>
```

`.claude.json` content (parsed):

```json
{"mcpServers": {"ai-memory": {"type": "stdio", "command": "<ai-memory binary>",
 "args": ["mcp-bridge", "--server-url", "http://127.0.0.1:49731/mcp",
          "--data-dir", "/private/tmp/aimem-sbx-1084/data"]}}}
```

The 18 hook scripts themselves are staged under the DATA dir
(`data/hooks/claude-code/`), not under the harness config home.

2b — WITH `--no-autowire`, fresh empty home, same run shape:

```text
$ diff -r "$SBX/empty-ref" "$SBX/home"
# no output; diff exit 0  → byte-identical
```

Matrix answers (scenario 2): autowire touches exactly two files in the
harness config home — `settings.json` (hooks only; existing keys preserved —
on the host it preserved unrelated groups while rewriting only ai-memory's
own entries) and `.claude.json` (`mcpServers.ai-memory`). `--no-autowire`
leaves the tree byte-identical.

---

### Scenario 3 — PTY behavior

Runs under `script -q /dev/null …` (all three stdio are the PTY slave).

3a — PTY + `--executable` + headless argv:

```text
# exit: 0, wall time 4s; menu grep: none
```

3c — PTY, no `--executable`, headless argv, stdin held open 40s
(`sleep 40 | script …`): NO menu printed, wrapper exited only when stdin hit
EOF — wall time 40s, exit 0. Control: same command with stdin `/dev/null`
exits in 3s. So in `--print` mode the wrapper relays stdin and finalizes at
EOF; the wait is not a menu.

3d — same as 3c plus `--no-exit-prompt`: identical (40s, no menu) — confirms
the wait is stdin relay, independent of the exit prompt.

3e — PTY, no `--executable`, NO native args (`ai-memory run --fresh --new
s3-bare claude`), stdin held 40s — the post-exit chooser menu APPEARED and
BLOCKED the session end until EOF:

```text
✓ ai-memory: claude exited successfully in workstream 's3-bare'.
  A switch keeps this workstream, its options and the same jail.
  r) run claude again
  q) quit (default)
Select [q]:
```

(exit 0; wall time 40s = the stdin hold; EOF chose the default `q) quit`.)

3g — PTY + `--executable` + maestro interactive-style argv
(`claude --session-id <uuid>`): no menu, exit 0, 3s. (First attempt with
`--fresh` failed by design, recorded verbatim: `Error: --fresh cannot be
combined with a native session, resume, continue, or fork selector`.)

Matrix answers (scenario 3):

- The chooser menu is shown only for a bare/native-style invocation (no
  headless argv) without `--executable` when stdio are TTYs; it blocks the
  session from ending until input or EOF; EOF = `q) quit`, exit 0.
- With `--executable` the menu never appears (matches the documented rule).
- With headless argv there is never a menu, but the wrapper waits for stdin
  EOF before exiting. maestro's headless transport spawns the child with
  `Stdio::null()` stdin (`src/agent_provider/claude/headless.rs:189`), so
  under the wrapper maestro gets immediate EOF and no hang (observed 3-4s
  total).

---

### Scenario 4 — worktrees and leases

4a — two sibling worktrees, concurrent, named workstreams (`repo-wt-a`
`--new s4-alpha` ∥ `repo-wt-b` `--new s4-beta`, launched in the same second):

```text
alpha-exit: 0
beta-exit: 0
--- alpha stderr tail:
ai-memory: starting a fresh claude session in workstream 's4-alpha'
ai-memory: workstream 's4-alpha' saved 2 new event(s)
--- beta stderr tail:
ai-memory: starting a fresh claude session in workstream 's4-beta'
ai-memory: workstream 's4-beta' saved 2 new event(s)
```

4c — same worktree, same EXISTING workstream, two concurrent launches: the
loser waits and then succeeds (both exit 0):

```text
ai-memory: another launcher owns this workstream; waiting briefly in case it is finalizing
```

(The non-interactive retry window outlasts a ~3s child, so a quick overlap
does not fail.)

4g — lease fail-fast: holder's child kept alive 20s (`claude-stub-slow`);
challenger launched +2s later against the same workstream:

```text
challenger-exit: 1
--- challenger stderr:
INFO ai_memory_cli: ai-memory starting version="3.4.5" server_url=http://127.0.0.1:49731/ data_dir=/private/tmp/aimem-sbx-1084/data bind=127.0.0.1:49731
ai-memory: another launcher owns this workstream; waiting briefly in case it is finalizing
Error: opening managed workstream; the agent was not started

Caused by:
    POST /workstream/runs: server returned 409 Conflict: {"error":"workstream is already active: owned by macpro:84886 until 2026-11-10T21:00:05.666651Z"}
holder-exit: 0
```

Also observed: the wrapper's post-exit stdin-EOF wait does NOT hold the lease
— the lease is released when the child exits (scenarios 4e/4f: a holder
waiting on stdin did not block a challenger that started seconds later).

Matrix answers (scenario 4): sibling worktrees run managed sessions
concurrently without conflict when they use separate workstreams (or the
per-checkout default — worktree fingerprints differ). Same workstream, same
worktree: short overlap → the second launcher waits then succeeds; a holder
that keeps its child alive past the retry window → the challenger fails fast
with exit 1 and the 409 text above (owner host:pid + lease expiry).

---

### Scenario 5 — data and privacy

5a — `ai-memory status` (server + data layout):

```text
ai-memory 3.4.5 (server)
ai-memory 3.4.5 (client)
  server:       http://127.0.0.1:49731
  data-dir:     /private/tmp/aimem-sbx-1084/data
  db:           /private/tmp/aimem-sbx-1084/data/db/memory.sqlite
  bind:         127.0.0.1:49731
  auth:         bearer token
  pages:        0 (all versions: 0)
  sessions:     2
  observations: 4
  fts:          pages 0/0; observations 4/4
  embeddings:   0 rows; 0 latest pages missing
  storage:      1.1 MiB on disk, 0 B reclaimable (0.0%)
  links:        0 latest-page links (unresolved: 0, stale: 0)
  wiki format:  OKF v0.2 (migrated)
  spool:
    pending:    6
  providers:
    llm:       disabled
    embedding: disabled
```

Data directory path on this OS (macOS): with `AI_MEMORY_DATA_DIR` set it is
honored exactly; the DEFAULT on macOS was observed from the owner's real
install (the host hook commands restored in the incident section point at
`/Users/carlos/Library/Application Support/ai-memory`) — i.e.
`~/Library/Application Support/ai-memory`, NOT the `~/.local/share` path the
Linux-oriented docs suggest.

5b — files one managed session creates (fresh workstream, snapshot diff of
the whole data dir before/after): exactly one new file,

```text
/tmp/aimem-sbx-1084/data/raw/workstreams/<workstream-uuid>/segments/<run-uuid>-<hash>.jsonl
```

plus rows in the server's SQLite ledger (`workstream_events` and friends).
Backfill additionally writes wiki pages (below).

5c — secret scrubbing, OBSERVED. A synthetic native transcript in the Claude
store layout
(`$CLAUDE_CONFIG_DIR/projects/-private-tmp-aimem-sbx-1084-repo-wt-a/5e5e5e5e-….jsonl`)
was planted with the fake key `sk-test-AAAAAAAAAAAAAAAA` in both a user
prompt and an assistant reply, then imported through the documented import
path (`ai-memory backfill`; natural managed runs do not read a stub's stdout
and the stub writes no native store of its own). Backfill output:

```text
📼 ai-memory imported 1 prior local session(s) (~2 events) for default/repo-wt-a. Opt out with AI_MEMORY_BACKFILL_ON_START=false.
```

Result, observed not assumed:

- `grep -rl "sk-test-AAAAAAAAAAAAAAAA" "$SBX/data"` → no matches (exit 1),
  covering `raw/`, `wiki/`, `db/`, logs excluded below.
- The imported content IS in the wiki, scrubbed. Session page
  (`wiki/<workspace>/<project>/sessions/5e5e5e5e-….md`), raw excerpt:

```markdown
# deploy now, the api key is [REDACTED:api_key] keep it secret
...
1. deploy now, the api key is [REDACTED:api_key] keep it secret
- `other [ai-memory-backfill:assistant-message]` @ 2026-11-10T20:59:05Z — Deploying. Using key [REDACTED:api_key] from your message.
```

- The daily log page renders the same `[REDACTED:api_key]` placeholder.
- The sanitizer therefore runs BEFORE any persistence (wiki, raw segments,
  FTS), as the docs claim.

Matrix answers (scenario 5): data dir on macOS defaults to
`~/Library/Application Support/ai-memory`; one managed session adds one raw
segment file plus ledger rows; a planted secret does NOT reach `raw/` or
`wiki/` unscrubbed — it is replaced by `[REDACTED:api_key]` everywhere.

---

### Scenario 6 — handoff

6a/6b — claude leg, then codex leg on the SAME workstream:

```sh
$ "$AIMEM" run --fresh --no-autowire --new s6-handoff --executable "$SBX/bin/claude-stub" claude --print --verbose --output-format stream-json --model x
# exit: 0
$ "$AIMEM" run --no-autowire --workstream s6-handoff --executable "$SBX/bin/codex-stub" codex
# exit: 0
```

codex leg stderr (RAW):

```text
ai-memory: could not prepare the previous claude-code session history (the linked session is unavailable in this client's native stores: no checkout-local native session matched); continuing with the portable workstream packet
ai-memory: this harness did not acknowledge its managed context packet; refresh its ai-memory hooks before the next run
ai-memory: workstream 's6-handoff' saved 2 new event(s)
```

The packet delivery path is the harness's SessionStart hook; a stub never
fires hooks, so "did not acknowledge" is the expected stub artifact — the
wrapper prepared and (attempted) delivery of the claude leg's portable
packet to the codex child. The workstream ledger records both legs:

```text
=== ai-memory workstream-search --workstream-id 01a1c743-9e3c-77d1-b7c5-5041854deac6 ===
## Event 4 | codex | import note
Provenance: {"loss_count":1}
Transcript extraction losses:
- native session id could not be discovered; transcript was not imported

## Event 3 | codex | repository checkpoint
Provenance: {"exit_code":0}
HEAD: 9120505ed633d2ac3e3d1ba922a9198a7de6a67f
Branch: wt-a

## Event 2 | claude-code | import note
Provenance: {"loss_count":1}
Transcript extraction losses:
- native transcript import failed: native transcript for c5d7072b-1020-4358-a0dc-a88fd8dcd7fd was not found

## Event 1 | claude-code | repository checkpoint
Provenance: {"exit_code":0}
```

6e — `ai-memory continue` from an unrelated cwd (`cd /tmp` first):

```text
# exit: 0
continuing default/repo-wt-a in /private/tmp/aimem-sbx-1084/repo-wt-a
ai-memory: linked claude session c5d7072b-1020-4358-a0dc-a88fd8dcd7fd is missing from its native store; starting fresh and repointing workstream 's6-handoff' after the new session is established
ai-memory: this harness did not acknowledge its managed context packet; refresh its ai-memory hooks before the next run
ai-memory: workstream 's6-handoff' saved 2 new event(s)
```

stdout (the stub's stream-json, relayed untouched):

```text
{"type":"system","subtype":"init","session_id":"e2e00000-0000-0000-0000-000000000001"}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"stub reply"}]}}
{"type":"result","subtype":"success","result":"stub reply","session_id":"e2e00000-0000-0000-0000-000000000001"}
```

Matrix answers (scenario 6): `ai-memory run codex` joins the claude workstream
and receives the portable packet (delivery acknowledged only by real hooks);
`ai-memory continue` resolves the managed checkout from anywhere and relaunches
the harness into the same workstream. True transcript handoff needs a real
harness store — with stubs the ledger honestly records the import loss.

---

## Matrix → seam options (evidence for the ADR)

- (a) documented per-agent shim script as `[agents.<id>] command`: scenario 1
  shows the exact properties maestro needs — byte-identical stdout with
  `--no-autowire`, `--version` passthrough for the health check, exit codes
  preserved, no menu on any maestro path (headless stdin is null; interactive
  passes `--executable`), stderr-only narration maestro never parses. Zero
  maestro code. Hazards to document: autowire MUST stay off (scenario 1a
  banners + scenario 2 host-config writes + the incident), and the wrapper
  needs a reachable server (degrades to a loud stderr warning + local spool
  when down, per its docs and the `--require-server` flag).
- (b) native `[agents.<id>] memory = "ai-memory"`: same wrapper behavior, but
  maestro would own argv construction, flag drift across ai-memory releases,
  and the server reachability policy in Rust — real code for exactly what the
  shim already does. Nothing in the matrix requires it.
- (c) hooks/MCP only: no wrapper means no lease, no cross-harness workstream,
  no `continue`/handoff — the features the epic wants (scenarios 4 and 6 all
  run through the wrapper). Capture-only, strictly less than (a).
- (d) skip: nothing in the matrix broke; skipping is not evidence-supported.

Full ADR text: PR body of the spike PR for #1084.
