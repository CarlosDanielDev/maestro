Parent: #1057

Four CI jobs fail on every PR — including config-only #1055 — and `wave merge` requires CI green, so no wave can merge until they pass. Last fully green CI on main: 2026-08-16. Symptoms, each to be confirmed and fixed at implementation:

1. **Cargo Deny** — `cargo deny check` exits 2: stale skip at `deny.toml:143` (wit-bindgen version moved) plus duplicate-crate errors (`base64` 0.22/0.23, `miniz_oxide` 0.8/0.9, `rand_core` — `Cargo.lock` entries), observed 2026-10-15 in `/Users/carlos/projects/maestro-1052`.
2. **Clippy** — `cargo clippy -- -D warnings` errors at `src/tui/screens/settings/schema_tab/widgets/dynamic_map/mod.rs:183` (lint from a newer stable toolchain; zero findings in files changed by open PRs). Note the repo gate runs `cargo clippy -- -D warnings -A dead_code` locally — the CI job must pass with the flags `.github/workflows/ci.yml` actually runs.
3. **File Size Lint** — `scripts/check-file-size.sh` exit 1: 90 entries in `scripts/allowlist-large-files.txt` carry deadlines in the past (deadline column format at `allowlist-large-files.txt:2-4`); zero size violations. The real split work is #1014.
4. **Architecture Layers** — `scripts/check-layers.sh` exit 1: 5 past-deadline entries in the debt arrays of `scripts/architecture-layers.yml` (`sanitize/screen.rs`, `commands/*`). The real decoupling work is #1015.

## Por que não é story

No product behavior changes; the gain is a mergeable gate — every current and future PR is blocked by these four reds.

## Done when

- [ ] `cargo deny check` exits 0 in a clean checkout of the PR branch (skip updated and/or duplicates resolved — no `--force`), shown with exit code in the PR — owner: implementer
- [ ] `cargo clippy -- -D warnings` (CI flags, no `-A`) exits 0 in the PR checkout — the `dynamic_map/mod.rs:183` finding fixed, not suppressed — owner: implementer
- [ ] `bash scripts/check-file-size.sh` exits 0 and `bash scripts/check-layers.sh` exits 0 in the PR checkout — deadlines re-dated per the owner's decision below, each bumped entry keeping its `ticket:` reference — owner: implementer
- [ ] the PR's own CI run shows all four jobs green (File Size Lint, Architecture Layers, Cargo Deny, Clippy) — owner: implementer

## ADR stub

For items 3-4 the fork is policy: (a) re-date the entries to 2026-12-31 with tickets #1014/#1015 kept (unblocks merge now; the real work keeps its issues); (b) do the 90 file splits and 5 layer decouplings now (days of work inside a gate-repair PR — rejected); (c) stop failing on past deadlines (weakened gate — rejected). Recommendation (to argue with): (a). For item 2, fix the code — suppressing a fresh lint with an allow attribute only moves the failure to the next toolchain bump.

## Out of scope

- The 90 large-file splits (#1014) and the 5 layer decouplings (#1015) — only their deadlines move here.
- Nightly-only jobs (mutation, miri, release) — not PR gates.

## Handoff

```bash
git worktree add -b chore/1057-ci-repair ../maestro-1057 origin/main
```

`codegraph explore "dynamic_map"` and read `deny.toml`, `scripts/check-file-size.sh`, `scripts/check-layers.sh`, `scripts/architecture-layers.yml`. Gate: the full `.wave.json` gate. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes 1057`.
