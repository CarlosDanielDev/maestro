# Epic — Workspace split: maestro-core / maestro-tui / maestro-cli + CI test sharding

Maestro today is one crate: `Cargo.toml:1` declares a single package and
`src/lib.rs:1` is the only crate root. The TUI alone is 100,652 lines
(`wc -l src/tui`, measured 2026-11-14) — ~47% of `src/` — and any screen can
import anything, because there is no boundary to enforce (`src/tui/mod.rs:1`,
1,639 lines of app shell; `src/cli.rs:1`, 1,728 lines of CLI dispatch sharing
the same internals).

Goal: three workspace crates. `maestro-core` (session pool, orchestration,
state, providers, budget, git) knows nothing about either frontend.
`maestro-tui` and `maestro-cli` depend only on the core public API defined in
#1303. CI test stage sharded across runners (#1308) lands first and
independently — it is the immediate relief and does not wait for the split.

## Fatia

- #1303 — the API seam, written down before any code moves
- #1304 — empty workspace skeleton, green build
- #1305 — core modules behind the API
- #1306 — TUI into its crate
- #1307 — CLI slim to dispatch
- #1308 — nextest + CI sharding, independent, first wave

## Done when

- [ ] every sub-issue closed by a merged PR — owner: implementer

## ADR stub

Two-crate split (tui + cli-with-core) vs three-crate split (core + tui + cli).
Recommend three: the CLI must not drag orchestration internals around, and a
clean core API is the contribution surface. Dissent welcome — record it in the
PR.

## Out of scope

- Screen inventory, cuts, or the god-file refactors (`input_handler.rs`,
  `screen_dispatch.rs`) — that is the TUI Redesign milestone's job. This epic
  moves code as-is, no behavior change.
- Extracting TurboQuant into its own crate — candidate, later decision.
- Publishing crates to crates.io — workspace stays private to this repo.

## Handoff

Parents never receive PRs — this epic closes when its leaves close
(`wave close-parents`).
