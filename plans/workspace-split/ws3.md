# Story — Move core modules into maestro-core behind the API

Parent: #1302

The core domain — session pool, orchestration, state, providers, budget, git,
config — is ~110k lines reachable from every screen (`src/session/pool.rs:1`,
2,274 lines; `src/tui/app/mod.rs:1`, 1,693 lines of app shell that today owns
the shared store). Move it behind the #1303 API. After this story,
`maestro-core` compiles with zero references to TUI or CLI code — the
compiler, not convention, holds the boundary.

## Done when

- [ ] core modules live under `crates/maestro-core/src/` with `git log --follow`-able history — owner: implementer
- [ ] zero tui/cli imports inside `maestro-core` — checked by `bash scripts/check-layers.sh` (extended if needed) — owner: implementer
- [ ] full `.wave.json` gate green, including `sync-templates --check` and `docs_gen_no_drift` — owner: implementer
- [ ] only the #1303 API surface is `pub` from `maestro-core` — reviewer spot-checks `cargo doc -p maestro-core --no-deps` — owner: reviewer

## ADR stub

Move files as-is vs extract the interface first. Recommend as-is (`git mv`,
preserve paths under `src/` where sensible): the API from #1303 names the
target, and behavior-preserving moves keep the diff reviewable. Interface
hardening (narrowing `pub`) lands as its own follow-up if this PR gets huge.
Dissent welcome.

## Out of scope

- Refactoring the moved modules — no god-file splits here.
- The TUI or CLI moves — #1306 and #1307.
- Any behavior change; snapshots must not move.

## Handoff

- `git worktree add -b feat/1305-core-move ../maestro-1305 origin/main`
- `codegraph explore "session, orchestration, state, provider, budget"`
- Gate: `.wave.json`; `Cargo.toml` is a serial path; `Closes #1305`.
