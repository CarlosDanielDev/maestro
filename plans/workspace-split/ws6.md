# Story — Adopt cargo-nextest and shard the CI test stage

Parent: #1302

The CI Test job runs the full suite twice on ONE runner —
`.github/workflows/ci.yml:41-56` runs `cargo test --verbose --locked` and
then runs it again only to grep for pending snapshots. The local gate mirrors
this (`.wave.json:3`, `cargo test --verbose` twice). Wall time grows with the
suite; the workspace split will multiply build graphs before it speeds
anything up. Sharding is independent of #1303–#1305 splits and lands first.

## Done when

- [ ] CI test stage is a matrix of 4 `ubuntu-latest` shards running `cargo nextest run --locked --partition hash:<i>/4` — owner: implementer
- [ ] pending-snapshots check preserved as one aggregation step running `cargo test --verbose --locked 2>&1 | (! grep -q "pending snapshots")` — owner: implementer
- [ ] doctests still run — `cargo test --doc` job green (nextest skips doctests) — owner: implementer
- [ ] wall time before/after posted on this issue using `gh run list --workflow ci.yml --json createdAt,updatedAt` timings — owner: implementer
- [ ] `.wave.json` gate uses the nextest command so `wave dispatch` agents run it — owner: implementer
- [ ] plain `cargo test` still green for contributors without nextest — owner: reviewer

## ADR stub

nextest vs sharding `cargo test` by test-name prefix across jobs. Recommend
nextest: per-test process isolation, built-in hash partitioning
(`--partition hash:i/N`), and the Rust ecosystem's default for CI. Name-prefix
sharding needs custom bucketing and breaks the moment a test is renamed.
Dissent welcome.

## Out of scope

- Workspace/crate split — this story is intentionally orthogonal.
- Coverage tiers (`.github/workflows/ci.yml:203`) — leave as-is; revisit
  after the split stabilizes.
- Nextest in the local dev loop — install docs welcome, not required.

## Handoff

- `git worktree add -b feat/1308-nextest-sharding ../maestro-1308 origin/main`
- `codegraph explore "ci workflow, test harness, snapshot_tests"`
- Gate: `.wave.json`; `.github/workflows/ci.yml` changes need CI green on the
  PR itself; `Closes #1308`.
