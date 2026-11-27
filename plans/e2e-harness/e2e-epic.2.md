Parent: #1047

The binary has no test harness: `grep -rn CARGO_BIN_EXE tests/ src/` prints nothing, and `src/integration_tests/mod.rs:161` forbids process spawning, so the closest coverage is in-process (`src/agent_provider/claude/interactive_session_tests.rs:209` spawns the PTY backend, not maestro). The full offline scenario was rehearsed by hand on d1ec627: temp dir + `maestro.toml` with `[agents.stub] command = <stub>` (`src/config/agents.rs:48`), `maestro run --prompt "hello" --once --skip-doctor` against a stub emitting init/assistant/result stream-json (`src/agent_provider/claude/headless.rs:96` spawns it) — exit 0, `"status": "completed"` + `agent_session_id` from the stub in `maestro-state.json` (`src/state/store.rs:34`, cwd-relative), `ASSISTANT: stub reply` + `COMPLETED` in `.maestro/logs/<id>.log` (`src/session/logger.rs:20`). The `--prompt` path touches no network (`src/commands/run.rs:170`); `--skip-doctor` bypasses the `gh`/`--version` preflight (`src/commands/run.rs:87`). Blocked by #1048: today the same command crashes without a TTY, and the harness must not need a pty.

## Por que não é story

No product behavior changes; the gain is a regression net over the only path no automated test covers — real binary, real state files, real logs, offline and deterministic.

## Done when

- [ ] `cargo test --test e2e_run_once` spawns the built binary (`env!("CARGO_BIN_EXE_maestro")`) in a temp dir whose `maestro.toml` points `[agents.stub]` `command` at a stub script; asserts exit code 0, `maestro-state.json` session `"status": "completed"`, `agent_session_id` equal to the stub's session id, and `.maestro/logs/<session-id>.log` containing an `ASSISTANT:` line — owner: implementer
- [ ] the stub is one script under `tests/fixtures/` (modeled on `tests/fixtures/mock-codex-cli.sh`): `--version` exits 0 printing a version, any other invocation prints the three stream-json lines (system init, assistant, result success) — owner: implementer
- [ ] the test runs offline (no `gh`, no network — `--prompt` + `--skip-doctor` path) and the new test file adds less than 30s to `cargo test` — owner: implementer
- [ ] the file respects the 400-line cap (`scripts/check-file-size.sh` passes) — owner: implementer

## ADR stub

Where: (a) a cargo integration test in `tests/` (process spawning is fine there, zero new infra); (b) relax the `src/integration_tests/` charter (no — the no-spawn rule is what keeps that suite fast and in-process); (c) a separate harness crate (overkill for one scenario). Recommendation (to argue with): (a), single `tests/e2e_run_once.rs` until a second scenario exists, then a `tests/e2e/` dir. Stub style: hardcoded minimal script (this issue) versus env-driven knobs like `mock-codex-cli.sh` (add the knobs when a scenario needs a failing agent or a multi-turn reply). Sandbox: cwd-relative state (`maestro-state.json`, `.maestro/`) is fully isolated by the temp dir; the XDG cache (`src/templates/rendered_cache.rs:69`) is shared and read-mostly — accepted for now.

## Out of scope

- Interaction-session scenario (PTY transport) — #1050.
- Golden-set / eval runs — #931, #932, #933.
- A `MAESTRO_HOME` env redirect for the XDG caches.
- Mutation-testing scope (`.cargo/mutants.toml` already excludes test files).

## Handoff

```bash
git worktree add -b chore/1049-e2e-harness ../maestro-1049 origin/main
```

`codegraph explore "cmd_run StateStore default_path"` and read `src/commands/run.rs` `--prompt` path plus `src/session/logger.rs`. TDD: the test is the deliverable — write it, watch it fail on the pre-#1048 TTY crash, watch it pass after. Gate: `cargo fmt --check && cargo clippy && cargo test && bash scripts/check-file-size.sh`. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes 1049`.
