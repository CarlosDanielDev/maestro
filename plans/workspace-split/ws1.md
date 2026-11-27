# Story — Define the maestro-core public API seam

Parent: #1302

Every split attempt without a written seam lands on a boundary made of
`pub` soup: today any TUI screen imports any internal type, because one crate
has no way to say no (`src/tui/screens/mod.rs:1` — screens return
`ScreenAction::LaunchSession(..)` and friends, driving core directly;
`src/session/pool.rs:1`, 2,274 lines, reachable from anywhere). Before any
file moves, write down what the frontends may consume.

## Done when

- [ ] ADR merged under `docs/adr/` naming the core API surface (session events, costs, config, actions; the `ScreenAction` pattern in `docs/tui-design-system.md:158` is the seed) — owner: implementer
- [ ] API sketch lists what `maestro-tui` and `maestro-cli` each consume; symbols both need are core API — owner: reviewer
- [ ] seam summary added to `directory-tree.md` — owner: implementer

## ADR stub

Seam shape: core emits events + frontends subscribe, vs frontends polling a
shared store. The TUI already reads shared state persisted to
`maestro-state.json`, so polling is the cheap path; events are the honest one
for async work (`docs/tui-design-system.md:206` already mandates visible
feedback for async work). Recommend: state reads stay, mutations go through
the action enum, events added only where polling forces polling loops.

## Out of scope

- Moving any code, creating the workspace, renaming modules.
- Hardening the API with visibility tricks — the split (#1304, #1305)
  enforces it mechanically.

## Handoff

- `git worktree add -b feat/1303-core-api-seam ../maestro-1303 origin/main`
- `codegraph explore "ScreenAction, SessionPool, TuiStore"`
- Gate: `.wave.json` (`cargo fmt`, `cargo clippy -D warnings`, `cargo test`);
  no AI attribution in commits/PRs; `Closes #1303`.
