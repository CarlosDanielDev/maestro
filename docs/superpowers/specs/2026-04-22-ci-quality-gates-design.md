---
title: CI Quality Gates — Design
date: 2026-04-22
status: Approved (design phase)
author: Carlos Daniel
---

# CI Quality Gates — Design

## Summary

Add six quality-enforcing gates to the Maestro CI pipeline, rolled out in
three waves ordered by infrastructure cost and run cadence. Each gate has
its own rollout strategy tailored to its cost class (per-gate strategy,
not uniform rollout). Per-PR gates block merges immediately; nightly gates
enforce via a branch-protection "recent-green within 3 days" rule backed
by a small custom freshness bot.

The project extends (does not replace) the existing six-job CI workflow at
`.github/workflows/ci.yml` and the supporting config at `clippy.toml`,
`deny.toml`, `rustfmt.toml`, and `scripts/check-file-size.sh`. It also
populates the intentionally-empty `.claude/hooks/preflight.sh` left behind
by the `/implement` harness spec, rehearsing the fast gates locally
before branch creation.

## Context and Problem

### Current CI baseline (already in place)

From `.github/workflows/ci.yml`:

- `test` — `cargo test --verbose` + insta snapshot enforcement.
- `clippy` — `cargo clippy -- -D warnings -A dead_code`.
- `fmt` — `cargo fmt --check`.
- `file-size` — `scripts/check-file-size.sh` with 500-LOC cap and a
  23-entry allowlist at `scripts/allowlist-large-files.txt`. Several
  entries are stale (e.g., `src/github/*` — the code was moved to
  `src/provider/github/*` and the allowlist wasn't updated). Phase 1b
  triage includes a stale-path audit.
- `deny` — `cargo deny check advisories bans licenses sources`.
- `audit` — `rustsec/audit-check@v2.0.0` with three documented ignores.

From `clippy.toml`:

```toml
too-many-arguments-threshold = 7
type-complexity-threshold = 250
cognitive-complexity-threshold = 25
too-many-lines-threshold = 120
```

From `deny.toml`: `multiple-versions = "warn"`, `wildcards = "deny"`,
license allowlist, unknown-registry/git denied.

From `docs/RUST-GUARDRAILS.md`: 8-principle policy,
`#![forbid(unsafe_code)]` at crate roots, `anyhow` at boundaries, async
hygiene rules, testing discipline with insta snapshots and real fakes.

### The gaps vs. original requirements

The initial brainstorm laid out seven asks. Gap analysis:

| # | Ask | Current state | Gap |
|---|-----|--------------|-----|
| 1 | Cognitive complexity ≤ 20 | `clippy.toml` at 25 | Tightening + CLI flag |
| 2 | Coverage 96% | None | New infra + test writing |
| 3 | Mutation testing 80% | None | New nightly infra |
| 4 | File size 400 LOC no allowlist | 500 LOC + 27-file allowlist | Cap tightening + allowlist restructuring + major refactoring |
| 5 | Dep structure + layer violations | Crate-level via cargo-deny | Module-level layer enforcement (custom) |
| 6 | Race + leak detection | None | miri + ThreadSanitizer infra |
| 7 | N+1 queries | N/A — no DB | Dropped |

### Locked design decisions

Surfaced during brainstorm:

1. **Per-gate rollout strategy.** Each gate gets its own cadence and
   migration mechanism. Uniform rollout is wrong because gaps range
   from 1-hour config changes to multi-month refactoring.

2. **All six gates in one spec.** Interactions between gates matter
   (coverage informs mutation; file-size splits inform layer rules).
   Single spec document; phased implementation plan.

3. **File size target: 400 LOC with deadlined allowlist.**
   `scripts/allowlist-large-files.txt` format changes to include
   `deadline`, `owner`, `ticket`, `plan` fields. CI fails when a
   deadline is in the past.

4. **Coverage: tiered floors, not flat 96%.** Core modules 90% floor
   (96% aspiration). TUI 70% floor. Binary wiring excluded. Plus a
   ratchet preventing decrease across PRs.

5. **Nightly gates enforce via freshness bot.** Branch protection
   requires mutation + miri to have passed within the last 3 days.
   Tsan is weekly + informational only.

## Non-Goals

Explicit to avoid scope creep:

- **N+1 query detection.** No DB layer.
- **Performance regression gates** (bench ratchet). Deferred to a
  separate spec. Criterion benchmarks exist but wiring them in blocking
  needs its own variance/hardware-normalization design.
- **Doc coverage** (`clippy::missing_docs_in_private_items`). Too noisy
  for a binary crate.
- **Full `clippy::pedantic` group.** Rejected — curated nursery subset
  instead.
- **Fuzz testing.** No fuzz targets exist; separate design project.
- **Backport gates to release branches.** Gates enforce on `main` only.
- **Author-based enforcement differences.** Every commit gets the same
  treatment regardless of authorship.

## Architecture Overview

Three-wave rollout. Each wave is independently shippable and has its own
review profile. Wave 1 ships in under a week; Wave 2 in 3-4 weeks; Wave 3
in 6-8 weeks.

```
Per-PR jobs (block merge immediately):
  ┌─ test, clippy, fmt (existing) ─────────────────────┐
  ├─ file-size (existing, tightening in Wave 2) ────────┤
  ├─ deny, audit (existing, tightening in Wave 1) ──────┤
  ├─ clippy-nursery (new Wave 1) ───────────────────────┤
  ├─ coverage (new Wave 2) ─────────────────────────────┤
  └─ layers (new Wave 2) ───────────────────────────────┘

Nightly scheduled jobs (block main via recent-green ≤ 3 days):
  ┌─ mutation (new Wave 3) ─────────────────────────────┐
  └─ miri (new Wave 3) ─────────────────────────────────┘

Weekly informational (pinned issue, no block):
  └─ tsan (new Wave 3) ─────────────────────────────────┘
```

**Why three waves:**

- Each wave is independently shippable. Wave 1 PRs can merge while Wave
  2 is still being designed; no blocking dependency chain.
- Each wave has a distinct review profile. Wave 1 is config review.
  Wave 2 is "does the infrastructure work". Wave 3 is "is the nightly
  infrastructure stable enough to block merges".
- Nightly rigor is gated behind successful Wave 2. Running mutation
  testing without coverage first is diagnosing diseases with no health
  checkup.

## Wave 1 — Quick wins (per-PR blocking)

Four config tightenings landing as one PR with four commits. Expected
timeline: 3-5 days of wall time including fixing newly-surfaced warnings.

### Gate 1.1 — Cognitive complexity 25 → 20

Change in `clippy.toml`:

```toml
cognitive-complexity-threshold = 20   # was 25
```

CI already runs `cargo clippy -- -D warnings`, so the cognitive-complexity
lint fails the build on any function exceeding the threshold. Lowering
surfaces 5-15 expected new violations.

**Rollout:** land config change + fix every surfaced violation in the
same PR. No "land config, fix later" — that means a broken main.

### Gate 1.2 — Curated clippy nursery subset

Seven lints chosen for high signal on this codebase:

```rust
#![warn(
    clippy::missing_const_for_fn,
    clippy::needless_pass_by_ref_mut,
    clippy::redundant_clone,
    clippy::significant_drop_tightening,
    clippy::fallible_impl_from,
    clippy::path_buf_push_overwrite,
    clippy::branches_sharing_code,
)]
```

Added to both `src/lib.rs` and `src/main.rs` (crate roots). `-D warnings`
promotes to errors in CI.

**Rationale per lint:**

- `missing_const_for_fn`: free performance wins; catches under-specified
  APIs.
- `needless_pass_by_ref_mut`: API hygiene.
- `redundant_clone`: correctness-adjacent; unnecessary clones usually
  indicate confusion about ownership.
- `significant_drop_tightening`: matches RUST-GUARDRAILS async policy
  ("no await-in-Mutex").
- `fallible_impl_from`: matches "errors are values; panics are bugs"
  policy (§2). **Highest-signal lint for this codebase.**
- `path_buf_push_overwrite`: real bug catcher — `push("/abs")` silently
  replaces the entire path.
- `branches_sharing_code`: good refactoring signal, low false-positive
  rate.

**Explicitly not included** (evaluated, rejected): `missing_docs_in_private_items`
(noisy), `use_self` (style), `option_if_let_else` (variable signal),
`suboptimal_flops` (codebase has almost no floats).

Expected: 10-30 violations on first enable. Fix in the same PR.

### Gate 1.3 — cargo-deny strict mode

Change in `deny.toml`:

```toml
[bans]
multiple-versions = "deny"   # was "warn"
skip = [
    # Document each skip with reason + upstream link.
    # { name = "syn", version = "1", reason = "ratatui still on syn 1" },
]
```

Before flipping, run `cargo deny check bans` locally, enumerate every
`multiple-versions` warning, and add a documented `skip` entry for each
that can't be resolved. Goal: empty `skip`. Reality: 5-10 skip entries
for transitive ratatui/syntect/reqwest duplicates.

Each skip entry includes: crate name, pinned old version, reason,
upstream ticket or RUSTSEC link where relevant.

### Gate 1.4 — File-size allowlist format change

Current `scripts/allowlist-large-files.txt` is bare paths with freeform
comments. New format is deadlined:

```
# Format: <path> # deadline: YYYY-MM-DD, owner: @handle, ticket: #N, plan: <brief>
src/tui/screens/settings/mod.rs # deadline: 2026-08-01, owner: @carlos, ticket: #TBD, plan: extract theme_editor, keybind_editor, general_prefs
```

`scripts/check-file-size.sh` grows a deadline check. **Critical: the
existing loader strips comments before populating `allowed[]`** (via
`line="${line%%#*}"`), so the deadline metadata lives only in the raw
line. The deadline check iterates a *parallel array* that preserves the
full line:

```bash
# Load allowlist — preserve both stripped paths and raw lines.
allowed=()
allowed_raw=()
if [[ -f "$ALLOWLIST" ]]; then
  while IFS= read -r line; do
    raw="$line"
    stripped="${line%%#*}"
    stripped="${stripped// /}"
    [[ -z "$stripped" ]] && continue
    allowed+=("$stripped")
    allowed_raw+=("$raw")
  done < "$ALLOWLIST"
fi

# Deadline enforcement — uses the raw array.
today=$(date +%Y-%m-%d)
for entry in "${allowed_raw[@]+"${allowed_raw[@]}"}"; do
  deadline=$(echo "$entry" | grep -oE 'deadline: [0-9-]+' | sed 's/deadline: //')
  if [[ -n "$deadline" && "$deadline" < "$today" ]]; then
    echo "DEADLINE PAST: $entry"
    violations=$((violations + 1))
  fi
done
```

Rationale for the parallel array: keeps the existing stripped-path
glob-match semantics for `is_allowed()` while adding deadline parsing
without regressing the old logic.

**Still honors 500 LOC cap** in Wave 1. The cap flip to 400 happens in
Wave 2.2.

**Two-phase allowlist triage:**

- **Phase 1a (part of Wave 1 PR):** migrate every existing entry to a
  placeholder deadline 14 days after Wave 1 merge. Default owner
  `@carlos`, `plan: TBD`.
- **Phase 1b (follow-up PR within 14 days):** triage every entry. Real
  deadlines. Real split plans. Reassign owners. **Remove stale entries**
  (paths that no longer exist — e.g., `src/github/*` after the move to
  `src/provider/github/*`). If Phase 1b slips past day 14, main goes
  red — the forcing function.

### Populate `.claude/hooks/preflight.sh`

The `/implement` harness spec left this file intentionally empty. Wave 1
ships it as:

```bash
#!/usr/bin/env bash
set -e
cargo fmt -- --check
cargo clippy -- -D warnings -A dead_code
bash scripts/check-file-size.sh
```

Fast local gates (< 10s total) so `/implement` catches regressions before
branching, not after pushing.

### Wave 1 exit criteria

- All four config changes merged.
- Zero clippy warnings under new thresholds.
- Zero `cargo deny` warnings under new settings.
- Phase 1b merged: allowlist cleaned of stale paths; every remaining
  entry has a real deadline + owner + split plan.
- `.claude/hooks/preflight.sh` populated.
- `docs/RUST-GUARDRAILS.md` appended (no fixed numbering — new sections
  go at the end of the current last section, currently §15).

## Wave 2 — Per-PR rollout (per-PR blocking)

Three gates requiring new infrastructure and new test-writing. Expected
timeline: 3-4 weeks wall time, three sub-PRs.

### Gate 2.1 — Coverage via `cargo-llvm-cov` with tiered floors

**Tool:** `cargo-llvm-cov` (not `tarpaulin`; llvm-cov is more accurate on
Rust 2024 edition and integrates with standard llvm coverage tooling).

**Tier manifest:** `scripts/coverage-tiers.yml` (YAML chosen over TOML
for consistency with GitHub Actions workflows):

```yaml
tiers:
  core:
    floor: 90.0
    aspiration: 96.0
    paths:
      - "src/session/**"
      - "src/state/**"
      - "src/adapt/**"
      - "src/turboquant/**"
      - "src/gates/**"
      - "src/provider/**"
      - "src/config.rs"
      - "src/cli.rs"

  tui:
    floor: 70.0
    paths:
      - "src/tui/**"

  excluded:
    paths:
      - "src/main.rs"
      - "src/lib.rs"
      - "src/integration_tests/**"
      - "**/tests.rs"
      - "**/*_test.rs"
```

**CI job** (new in `.github/workflows/ci.yml`):

```yaml
coverage:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - uses: Swatinem/rust-cache@v2
    - run: cargo install cargo-llvm-cov --locked
    - run: cargo llvm-cov --workspace --lcov --output-path coverage.lcov
    - run: bash scripts/check-coverage-tiers.sh coverage.lcov
```

**`scripts/check-coverage-tiers.sh`** (new, ~80 lines of bash + `yq`):

1. Parse `coverage.lcov` and group files by the tier manifest globs.
2. For each tier, compute weighted mean line coverage (by LOC).
3. Assert `mean ≥ floor`; print per-file breakdown on failure.
4. Assert `total coverage did not decrease vs main` — the ratchet.

**Rollout for existing code:** measure current coverage baseline on
main. If core < 90%, don't enable the core-tier floor until the
baseline is at or above 90%. Until then, the "does not decrease" ratchet
is the active gate. Expected baseline: core ≈ 65-80%, TUI ≈ 30-50%.
Floor gates activate 2-4 weeks after ratchet, as tests are written.

### Gate 2.2 — File size 500 → 400 transition

**Prerequisites:** Wave 1 Phase 1b done (real deadlines on all current
violators).

**The cap flip:** `scripts/check-file-size.sh` `MAX_LINES=500` → `400`.
Every file in the 400-500 band that's NOT already allowlisted is added
with a triaged deadline (same process as Phase 1b, new scope). Expected
scope: 20-40 additional files. Total allowlist after Wave 2.2: ~50-70
entries.

**RUST-GUARDRAILS §1 update:**

```markdown
**File size.** Hard cap 400 LOC. Soft target 300 LOC — anything
approaching 400 is a review signal. Allowlist is temporary — every
entry has a deadline. Extending a deadline requires a paragraph in the
PR explaining why the refactor wasn't done and a new realistic deadline.
```

### Gate 2.3 — Layer violations via custom script + manifest

**Manifest:** `scripts/architecture-layers.yml` (YAML for consistency):

```yaml
# Layers are listed lowest to highest. A module may import from the
# same layer or any lower layer. Imports from higher layers are forbidden.

layers:
  - name: primitives
    number: 1
    paths:
      - "src/icons.rs"
      - "src/icon_mode.rs"
      - "src/util/**"

  - name: domain
    number: 2
    paths:
      - "src/session/**"
      - "src/state/**"
      - "src/adapt/**"
      - "src/turboquant/**"
      - "src/provider/**"
      - "src/gates/**"
      - "src/config.rs"

  - name: orchestration
    number: 3
    paths:
      - "src/cli.rs"
      - "src/commands/**"

  - name: ui
    number: 4
    paths:
      - "src/tui/**"

  - name: entry
    number: 5
    paths:
      - "src/main.rs"
      - "src/lib.rs"

forbidden:
  - from: "src/session/**"
    to: "src/tui/**"
  - from: "src/state/**"
    to: "src/tui/**"
  - from: "src/provider/**"
    to: "src/tui/**"
  - from: "src/config.rs"
    to: "src/tui/**"
```

**Enforcement:** `scripts/check-layers.sh` (new, ~100 lines):

1. Parse the manifest into a layer map and forbidden-pairs list.
2. For every `.rs` file in `src/`, extract `use crate::...` statements
   via regex (simple — doesn't need a full Rust parser).
3. Look up importing file's layer and imported module's layer.
4. Fail if importer layer < imported module layer. Fail on any
   forbidden-pair match.
5. Print violations as `<file>:<line>: <file> (layer N) imports <mod> (layer M)`.

**Baseline:** run on main first, list existing violations, add to
`docs/layers-debt.txt` with deadlines. Same deadlined-allowlist pattern
as file-size violations. Each debt entry gets refactored to restore
compliance.

**Known debt at design time** (will land in `docs/layers-debt.txt` as
part of Wave 2.3):

- `src/config.rs` imports `crate::tui::theme::{ThemeConfig, ThemePreset, SerializableColor}`
  in 4 locations (lines 2, 1052, 1077, 1664). Violates the `config.rs` → `src/tui/**`
  forbidden pair. Resolution: move `ThemeConfig` + related types out of
  `src/tui/theme.rs` into a layer-2 module (e.g., `src/config/theme.rs`
  or `src/theme.rs`), with the TUI consuming those types instead of
  defining them. Scope: non-trivial; deserves its own refactor PR.

**Why not `archunit-rs`:** emerging, unstable API, requires a new
dev-dependency, DSL more complex than the problem needs. 100 lines of
bash + a YAML manifest is reviewable and maintainable. Move to
`archunit-rs` only if the manifest grows complex enough that shell
parsing strains.

### Wave 2 exit criteria

**Hard (infrastructure complete):**

- `coverage` CI job green with ratchet active.
- `scripts/check-file-size.sh` at 400 LOC cap; full allowlist triaged
  (including 400-500 band additions).
- `scripts/architecture-layers.yml` + `scripts/check-layers.sh` in
  place; all existing violations in `docs/layers-debt.txt` with
  deadlines (including `src/config.rs` → `src/tui/theme` imports,
  known at design time).
- `docs/RUST-GUARDRAILS.md` appended with coverage and layer policy
  sections (end-of-document, no fixed numbering).

**Conditional (may extend beyond Wave 2 calendar):**

- **Core-tier coverage floor of 90% activated.** Only activates once
  the measured core coverage is ≥ 90%. If the Wave 2.1 baseline
  measurement shows core < 90%, this milestone is a follow-up
  test-writing project with its own calendar (expected 2-8 weeks
  depending on baseline). Wave 2 exits as "hard-complete" when the
  ratchet is live; floor activation is a separate tracking task.

- **TUI-tier coverage floor of 70% activated.** Same pattern — floor
  activates when baseline ≥ 70%.

## Wave 3 — Nightly heavyweight (scheduled, recent-green enforcement)

Three gates that cannot run per-PR. Expected timeline: 6-8 weeks wall
time — driven by warmup periods, not infrastructure complexity.

### Gate 3.1 — Mutation testing via `cargo-mutants`

**Scope:** core tier only (same paths as coverage core tier). TUI
excluded — mutation on rendering produces low-signal mutations mostly
caught by snapshot diff, not by behavior assertions.

**Threshold:** 80% mutation score on core tier. Below → nightly fails.

**Config:** `.cargo/mutants.toml`:

```toml
exclude_globs = [
  "src/tui/**",
  "src/main.rs",
  "src/lib.rs",
  "src/integration_tests/**",
  "**/tests.rs",
  "**/*_test.rs",
]
timeout_multiplier = 5.0
minimum_test_timeout = 60
```

**Runtime strategies:**

- **Sharding.** 4 shards via GitHub Actions matrix (`--shard N/4`).
  Full-run time: 1-2 hours per shard, 4 in parallel.
- **Mutant graph caching.** Persist mutant list; regenerate only when
  `src/` changes.
- **Incremental mode.** Opt-in via label for on-demand PR-specific
  runs; not the default.

**Warmup:** 2 weeks reporting-only before branch protection activates.
Iterate on timeouts and exclude-globs during warmup.

### Gate 3.2 — Miri

**Tool:** `cargo miri test`. Runs under Rust's UB interpreter. Catches:
use-after-free, double-free, data races, stacked-borrows violations,
uninitialized reads, out-of-bounds.

**Scope:** integration tests + parser + transition + state-store tests.
`cargo miri test --package maestro --test '*'` plus specific `#[test]`
functions in the narrow set.

**Limitations:** miri can't run FFI or certain syscalls. Tests that
spawn subprocesses (Claude CLI) are marked `#[cfg_attr(miri, ignore)]`
with a comment explaining why.

**Threshold:** pass/fail, no score.

**Runtime:** 15-45 minutes for the scoped set. No sharding.

**Warmup:** 1 week reporting-only.

### Gate 3.3 — ThreadSanitizer (weekly, informational)

**Tool:** `RUSTFLAGS="-Zsanitizer=thread" cargo test --target x86_64-unknown-linux-gnu -Z build-std`. Nightly Rust.

**Scope:** async tests (`#[tokio::test]`). Filtered via test-name pattern.

**Threshold:** none — informational only. Reports to a pinned GitHub
issue "Weekly ThreadSanitizer Report". Does NOT block main.

**Rationale for informational:** tsan produces false positives on tokio
internals (legitimate-but-looks-racy patterns). Making it blocking
would require a large suppressions list with its own maintenance burden.
Weekly report catches real regressions without the cost.

**Cadence:** Sundays 03:00 UTC.

### Workflow files

**`.github/workflows/nightly.yml`** (new):

```yaml
name: Nightly
on:
  schedule:
    - cron: '0 3 * * *'
  workflow_dispatch:

jobs:
  mutation:
    strategy:
      matrix:
        shard: [0, 1, 2, 3]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo install cargo-mutants --locked
      # --baseline=skip is mandatory when sharding — each shard would
      # otherwise re-run the baseline test suite, quadrupling the work.
      # The baseline-green assumption is enforced by the regular `test`
      # job in ci.yml (per-PR, blocking); mutation only runs nightly
      # after main's test suite is known-green.
      - run: cargo mutants --shard ${{ matrix.shard }}/4 --no-shuffle --baseline=skip

  miri:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: nightly
          components: miri, rust-src
      - uses: Swatinem/rust-cache@v2
      - run: cargo miri test --package maestro --test '*'
```

**`.github/workflows/weekly.yml`** (new):

```yaml
name: Weekly
on:
  schedule:
    - cron: '0 3 * * 0'  # Sunday 03:00 UTC

jobs:
  tsan:
    runs-on: ubuntu-latest
    env:
      RUSTFLAGS: "-Zsanitizer=thread"
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: nightly
          components: rust-src
      - run: cargo +nightly test --target x86_64-unknown-linux-gnu -Z build-std
      - name: Post report to pinned issue
        uses: peter-evans/create-or-update-comment@v4
        # ... post test results summary
```

### Freshness bot — branch protection for Wave 3

**`.github/workflows/freshness.yml`** (new) — a small GitHub Action that
runs on PR events and verifies the most recent scheduled nightly on
`main` succeeded within the past 3 days. Posts a status check named
`nightly-freshness` that is required by branch protection.

**Branch protection rule** updated to require:
- `Nightly / mutation (shard 0)` through `Nightly / mutation (shard 3)`
- `Nightly / miri`
- `nightly-freshness` (computed by the freshness bot)

Weekly tsan is NOT required (informational only).

**Why the bot:** GitHub native branch protection supports "must be
up-to-date with main" but not "nightly must be green within N days".
The bot fills that gap with ~50 lines of JavaScript in
`.github/workflows/freshness.yml` (or a dedicated
`.github/actions/freshness/` if it grows).

### Wave 3 exit criteria

- Mutation: 80% score on core tier. Branch protection active after
  2-week warmup.
- Miri: all scoped tests pass. Branch protection active after 1-week
  warmup.
- Tsan: weekly job lands, posts to pinned issue. No branch protection.
- Freshness bot ships and enforces 3-day recent-green window.
- `docs/RUST-GUARDRAILS.md` appended with mutation and runtime-safety
  policy sections.

## Rollout Calendar

Indicative (not hard-committed). Assumes half-time investment in CI
work; multiply by 0.6 for full focus or 1.5-2x for mostly-feature-mode.

| Phase | Start | End | Nature |
|-------|-------|-----|--------|
| Wave 1 | 2026-04-28 | 2026-05-05 | Config tightenings + allowlist format change |
| Wave 1b (triage) | 2026-05-05 | 2026-05-12 | Assign real deadlines/owners to 23 allowlist entries (after stale-path removal in Phase 1b) |
| Wave 2.1 (coverage) | 2026-05-05 | 2026-05-26 | cargo-llvm-cov infra + tier manifest + ratchet |
| Wave 2.2 (file size) | 2026-05-12 | 2026-05-19 | Cap 500 → 400 + 400-500 band allowlist triage |
| Wave 2.3 (layers) | 2026-05-19 | 2026-06-02 | Manifest + script + debt-file baseline |
| Wave 3.1 (mutation) | 2026-06-02 | 2026-07-14 | Infra → 2-week warmup → blocking |
| Wave 3.2 (miri) | 2026-06-09 | 2026-06-23 | Infra → 1-week warmup → blocking |
| Wave 3.3 (tsan, weekly) | 2026-06-16 | 2026-06-23 | Infra → pinned-issue report |
| Freshness bot | 2026-07-07 | 2026-07-14 | Wires branch protection for 3.1 + 3.2 |

Total wall time: 2026-04-28 → 2026-07-14, roughly 11 weeks.

**Serialization:**

- Wave 1 → Wave 1b (same file, follow-up triage).
- Wave 1b → Wave 2.2 (tightens the cap the allowlist guards).
- Wave 2.1, 2.2, 2.3 run in parallel after prerequisites.
- Wave 3 requires Wave 2 fully merged.

## Accountability Mechanisms

### The deadlined-allowlist pattern

Used in both `scripts/allowlist-large-files.txt` and
`docs/layers-debt.txt`. Same mechanics, different content.

**Entry format:**

```
<path or rule> # deadline: YYYY-MM-DD, owner: @handle, ticket: #N, plan: <brief>
```

**Deadline enforcement:** each check script has a loop that fails when
`deadline < today`. See Wave 1.4 snippet.

**Dealing with a slip:**

- **Option A — refactor the file.** Intended path. Removes the entry.
- **Option B — extend the deadline.** PR bumps the date. Reviewer
  expected to push back on habitual extensions.
- **Option C — remove the gate.** ADR-level change to RUST-GUARDRAILS.
  Rare.

**What NOT to do:** disable the gate temporarily, silently remove from
allowlist without refactoring, bypass CI via `--no-verify` (already
forbidden by CLAUDE.md §1).

### Blast radius summary

| Gate | Trigger | Blast radius |
|------|---------|-------------|
| Wave 1 — clippy/deny/CCN | Per-PR | PR blocked until fixed |
| Wave 1.4 — allowlist deadline past | Per-PR | Main red until extended or refactored |
| Wave 2.1 — coverage tier floor | Per-PR | PR blocked; grace via ratchet |
| Wave 2.1 — coverage ratchet | Per-PR | PR blocked until tests added |
| Wave 2.2 — file size 400 | Per-PR | PR blocked |
| Wave 2.3 — layer violation | Per-PR | PR blocked |
| Wave 3.1 — mutation score < 80% | Nightly | Freshness bot blocks main after 3 days |
| Wave 3.2 — miri error | Nightly | Freshness bot blocks main after 3 days |
| Wave 3.3 — tsan | Weekly | No block — pinned issue |

### Rollback strategy

Each wave lands as one or more PRs. Rollback is a single revert commit
followed by a re-design PR that tunes the config. Wave 3 rollback is
gentler: flip the `required` flag on branch protection. Nightly still
runs, just doesn't block. Investigate, then re-activate.

## Testing Strategy

Same layered approach as the `/implement` harness.

**Layer 1 — Shell-script tests via `bats`:**

- `tests/scripts/check-file-size.bats` — old-format migration,
  deadline-past failure, deadline-future pass, missing-deadline error.
- `tests/scripts/check-layers.bats` — same-layer pass, lower-layer
  pass, higher-layer fail, forbidden-pair fail.
- `tests/scripts/check-coverage-tiers.bats` — synthetic lcov inputs at
  various tier percentages.

**Layer 2 — YAML manifest schema tests:**

- `tests/manifests/validate-manifests.py` — Python `unittest` (stdlib)
  that parses `scripts/coverage-tiers.yml` and
  `scripts/architecture-layers.yml`, validates required fields, asserts
  paths exist.

**Layer 3 — CI workflow smoke:**

- Manual procedure documented at `docs/ci-smoke-check.md`. Scratch PR
  deliberately introduces a violation for each gate; confirms CI fails
  with the right message. Not automated — running this on every main
  push isn't worth the CI cost.

**Layer 4 — Freshness bot unit tests:**

- `.github/actions/freshness/test/*.test.js` using Node 18+ stdlib
  runner. Covers: nightly pass < 3 days ago → allow; nightly fail →
  block; no nightly ran yet → block; aged-out nightly → block.

**Layer 5 — Nightly job dry runs:**

- Warmup period for each Wave 3 gate. Reporting-only mode. Integration
  test by exposure — if warmup surfaces tuning issues, fix before
  activating branch protection.

## File Inventory

### New files

| Path | Purpose |
|------|---------|
| `.github/workflows/nightly.yml` | Mutation + miri schedule |
| `.github/workflows/weekly.yml` | ThreadSanitizer schedule |
| `.github/workflows/freshness.yml` (or `.github/actions/freshness/`) | Freshness bot |
| `scripts/coverage-tiers.yml` | Coverage tier manifest |
| `scripts/architecture-layers.yml` | Layer manifest + forbidden pairs |
| `scripts/check-coverage-tiers.sh` | Coverage floor + ratchet enforcement |
| `scripts/check-layers.sh` | Layer violation enforcement |
| `docs/layers-debt.txt` | Deadlined debt file for existing layer violations |
| `docs/ci-smoke-check.md` | Manual smoke procedure |
| `tests/scripts/check-file-size.bats` | Bats tests for file-size script |
| `tests/scripts/check-layers.bats` | Bats tests for layers script |
| `tests/scripts/check-coverage-tiers.bats` | Bats tests for coverage script |
| `tests/scripts/fixtures/*` | Test-data inputs |
| `tests/manifests/validate-manifests.py` | Python unittest for manifest schemas |
| `.cargo/mutants.toml` | cargo-mutants config |

### Modified files

| Path | Change |
|------|--------|
| `.github/workflows/ci.yml` | Add `clippy-nursery`, `coverage`, `layers` jobs |
| `clippy.toml` | `cognitive-complexity-threshold` 25 → 20 |
| `deny.toml` | `multiple-versions` warn → deny + skip list |
| `src/lib.rs` | Add curated nursery `#![warn(...)]` |
| `src/main.rs` | Same nursery warnings |
| `scripts/check-file-size.sh` | Deadline parsing; `MAX_LINES` 500 → 400 in Wave 2 |
| `scripts/allowlist-large-files.txt` | Format change to deadlined entries |
| `docs/RUST-GUARDRAILS.md` | New §10-13 documenting each new gate |
| `.claude/hooks/preflight.sh` | Populate with fast per-PR gates |

### Not modified (explicitly)

- `.claude/commands/implement.md` — harness spec's implement command;
  this project only populates its preflight hook.
- `rustfmt.toml` — no fmt policy changes.
- `rust-toolchain.toml` — no toolchain changes (stays stable).

## Open Questions

Surfaced during brainstorm, explicitly parked:

1. **Curated nursery subset refinement.** The 7 proposed lints are a
   best-guess. Wave 1 measurement step may reveal 1-2 are too noisy
   (likely `missing_const_for_fn`); plan includes a quick re-evaluation
   before committing.

2. **Coverage baseline measurement.** Current coverage is unknown. Must
   be measured before Wave 2.1 floor gates can be set realistically.
   Plan includes a measurement task before the gate activates.

3. **Per-file vs per-tier coverage floors.** Design uses per-tier
   weighted means. If this masks a 40%-covered file averaged against a
   98%-covered file, per-file minimums may be needed. Deferred until
   baseline measured.

4. **Freshness bot hosting.** Default: GitHub Action posting a status
   check. Alternative: Cloudflare Workers serverless endpoint. Action
   simpler; spec commits to Action unless plan surfaces a reason
   otherwise.

5. **`cargo-mutants` shard count.** 4 is a reasonable default. If total
   nightly time exceeds 6 hours, bump to 8. Tuned in plan phase after
   first measurement.

6. **YAML parsing in shell scripts.** Default: `yq` (apt/brew
   installable, already common on CI runners). Alternative: `pyyaml`
   (violates stdlib-only promise) or hand-parse. `yq` selected.

7. **Deadline-extension pattern detection.** Detecting "repeated
   extensions on the same file" requires parsing git log. Nice-to-have
   but not v1. For now, extensions caught by PR review.

8. **Benchmark ratchet.** Explicitly a non-goal for this spec. Separate
   spec if/when needed.

9. **CI smoke check — manual vs. maintained-broken-branch.** v1 is
   manual (`docs/ci-smoke-check.md`). Upgrade to a maintained
   `test-ci-enforcement` branch if the manual procedure becomes stale
   or is skipped before releases.

## References

- `docs/superpowers/specs/2026-04-21-implement-harness-enforcement-design.md`
  — the `/implement` harness spec; this project populates its
  `preflight.sh` hook at the end of Wave 1.
- `docs/RUST-GUARDRAILS.md` — the 8-principle policy this spec
  extends. Sections 10-13 added by this project.
- `.github/workflows/ci.yml` — existing CI; grows with new jobs.
- `scripts/check-file-size.sh` — existing file-size gate; gets
  deadline-parsing logic added.
- `.claude/CLAUDE.md` — orchestrator rules; no changes.
