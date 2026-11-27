# Story — Create the cargo workspace skeleton with three crates

Parent: #1302

`Cargo.toml:1` is a single package today; there is no workspace. This story
creates the three empty members — `crates/maestro-core`, `crates/maestro-tui`,
`crates/maestro-cli` — wires them into the root binary, and keeps every gate
green. Nothing moves yet; the point is a green skeleton the next stories fill.

## Done when

- [ ] `cargo metadata --no-deps` lists `maestro-core`, `maestro-tui`, `maestro-cli` as workspace members — owner: implementer
- [ ] `cargo build`, `cargo test`, `cargo clippy -- -D warnings -A dead_code` green with zero behavior change — owner: implementer
- [ ] `directory-tree.md` updated with the `crates/` layout — owner: implementer
- [ ] `bash scripts/check-layers.sh` still green after the skeleton lands — owner: reviewer

## ADR stub

Three crates vs two (CLI folded into core). Recommend three: `maestro-cli`
must stay runnable headless without linking the TUI, and a thin CLI crate is
the cheapest place to keep it thin. Two crates would let CLI code quietly
depend on core internals forever. Dissent welcome.

## Out of scope

- Moving any module out of `src/` — #1305, #1306, #1307.
- Changing the binary's behavior, flags, or output.
- Touching `Cargo.lock` policy (`--locked` stays in CI).

## Handoff

- `git worktree add -b feat/1304-workspace-skeleton ../maestro-1304 origin/main`
- `codegraph explore "Cargo.toml, src/lib.rs, src/main.rs"`
- Gate: `.wave.json`; `Cargo.toml` is a serial path — one PR at a time
  touches workspace manifests; `Closes #1304`.
