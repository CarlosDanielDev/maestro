Parent: #{{memepic.n}}

Question: can maestro's agent sessions run under ai-memory without breaking, and which seam is the cheapest safe one? What maestro does today, with evidence: the agent command comes from `[agents.<id>] command` (`src/config/agents.rs:48`) and is resolved in `provider_for_agent` (`src/commands/run.rs:256-270`, default `claude`); the headless transport builds `<command> --print --verbose --output-format stream-json --model ...` (`src/agent_provider/claude/headless.rs:28`), spawns it (`:177-193`), and preflights `<command> --version` (`headless.rs` health check); the interactive transport runs the binary in a PTY and re-attaches with `claude --resume <id>` (`src/agent_provider/claude/interactive.rs:260`). What ai-memory documents (README and `docs/managed-workstreams.md`): wrapper options must come BEFORE the harness name and everything after it is native argv; the wrapper is the foreground process and the agent is its child in the same process group; the first launch of a harness auto-installs hooks and an MCP entry into that harness's config unless `--no-autowire` / `AI_MEMORY_RUN_AUTOWIRE=false`; the post-exit chooser menu appears only when stdin, stdout and stderr are all TTYs and `--executable` was not used; non-interactive launches fail fast on a lease conflict; the child's exit code is preserved. The docs do NOT say which files autowire touches, whether scope is cwd or git root, or what the wrapper prints on stdout in print mode — the spike measures those.

## Done when

- [ ] a scenario matrix is checked in at `docs/spikes/ai-memory.md` with the RAW output of every scenario below, each run against the real `ai-memory` binary inside an isolated sandbox (temp `HOME`, temp `XDG_*`, temp data dir, stub agents from `tests/fixtures/e2e_stub_agent.sh`, no provider cost) — `grep -c "^### Scenario" docs/spikes/ai-memory.md` prints 6 — owner: implementer
- [ ] scenario 1, headless stream-json: maestro-style argv through the wrapper (`ai-memory run --executable <stub> claude --print --verbose --output-format stream-json --model x`) — the matrix states whether stdout is byte-identical to the stub's own stream-json (no banner lines), whether `--version` passes through, and the exit code for a stub that exits 3 — owner: implementer
- [ ] scenario 2, autowire footprint: a before/after `diff -r` of the sandbox home shows exactly which files autowire created or modified for `claude`, and that `--no-autowire` leaves the tree byte-identical (`diff -r` prints nothing) — owner: implementer
- [ ] scenario 3, PTY: under a pseudo-terminal (`script -q /dev/null ...` or the repo's PTY backend) the post-exit chooser menu is shown or not, with and without `--executable`, and whether it blocks the session from ending — owner: implementer
- [ ] scenario 4, worktrees and leases: two sibling worktrees of one repo each run a managed session at the same time (separate workstreams, no conflict) and the same worktree twice (the fail-fast error text and exit code are recorded) — owner: implementer
- [ ] scenario 5, data and privacy: the data directory path on this OS, the files one session creates, and whether a fake secret planted in the stub transcript (`sk-test-AAAAAAAAAAAAAAAA`) reaches `raw/` or `wiki/` unscrubbed — the result is recorded as observed, not assumed — owner: implementer
- [ ] scenario 6, handoff: after a stub `claude` session under `ai-memory run`, `ai-memory continue` or `ai-memory run codex` with a stub codex receives the handoff, with the commands and output pasted — owner: implementer
- [ ] the ADR in the PR picks one seam with the owner: (a) a documented per-agent shim script used as `command` (zero maestro code), (b) a native `[agents.<id>] memory = "ai-memory"` setting where maestro builds `ai-memory run --no-autowire ... <harness>` itself, (c) hooks/MCP only (maestro never wraps; the user installs ai-memory's own plugin into the harness), or (d) skip — and states what the matrix showed for each — owner: reviewer
- [ ] the follow-up slices for the chosen option (only that option) exist as a plan directory whose `wave plan plans/<dir> --dry-run` prints the issues and exits 0 — owner: implementer

## ADR stub

Recommendation (to argue with): start with (a). It costs no maestro code, and it keeps autowire — which edits the user's harness config — out of maestro's hands. Move to (b) only if the matrix shows the shim breaks something real (stdout noise in stream-json, the `--version` preflight, or the PTY resume path). (c) avoids wrapping altogether but gives up the lease and handoff features. Qwen is unsupported by ai-memory, so Qwen agents stay out of v1.

## Out of scope

- The owner's real home and real harness config: every scenario runs in the sandbox, never against `~/.claude` or the real `~`.
- Running `ai-memory setup`: it installs a per-user launchd/systemd service that a temp `HOME` does NOT isolate. Start the server in the foreground against the temp data dir using the command from ai-memory's `docs/install.md`, or stop and report.
- Any maestro integration code, any default-on behavior, Qwen support, and the policy of what agents should remember.

## Handoff

```bash
git worktree add -b spike/{{memepic.1.n}}-ai-memory ../maestro-{{memepic.1.n}} origin/main
```

Needs the owner's OK before downloading and running the third-party binary (same rule as the herdr spike #1069): download the release asset from https://github.com/akitaonrails/ai-memory/releases to a temp directory, verify its checksum if one is published, never pipe an installer into a shell. `codegraph explore "provider_for_agent build_stream_args"` and read `src/commands/run.rs`, `src/agent_provider/claude/headless.rs`, `src/agent_provider/claude/interactive.rs`. Gate: `cargo fmt --check && cargo clippy && cargo test && bash scripts/check-file-size.sh` (the deliverable is `docs/spikes/ai-memory.md` plus a plan directory, so the Rust gate must stay green). Skills: project-patterns. No AI attribution; none of the forbidden git commands. `Closes {{memepic.1.n}}`.
