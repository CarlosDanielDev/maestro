# CI Smoke Check — Manual Procedure

Run before tagging any release that modifies CI infrastructure
(`.github/workflows/*.yml`, `scripts/check-*.sh`, `scripts/*-tiers.yml`,
`scripts/architecture-layers.yml`, `.cargo/mutants.toml`, `deny.toml`,
`clippy.toml`, `.claude/hooks/preflight.sh`).

Expected time: ~15 minutes.

## Scenario 1 — Cognitive complexity gate

- [ ] Create a scratch branch.
- [ ] Add a function to any file in `src/` with cognitive complexity > 20 (e.g., nested matches, 10+ branches).
- [ ] Push as a PR.
- [ ] Verify `Clippy` CI job fails with `cognitive_complexity` warning.

## Scenario 2 — Curated nursery lint

- [ ] On a scratch branch, introduce a redundant `.clone()` on a value that's about to be moved.
- [ ] Push. Verify `Clippy` fails with `clippy::redundant_clone`.

## Scenario 3 — cargo-deny strict mode

- [ ] Add a dependency that pulls in a duplicate of an already-present crate NOT in the `skip` list.
- [ ] Push. Verify `Cargo Deny` job fails with `multiple-versions`.

## Scenario 4 — File-size allowlist deadline past

- [ ] Edit `scripts/allowlist-large-files.txt`. Change one entry's deadline to `2000-01-01`.
- [ ] Push. Verify `File Size Lint` job fails with `DEADLINE PAST`.

## Scenario 5 — File 400+ LOC not on allowlist

- [ ] Create `src/smoke_test.rs` with 450 lines.
- [ ] Push. Verify `File Size Lint` job fails with `VIOLATION`.

## Scenario 6 — Coverage floor (after activation)

- [ ] (Once core tier is activated via `--enforce` flag) Remove a test file that was covering a core module.
- [ ] Push. Verify `Coverage Tiers` job fails — drops below floor.

## Scenario 7 — Layer violation

- [ ] Add `use crate::tui::theme::SerializableColor;` to `src/session/manager.rs` (domain importing UI).
- [ ] Push. Verify `Architecture Layers` job fails with `FORBIDDEN`.

## Scenario 8 — Layer-debt deadline past

- [ ] Edit `docs/layers-debt.txt`. Change one entry's deadline to `2000-01-01`.
- [ ] Push. Verify `Architecture Layers` job fails with `DEADLINE PAST`.

## Scenario 9 — Nightly freshness (after Wave 3 activation)

- [ ] Verify current `nightly-freshness` status on any open PR is green.
- [ ] If nightly intentionally failed overnight, verify `nightly-freshness` on open PRs goes red within the 3-day window.

## Scenario 10 — Preflight hook

- [ ] Run `bash .claude/hooks/preflight.sh` locally on a clean tree.
- [ ] Verify all three gates run and exit 0.
- [ ] Deliberately introduce a fmt violation; re-run; verify the hook fails on fmt.

## Regression log

If any scenario fails unexpectedly, file a GitHub issue tagged `bug` + `area:ci` before releasing.
