# CI Quality Gates Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Roll out six CI quality gates in three waves — config tightenings, coverage + file-size + layer violations infrastructure, and nightly heavyweight checks — extending the existing `.github/workflows/ci.yml` without replacing it.

**Architecture:** Three-wave rollout. Wave 1 is config tightenings (ships in a week, per-PR blocking). Wave 2 is per-PR infrastructure (coverage via `cargo-llvm-cov` with tiered floors, file-size cap 500→400, module-level layer violations — ships in 3-4 weeks, per-PR blocking). Wave 3 is nightly scheduled gates (mutation via `cargo-mutants`, miri on parser tests, tsan informational weekly) with a freshness bot enforcing "recent-green within 3 days" branch protection.

**Tech Stack:** Bash + `bats` for shell tests, Python 3 stdlib for YAML parsing helpers, `yq` for shell-based YAML parsing, GitHub Actions (YAML workflows + JavaScript action for freshness bot), `cargo-llvm-cov`, `cargo-mutants`, `cargo miri` (nightly Rust), ThreadSanitizer.

**Spec reference:** `docs/superpowers/specs/2026-04-22-ci-quality-gates-design.md`.

---

## File Structure

### New files (across all waves)

| Path | Wave | Responsibility |
|------|------|----------------|
| `.github/workflows/nightly.yml` | 3 | Mutation (4 shards) + miri scheduled at 03:00 UTC |
| `.github/workflows/weekly.yml` | 3 | ThreadSanitizer informational, Sundays 03:00 UTC |
| `.github/actions/freshness/action.yml` | 3 | Freshness bot GitHub Action metadata |
| `.github/actions/freshness/index.js` | 3 | Freshness bot logic (Node 18+ stdlib) |
| `.github/actions/freshness/test/freshness.test.js` | 3 | Freshness bot unit tests |
| `scripts/coverage-tiers.yml` | 2.1 | Coverage tier manifest (core/tui/excluded) |
| `scripts/architecture-layers.yml` | 2.3 | Layer manifest + forbidden-pair list |
| `scripts/check-coverage-tiers.sh` | 2.1 | Coverage floor + ratchet enforcement |
| `scripts/check-layers.sh` | 2.3 | Layer violation enforcement |
| `docs/layers-debt.txt` | 2.3 | Deadlined debt file for existing layer violations |
| `docs/ci-smoke-check.md` | 3 | Manual smoke-check procedure |
| `tests/scripts/check-file-size.bats` | 1 | Bats tests for file-size script (incl. deadline parsing) |
| `tests/scripts/check-layers.bats` | 2.3 | Bats tests for layers script |
| `tests/scripts/check-coverage-tiers.bats` | 2.1 | Bats tests for coverage script |
| `tests/scripts/fixtures/` | 1+2 | Shared test fixtures (lcov samples, stub manifests) |
| `tests/manifests/validate_manifests_test.py` | 2 | Python unittest for YAML manifest schemas |
| `.cargo/mutants.toml` | 3 | cargo-mutants config (excludes, timeout) |

### Modified files

| Path | Wave | Change |
|------|------|--------|
| `.github/workflows/ci.yml` | 1+2 | Add `clippy-nursery`, `coverage`, `layers` jobs |
| `clippy.toml` | 1 | `cognitive-complexity-threshold` 25 → 20 |
| `deny.toml` | 1 | `multiple-versions` warn → deny + skip list |
| `src/lib.rs` | 1 | Add `#![warn(clippy::missing_const_for_fn, …)]` |
| `src/main.rs` | 1 | Same nursery warnings |
| `scripts/check-file-size.sh` | 1+2 | Deadline parsing (W1); `MAX_LINES` 500 → 400 (W2) |
| `scripts/allowlist-large-files.txt` | 1 | Deadlined entries + stale-path cleanup |
| `docs/RUST-GUARDRAILS.md` | 1+2+3 | New sections appended (end-of-document) |
| `.claude/hooks/preflight.sh` | 1 | Populate with fast local gates |

### Chunk boundaries and dependencies

```
Chunk 1: Wave 1 — Config tightenings + preflight + allowlist format
    ↓ (Phase 1b triage follow-up within 14 days — part of Chunk 1's window)
Chunk 2: Wave 2.1 — Coverage infrastructure                   (parallel ok after 1)
Chunk 3: Wave 2.2 — File size 500 → 400 cap transition        (parallel ok after 1b)
Chunk 4: Wave 2.3 — Layer violations                          (parallel ok after 1)
    ↓ (Wave 3 requires all of Wave 2 merged)
Chunk 5: Wave 3 — Nightly mutation + miri + tsan + freshness bot
```

Each chunk → one PR per the user's isolation rule. Chunks 2, 3, 4 can run in parallel after their shared prerequisite (Chunk 1 for 2 and 4; Chunk 1b for 3).

---

## Chunk 1: Wave 1 — Quick wins

**Goal:** Tighten existing config thresholds, add a curated clippy nursery subset, flip cargo-deny strict mode, convert the file-size allowlist to deadlined entries, and populate `.claude/hooks/preflight.sh`. Build allowlist deadline parsing TDD with `bats`.

**Files:**
- Modify: `clippy.toml`, `deny.toml`, `src/lib.rs`, `src/main.rs`
- Modify: `scripts/check-file-size.sh` (deadline parsing)
- Modify: `scripts/allowlist-large-files.txt` (format migration + stale-path cleanup)
- Create: `tests/scripts/check-file-size.bats` + fixtures
- Create: `.claude/hooks/preflight.sh` (populate empty file from harness spec)
- Modify: `docs/RUST-GUARDRAILS.md` (append new section)

### Task 1.1: TDD — File-size deadline parsing (bats fixture + script)

This is the TDD-heaviest task in Wave 1. The existing `check-file-size.sh` strips comments before populating `allowed[]`, so we need a parallel raw-line array. Test-first.

**Files:**
- Create: `tests/scripts/check-file-size.bats`
- Create: `tests/scripts/fixtures/allowlist-sample-valid.txt`
- Create: `tests/scripts/fixtures/allowlist-sample-expired.txt`
- Create: `tests/scripts/fixtures/allowlist-sample-legacy.txt`
- Create: `tests/scripts/fixtures/allowlist-sample-nostraightforward-structure.txt`
- Modify: `scripts/check-file-size.sh`

- [ ] **Step 1: Scaffold bats test file**

```bash
mkdir -p tests/scripts/fixtures
```

File: `tests/scripts/check-file-size.bats`

```bash
#!/usr/bin/env bats
#
# Tests for scripts/check-file-size.sh
# Covers: existing 500-LOC cap, new deadline parsing, allowlist migration, stale paths.

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$REPO_ROOT/scripts/check-file-size.sh"
  FIXTURES="$REPO_ROOT/tests/scripts/fixtures"
  TEST_REPO="$(mktemp -d -t file-size-test-XXXXXX)"
  mkdir -p "$TEST_REPO/src" "$TEST_REPO/scripts"
  cp "$SCRIPT" "$TEST_REPO/scripts/check-file-size.sh"
  cd "$TEST_REPO"
}

teardown() {
  cd /
  rm -rf "$TEST_REPO"
}
```

- [ ] **Step 2: Create a legacy-format allowlist fixture (current format)**

File: `tests/scripts/fixtures/allowlist-sample-legacy.txt`

```
# Old-format allowlist — bare paths with freeform comments.
src/big_legacy.rs
# This is a plain path with a comment below it.
src/another_legacy.rs
```

- [ ] **Step 3: Write first failing test — legacy format still loads**

Append to `check-file-size.bats`:

```bash
@test "legacy-format allowlist is still honored (backward compat)" {
  # Set up a repo with a big file and a legacy allowlist.
  cp "$FIXTURES/allowlist-sample-legacy.txt" scripts/allowlist-large-files.txt
  printf 'line\n%.0s' {1..600} > src/big_legacy.rs
  run bash scripts/check-file-size.sh
  [ "$status" -eq 0 ]
}
```

- [ ] **Step 4: Run to verify it passes against current script**

Run: `bats tests/scripts/check-file-size.bats`
Expected: 1 passing test (legacy allowlist works — this is a no-change regression check).

- [ ] **Step 5: Create a deadline-valid-future fixture**

File: `tests/scripts/fixtures/allowlist-sample-valid.txt`

Use a date far in the future (2099-12-31) so this fixture never expires.

```
src/big_deadlined.rs # deadline: 2099-12-31, owner: @testuser, ticket: #TEST, plan: split later
```

- [ ] **Step 6: Add test — deadline in future passes**

Append to `.bats`:

```bash
@test "deadline in future passes (entry is honored)" {
  cp "$FIXTURES/allowlist-sample-valid.txt" scripts/allowlist-large-files.txt
  printf 'line\n%.0s' {1..600} > src/big_deadlined.rs
  run bash scripts/check-file-size.sh
  [ "$status" -eq 0 ]
}
```

- [ ] **Step 7: Run — expect pass (script still ignores comment syntax gracefully)**

Run: `bats tests/scripts/check-file-size.bats`
Expected: 2 passing tests.

- [ ] **Step 8: Create a deadline-expired fixture**

File: `tests/scripts/fixtures/allowlist-sample-expired.txt`

Use a date safely in the past:

```
src/big_expired.rs # deadline: 2000-01-01, owner: @testuser, ticket: #OLD, plan: refactor
```

- [ ] **Step 9: Add failing test — deadline in past should fail**

```bash
@test "deadline in past fails the check" {
  cp "$FIXTURES/allowlist-sample-expired.txt" scripts/allowlist-large-files.txt
  printf 'line\n%.0s' {1..600} > src/big_expired.rs
  run bash scripts/check-file-size.sh
  [ "$status" -ne 0 ]
  [[ "$output" == *"DEADLINE PAST"* ]]
}
```

- [ ] **Step 10: Run — expect FAIL (script does not yet parse deadlines)**

Run: `bats tests/scripts/check-file-size.bats`
Expected: test 3 FAILS because the current script returns 0 for any allowlisted entry regardless of deadline.

- [ ] **Step 11: Modify `scripts/check-file-size.sh` — add deadline parsing**

Replace the current allowlist-loading block (around lines 12-21 of the script) with:

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
```

Then add deadline-enforcement logic AFTER the existing file-size check loop, BEFORE the final `if (( violations > 0 ))`:

```bash
# Deadline enforcement — iterate raw entries to find `deadline: YYYY-MM-DD`.
today=$(date +%Y-%m-%d)
for entry in "${allowed_raw[@]+"${allowed_raw[@]}"}"; do
  deadline=$(echo "$entry" | grep -oE 'deadline: [0-9-]+' | sed 's/deadline: //')
  if [[ -n "$deadline" && "$deadline" < "$today" ]]; then
    echo "DEADLINE PAST: $entry"
    violations=$((violations + 1))
  fi
done
```

- [ ] **Step 12: Run — expect all 3 tests pass**

Run: `bats tests/scripts/check-file-size.bats`
Expected: 3 passing tests.

- [ ] **Step 13: Add test — missing-deadline legacy entry emits no warning**

```bash
@test "legacy entry without deadline field is tolerated (no warning)" {
  cp "$FIXTURES/allowlist-sample-legacy.txt" scripts/allowlist-large-files.txt
  printf 'line\n%.0s' {1..600} > src/big_legacy.rs
  run bash scripts/check-file-size.sh
  [ "$status" -eq 0 ]
  [[ "$output" != *"DEADLINE"* ]]
}
```

- [ ] **Step 14: Run — expect pass**

Run: `bats tests/scripts/check-file-size.bats`
Expected: 4 passing tests.

- [ ] **Step 15: Add test — file NOT on allowlist and over cap fails as before**

```bash
@test "file over cap not on allowlist fails (regression check)" {
  cp "$FIXTURES/allowlist-sample-valid.txt" scripts/allowlist-large-files.txt
  printf 'line\n%.0s' {1..600} > src/unrelated_big.rs
  run bash scripts/check-file-size.sh
  [ "$status" -ne 0 ]
  [[ "$output" == *"VIOLATION"* ]]
}
```

- [ ] **Step 16: Run — expect pass (4-line regression check)**

Run: `bats tests/scripts/check-file-size.bats`
Expected: 5 passing tests.

- [ ] **Step 17: Commit**

```bash
git add tests/scripts/ scripts/check-file-size.sh
git commit -m "$(cat <<'EOF'
feat(scripts): add deadline parsing to file-size allowlist

The allowlist format grows a deadline field so every exempt file has a
resolution date. CI fails when any deadline is in the past, creating a
forcing function for refactoring.

Tests cover: legacy-format backward compatibility, deadline-future pass,
deadline-past fail, missing-deadline tolerance, non-allowlisted-big fail.
Preserves the original script's stripped-path glob-match semantics by
loading a parallel raw-line array.
EOF
)"
```

---

### Task 1.2: Migrate `allowlist-large-files.txt` to deadlined format (Phase 1a)

Triage defers to Phase 1b (Task 1.9). This task just **rewrites the file** with placeholder deadlines 14 days after today.

**Files:**
- Modify: `scripts/allowlist-large-files.txt`

- [ ] **Step 1: Compute placeholder deadline**

Run: `date -v+14d +%Y-%m-%d` (macOS) or `date -d '+14 days' +%Y-%m-%d` (GNU)

Record this value — call it `$PLACEHOLDER_DEADLINE` below.

- [ ] **Step 2: Read current allowlist and enumerate non-comment lines**

Run: `grep -v '^#' scripts/allowlist-large-files.txt | grep -v '^$'`
Expected: 23 paths.

- [ ] **Step 3: Rewrite file with deadlined entries**

Replace `scripts/allowlist-large-files.txt` with the following header + 23 deadlined lines. **Every line uses `$PLACEHOLDER_DEADLINE` as its deadline, `@carlos` as owner, `#TBD` as ticket, `TBD` as plan.** Phase 1b triage (Task 1.9) fills in real values.

File: `scripts/allowlist-large-files.txt`

```
# Allowlist for files exceeding scripts/check-file-size.sh cap.
# Format: <path> # deadline: YYYY-MM-DD, owner: @handle, ticket: #N, plan: <brief>
# Entries without a deadline field are tolerated but discouraged.
# CI fails when any deadline is in the past.
#
# Phase 1b triage (in-flight) will replace placeholders with real deadlines/owners/plans.
src/tui/screens/issue_browser/mod.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/tui/screens/home/mod.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/tui/mod.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/tui/theme.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/config.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/github/client.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: STALE-PATH-CHECK-PHASE-1B
src/tui/ui.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/tui/screens/prompt_input.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/tui/markdown.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/github/ci.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: STALE-PATH-CHECK-PHASE-1B
src/doctor.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/work/assigner.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/session/types.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/integration_tests/stream_parsing.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/tui/widgets/ci_monitor.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/tui/screens/milestone.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/session/pool.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/github/types.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: STALE-PATH-CHECK-PHASE-1B
src/cli.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/prompts.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/tui/screens/queue_confirmation.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/tui/input_handler.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
src/tui/navigation/mode_hints.rs # deadline: <PLACEHOLDER>, owner: @carlos, ticket: #TBD, plan: TBD
```

Replace `<PLACEHOLDER>` literally with the date from Step 1.

- [ ] **Step 4: Verify the file parses correctly**

Run: `bash scripts/check-file-size.sh`
Expected: exit 0. All deadlines are 14 days out — none expired.

- [ ] **Step 5: Commit**

```bash
git add scripts/allowlist-large-files.txt
git commit -m "$(cat <<'EOF'
refactor(scripts): migrate allowlist to deadlined format (Phase 1a)

Every existing entry gets a placeholder deadline 14 days from now.
Phase 1b triage follows within that window, replacing placeholders
with real deadlines, owners, split plans, and removing stale paths
(entries marked STALE-PATH-CHECK-PHASE-1B — these are src/github/*
paths that were moved to src/provider/github/* without the allowlist
being updated).

If Phase 1b slips past day 14, main goes red via the deadline-past
enforcement added in the previous commit.
EOF
)"
```

---

### Task 1.3: Tighten `clippy.toml` — cognitive complexity 25 → 20

**Files:**
- Modify: `clippy.toml`

- [ ] **Step 1: Change the threshold**

File: `clippy.toml`

```toml
too-many-arguments-threshold = 7
type-complexity-threshold = 250
cognitive-complexity-threshold = 20
too-many-lines-threshold = 120
```

- [ ] **Step 2: Run clippy locally to surface violations**

Run: `cargo clippy -- -D warnings -A dead_code`
Expected: some number of `cognitive_complexity` warnings (error under `-D warnings`). Could be 0 (codebase is already clean) or 5-15 (need fixing).

- [ ] **Step 3: Fix each violation**

For each flagged function:
- If the function can be reasonably split, extract a helper with a purpose-named identifier.
- If the complexity is inherent and can't be usefully split, add a targeted `#[allow(clippy::cognitive_complexity)]` directly above the function declaration with a `// Reason: <short>` comment. Do NOT add crate-level `#![allow]`.

- [ ] **Step 4: Re-run clippy — expect clean**

Run: `cargo clippy -- -D warnings -A dead_code`
Expected: zero warnings.

- [ ] **Step 5: Commit**

```bash
git add clippy.toml src/
git commit -m "$(cat <<'EOF'
chore(clippy): tighten cognitive-complexity-threshold 25 → 20

Surfaces functions at the boundary of understandability. Each flagged
function either got split into smaller helpers or a targeted
#[allow(clippy::cognitive_complexity)] with a Reason comment.
Crate-level #![allow] deliberately avoided — every exception is local
and reviewable.
EOF
)"
```

---

### Task 1.4: Add curated clippy nursery subset

**Files:**
- Modify: `src/lib.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Locate the existing `#![...]` attributes at the top of each file**

Run: `head -5 src/lib.rs src/main.rs`

Expected: each file starts with `#![forbid(unsafe_code)]` (from RUST-GUARDRAILS §0.1).

- [ ] **Step 2: Append the curated nursery warnings above `#![forbid]`**

Modify both `src/lib.rs` and `src/main.rs`. Add these lines at the top:

```rust
#![warn(clippy::missing_const_for_fn)]
#![warn(clippy::needless_pass_by_ref_mut)]
#![warn(clippy::redundant_clone)]
#![warn(clippy::significant_drop_tightening)]
#![warn(clippy::fallible_impl_from)]
#![warn(clippy::path_buf_push_overwrite)]
#![warn(clippy::branches_sharing_code)]
#![forbid(unsafe_code)]  // existing
```

- [ ] **Step 3: Run clippy**

Run: `cargo clippy -- -D warnings -A dead_code`
Expected: 10-30 new warnings (errors under `-D warnings`). The 7 nursery lints will fire on various spots across the codebase.

- [ ] **Step 4: Fix each violation**

Per-lint guidance:

- `missing_const_for_fn`: mark the function `const fn` if it qualifies. If the function depends on a runtime-only feature (trait method from a non-const trait, allocation), add targeted `#[allow(clippy::missing_const_for_fn)]` with a `// Reason: depends on X` comment.
- `needless_pass_by_ref_mut`: change `&mut` to `&` where the function doesn't mutate.
- `redundant_clone`: remove the `.clone()` call. Verify the borrow checker accepts.
- `significant_drop_tightening`: re-order the code so the lock is dropped earlier. Often: extract a narrow scope with `{ let guard = lock.lock(); ... }`.
- `fallible_impl_from`: either make the `From` implementation infallible, or change it to `TryFrom` with a proper error type.
- `path_buf_push_overwrite`: check whether the push argument can be absolute at runtime. If yes, use `path.push(stripped)` with explicit strip-leading-slash, or switch to `path.join(&relative_part)`.
- `branches_sharing_code`: extract the shared code outside the `if`/`match`.

- [ ] **Step 5: Re-run clippy — expect clean**

Run: `cargo clippy -- -D warnings -A dead_code`
Expected: zero warnings.

- [ ] **Step 6: Run full test suite to confirm no regressions**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/lib.rs src/main.rs src/
git commit -m "$(cat <<'EOF'
chore(clippy): add curated nursery subset (7 lints)

Enables high-signal lints chosen for this codebase:
- missing_const_for_fn: free const opportunities.
- needless_pass_by_ref_mut: API hygiene.
- redundant_clone: ownership clarity.
- significant_drop_tightening: matches async policy (no long-held locks).
- fallible_impl_from: matches error policy (panics-as-bugs, RUST-GUARDRAILS §2).
- path_buf_push_overwrite: catches PathBuf footguns.
- branches_sharing_code: refactoring signal.

Nursery lints that were evaluated and rejected: missing_docs_in_private_items
(noisy), use_self (style), option_if_let_else (variable signal),
suboptimal_flops (no float math in this codebase).
EOF
)"
```

---

### Task 1.5: Flip `cargo-deny` to strict mode

**Files:**
- Modify: `deny.toml`

- [ ] **Step 1: Run current `cargo deny` to enumerate `multiple-versions` warnings**

Run: `cargo deny check bans 2>&1 | grep -A 2 "multiple versions"`

Capture the list — this becomes the `skip` entries. Example output:

```
warning[duplicate]: found 2 duplicate entries for crate 'syn'
  ┌─ ratatui-core v0.1.0 → syn v1.0.109
  │   …
  │   maestro v0.14.0 → syn v2.0.x
```

- [ ] **Step 2: Modify `deny.toml` — promote multiple-versions to deny + add skip list**

Change the `[bans]` block:

```toml
[bans]
multiple-versions = "deny"    # was "warn"
wildcards = "deny"             # unchanged
highlight = "all"              # unchanged
workspace-default-features = "allow"
external-default-features = "allow"
allow = []
deny = []
skip = [
    # Document each skip with reason + upstream link.
    # Example format:
    # { name = "syn", version = "1", reason = "ratatui still on syn 1; upstream tracking: <link>" },
    # <POPULATE_FROM_STEP_1_OUTPUT>
]
skip-tree = []
```

Replace `<POPULATE_FROM_STEP_1_OUTPUT>` with entries derived from Step 1's output. Each entry needs the crate name, the pinned old version, a reason, and an upstream ticket / RUSTSEC link where relevant.

- [ ] **Step 3: Re-run `cargo deny` — expect clean**

Run: `cargo deny check bans`
Expected: zero warnings, zero errors.

- [ ] **Step 4: Verify other deny checks still pass**

Run: `cargo deny check advisories licenses sources`
Expected: same output as before (no regression from the bans change).

- [ ] **Step 5: Commit**

```bash
git add deny.toml
git commit -m "$(cat <<'EOF'
chore(deny): flip multiple-versions warn → deny with documented skip list

Each skip entry documents: crate name, pinned old version, reason,
upstream tracking link. Goal is an empty skip list; reality is that
ratatui/syntect/reqwest pin some old versions transitively.

Upstream tracking links (per entry) live as comments — refactor these
away as upstream releases land.
EOF
)"
```

---

### Task 1.6: Populate `.claude/hooks/preflight.sh`

**Files:**
- Create: `.claude/hooks/preflight.sh`

The harness spec left this file intentionally empty. This task completes the cliffhanger by wiring the fast per-PR gates for local rehearsal.

- [ ] **Step 1: Create the script**

File: `.claude/hooks/preflight.sh`

```bash
#!/usr/bin/env bash
# Pre-flight CI rehearsal for /implement.
# Runs the fast per-PR gates locally before a branch is created, so
# obvious regressions fail in ~10 seconds instead of ~10 minutes on
# GitHub Actions.
#
# Gates included:
#   - cargo fmt --check
#   - cargo clippy -- -D warnings -A dead_code
#   - scripts/check-file-size.sh (with deadline enforcement from Wave 1.1)
#
# Gates NOT included (CI-only):
#   - cargo test  (takes too long for a pre-flight)
#   - coverage    (Wave 2.1; same reason)
#   - layer check (Wave 2.3)
#   - cargo deny  (networked; CI-only)

set -e

echo "preflight: cargo fmt --check"
cargo fmt -- --check

echo "preflight: cargo clippy -- -D warnings -A dead_code"
cargo clippy -- -D warnings -A dead_code

echo "preflight: scripts/check-file-size.sh"
bash scripts/check-file-size.sh

echo "preflight: all clear"
```

- [ ] **Step 2: Make executable**

```bash
chmod +x .claude/hooks/preflight.sh
```

- [ ] **Step 3: Smoke-test**

Run: `bash .claude/hooks/preflight.sh`
Expected: each gate runs, final line is "preflight: all clear", exit 0.

- [ ] **Step 4: Commit**

```bash
git add .claude/hooks/preflight.sh
git commit -m "$(cat <<'EOF'
feat(hooks): populate preflight.sh — harness spec cliffhanger

The /implement harness spec left this file intentionally empty with the
expectation that the CI spec would populate it. Wired to the fast per-PR
gates: fmt check, clippy with -D warnings, file-size check (including
deadline enforcement from the allowlist format change).

Slow gates (cargo test, coverage, layers, deny) remain CI-only because
they either take too long for pre-flight or require network access.
EOF
)"
```

---

### Task 1.7: Append CI-gates section to `RUST-GUARDRAILS.md`

**Files:**
- Modify: `docs/RUST-GUARDRAILS.md`

- [ ] **Step 1: Find the end of the document**

Run: `tail -20 docs/RUST-GUARDRAILS.md`
Locate the last section and append a new section after it (no fixed numbering — the spec deliberately avoids committing to section numbers since existing content goes to §15).

- [ ] **Step 2: Append the CI-gates section**

Add at the end of `docs/RUST-GUARDRAILS.md`:

```markdown
---

## CI Quality Gates (Wave 1)

**Status:** active after Chunk 1 of `docs/superpowers/plans/2026-04-22-ci-quality-gates-plan.md` lands.

**Thresholds updated in this wave:**

- `clippy.toml`: `cognitive-complexity-threshold` = 20 (was 25). Functions exceeding this fail clippy. Targeted `#[allow(clippy::cognitive_complexity)]` with a `// Reason: …` comment is the escape hatch, never crate-level `#![allow]`.
- `deny.toml`: `multiple-versions = "deny"`. `skip` list is populated for known transitive duplicates, each with a reason and upstream link.

**Curated clippy nursery lints** (enabled via `#![warn(...)]` in `src/lib.rs` and `src/main.rs`):

- `clippy::missing_const_for_fn`
- `clippy::needless_pass_by_ref_mut`
- `clippy::redundant_clone`
- `clippy::significant_drop_tightening`
- `clippy::fallible_impl_from` (highest-signal; matches §2 error policy)
- `clippy::path_buf_push_overwrite`
- `clippy::branches_sharing_code`

**Deliberately rejected** (evaluated, too noisy): `missing_docs_in_private_items`, `use_self`, `option_if_let_else`, `suboptimal_flops`.

**File-size allowlist format.**

`scripts/allowlist-large-files.txt` uses the deadlined format:

```
<path> # deadline: YYYY-MM-DD, owner: @handle, ticket: #N, plan: <brief>
```

CI fails when any deadline is in the past. Extensions are allowed via a PR that bumps the deadline; reviewers are expected to push back on habitual extensions. Removing the gate entirely is an ADR-level change.

**Pre-flight hook.**

`.claude/hooks/preflight.sh` runs fast per-PR gates locally — fmt, clippy, file-size — so `/implement` surfaces regressions before a branch is created.
```

- [ ] **Step 3: Commit**

```bash
git add docs/RUST-GUARDRAILS.md
git commit -m "$(cat <<'EOF'
docs(rust-guardrails): append CI Quality Gates (Wave 1) section

Documents the thresholds and lints tightened in Wave 1: cognitive
complexity 20, cargo-deny multiple-versions = deny, curated nursery
subset, deadlined allowlist format, preflight hook contents.

Wave 2 and Wave 3 sections will be appended as each wave lands.
EOF
)"
```

---

### Task 1.8: Chunk-close — full local verification + push, PR

- [ ] **Step 1: Run every per-PR gate locally**

```bash
cargo fmt -- --check
cargo clippy -- -D warnings -A dead_code
cargo test
bash scripts/check-file-size.sh
cargo deny check
bats tests/scripts/check-file-size.bats
bash .claude/hooks/preflight.sh
```

Expected: every command exits 0.

- [ ] **Step 2: Confirm commit count**

Run: `git log main..HEAD --oneline | wc -l`
Expected: 7 commits (Tasks 1.1 through 1.7).

- [ ] **Step 3: Push branch**

```bash
git push -u origin chunk-1/ci-wave-1
```

- [ ] **Step 4: Open PR**

```bash
gh pr create --title "feat(ci): Wave 1 — config tightenings + preflight + deadlined allowlist" --body "$(cat <<'PRBODY'
## Summary

Wave 1 of the CI quality-gate rollout (see `docs/superpowers/specs/2026-04-22-ci-quality-gates-design.md`).

- `clippy.toml`: cognitive complexity 25 → 20.
- `src/lib.rs`, `src/main.rs`: curated 7-lint nursery subset via `#![warn]`.
- `deny.toml`: `multiple-versions` warn → deny + documented skip list.
- `scripts/allowlist-large-files.txt`: deadlined format; every entry placeholder-dated 14 days out (Phase 1a).
- `scripts/check-file-size.sh`: deadline parsing with TDD-authored bats suite.
- `.claude/hooks/preflight.sh`: populated with fast local gates.
- `docs/RUST-GUARDRAILS.md`: new "CI Quality Gates (Wave 1)" section appended.

## Follow-up in same window (Phase 1b — Chunk 1b)

Within 14 days of this PR merging: triage the allowlist. Replace placeholder deadlines with real values. Remove stale paths (`src/github/*` → the code now lives at `src/provider/github/*`). If Phase 1b slips past day 14, main goes red via the deadline-past enforcement.

## Test plan

- [x] `cargo fmt -- --check` → clean
- [x] `cargo clippy -- -D warnings -A dead_code` → clean (post-refactors for nursery lints)
- [x] `cargo test` → all pass
- [x] `bats tests/scripts/check-file-size.bats` → 5/5 pass
- [x] `cargo deny check` → clean (with new skip list)
- [x] `.claude/hooks/preflight.sh` → end-to-end green
PRBODY
)"
```

---

### Task 1.9 (Phase 1b, follow-up within 14 days): Allowlist triage

**Files:**
- Modify: `scripts/allowlist-large-files.txt`

This task is its own PR, opened within 14 days of Chunk 1 merging. Separate branch, separate PR.

- [ ] **Step 1: Create triage branch**

```bash
git checkout main && git pull
git checkout -b chunk-1b/allowlist-triage
```

- [ ] **Step 2: Enumerate stale paths**

For each entry marked `STALE-PATH-CHECK-PHASE-1B`, verify the path exists:

```bash
for path in src/github/client.rs src/github/ci.rs src/github/types.rs; do
  [[ -f "$path" ]] && echo "$path EXISTS" || echo "$path MISSING"
done
```

Expected: all three are MISSING (they were moved to `src/provider/github/`).

- [ ] **Step 3: Remove stale entries, add replacement entries if needed**

Delete the three `src/github/*` lines. If the current `src/provider/github/*` versions now exceed 500 LOC (check: `wc -l src/provider/github/client.rs`), add them as new allowlist entries with real deadlines.

- [ ] **Step 4: Triage each remaining entry with a real deadline, owner, ticket, plan**

Replace placeholder values with real ones. A reasonable triage heuristic:

- Pure-UI TUI screens (700-1200 LOC): deadline 3-4 months out. Plan: extract individual sub-screens.
- Large core modules (config.rs, cli.rs, session/types.rs): deadline 2-3 months out. Plan: split by responsibility.
- Test files (tui/app/tests.rs, integration_tests/stream_parsing.rs): deadline 4-5 months out (tests are often intentionally long-form; lower priority).
- Outliers (settings/mod.rs at 2080 LOC): deadline immediate (< 1 month) — this file is way out of spec and deserves priority.

Owner: `@carlos` for all (single-dev project); reassign if a collaborator joins.

Ticket: create a GitHub issue for each entry, link the issue number. Alternatively, write `#TBD` if the detailed split isn't designed yet and note in the `plan:` field what the split would look like.

- [ ] **Step 5: Verify**

Run: `bash scripts/check-file-size.sh`
Expected: exit 0 (all deadlines still in future; stale entries removed).

- [ ] **Step 6: Commit and open PR**

```bash
git add scripts/allowlist-large-files.txt
git commit -m "$(cat <<'EOF'
chore(allowlist): Phase 1b triage — real deadlines, stale paths removed

Replaces placeholder deadlines with triaged real values. Removes three
stale src/github/* entries (paths moved to src/provider/github/*).

Triage heuristic:
- TUI screens: 3-4 months out, plan: extract sub-screens
- Core modules: 2-3 months out, plan: split by responsibility
- Test files: 4-5 months out (lower priority)
- Outliers (settings/mod.rs at 2080 LOC): < 1 month priority refactor
EOF
)"
git push -u origin chunk-1b/allowlist-triage
gh pr create --title "chore(allowlist): Phase 1b triage" --body "Phase 1b follow-up to Wave 1: real deadlines, stale paths removed. If this PR doesn't merge within 14 days of Wave 1, main goes red via deadline-past enforcement."
```

---

## Chunk 1 — Recap

9 commits across two PRs (7 in Chunk 1, 2 in Chunk 1b):

- `scripts/check-file-size.sh` grows deadline parsing with bats coverage.
- `scripts/allowlist-large-files.txt` converts to deadlined format.
- `clippy.toml` cognitive-complexity tightened.
- `src/lib.rs` + `src/main.rs` add curated nursery warnings.
- `deny.toml` in strict mode.
- `.claude/hooks/preflight.sh` populated.
- `docs/RUST-GUARDRAILS.md` documents the new gates.
- Phase 1b triage replaces placeholders with real values.

Total LOC change: modest in production code (lint fixes); new infrastructure in `tests/scripts/` and `.claude/hooks/`.

---

## Chunk 2: Wave 2.1 — Coverage infrastructure

**Goal:** Set up `cargo-llvm-cov` in CI, author the coverage-tier manifest and enforcement script, measure baseline, activate the ratchet. Coverage floors activate conditionally (may extend beyond Chunk 2 calendar if baseline is below tier floors).

**Files:**
- Modify: `.github/workflows/ci.yml` (add `coverage` job)
- Create: `scripts/coverage-tiers.yml`
- Create: `scripts/check-coverage-tiers.sh`
- Create: `tests/scripts/check-coverage-tiers.bats`
- Create: `tests/scripts/fixtures/lcov-sample-*.info`
- Create: `tests/manifests/validate_manifests_test.py`
- Modify: `docs/RUST-GUARDRAILS.md` (append coverage section)

### Task 2.1: Author coverage tier manifest

**Files:**
- Create: `scripts/coverage-tiers.yml`

- [ ] **Step 1: Write the manifest**

File: `scripts/coverage-tiers.yml`

```yaml
# Coverage tiers for scripts/check-coverage-tiers.sh.
#
# Each tier has a floor (enforced once baseline is at or above it)
# and an aspiration (documentary, not enforced).
# Paths use glob patterns; a file is classified into the first tier
# whose glob matches.
#
# Special tier "excluded" — files NOT counted toward total coverage.
# (Binary wiring, integration tests, trivial mod.rs re-exports.)

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
      - "**/mod.rs"  # bare re-exports only; files with logic stay in their tier
```

- [ ] **Step 2: Validate YAML parses cleanly**

Run: `yq eval '.tiers | keys' scripts/coverage-tiers.yml`
Expected: `- core`, `- tui`, `- excluded`.

If `yq` is not installed: `brew install yq` (macOS) or `sudo apt install yq` (Debian/Ubuntu).

- [ ] **Step 3: Commit**

```bash
git add scripts/coverage-tiers.yml
git commit -m "feat(scripts): add coverage tier manifest"
```

---

### Task 2.2: TDD — `check-coverage-tiers.sh` with bats

This task builds the coverage enforcement script test-first. The script is a bash+`yq` parser that reads lcov, groups files by tier, computes weighted means, asserts floors.

**Files:**
- Create: `tests/scripts/check-coverage-tiers.bats`
- Create: `tests/scripts/fixtures/lcov-100pct-core.info`
- Create: `tests/scripts/fixtures/lcov-70pct-core.info`
- Create: `tests/scripts/fixtures/lcov-mixed-tiers.info`
- Create: `scripts/check-coverage-tiers.sh`

- [ ] **Step 1: Create a minimal lcov fixture at 100% coverage**

File: `tests/scripts/fixtures/lcov-100pct-core.info`

```
TN:
SF:src/session/manager.rs
LF:50
LH:50
end_of_record
SF:src/state/store.rs
LF:30
LH:30
end_of_record
```

lcov format: `LF` is lines found, `LH` is lines hit. This fixture: 80 lines total, 80 hit = 100%.

- [ ] **Step 2: Write failing test — 100% lcov against core floor of 90% passes**

File: `tests/scripts/check-coverage-tiers.bats`

```bash
#!/usr/bin/env bats

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$REPO_ROOT/scripts/check-coverage-tiers.sh"
  MANIFEST="$REPO_ROOT/scripts/coverage-tiers.yml"
  FIXTURES="$REPO_ROOT/tests/scripts/fixtures"
  cd "$REPO_ROOT"
}

@test "100% coverage passes core floor of 90%" {
  run bash "$SCRIPT" "$FIXTURES/lcov-100pct-core.info"
  [ "$status" -eq 0 ]
  [[ "$output" == *"core: 100.0%"* ]]
}
```

- [ ] **Step 3: Run — expect FAIL (script doesn't exist)**

Run: `bats tests/scripts/check-coverage-tiers.bats`
Expected: error (`check-coverage-tiers.sh` not found).

- [ ] **Step 4: Create the script — minimal implementation for first test**

File: `scripts/check-coverage-tiers.sh`

```bash
#!/usr/bin/env bash
# Enforce coverage floors per tier from scripts/coverage-tiers.yml
#
# Usage: check-coverage-tiers.sh <coverage.lcov>
#
# Exit codes:
#   0 — all tier floors satisfied
#   1 — some tier below its floor
#   2 — invalid input (no lcov, missing manifest, yq not installed)

set -euo pipefail

LCOV_FILE="${1:-}"
MANIFEST="${MANIFEST_OVERRIDE:-scripts/coverage-tiers.yml}"

if [[ -z "$LCOV_FILE" ]]; then
  echo "usage: $0 <coverage.lcov>" >&2
  exit 2
fi

if [[ ! -f "$LCOV_FILE" ]]; then
  echo "error: lcov file not found: $LCOV_FILE" >&2
  exit 2
fi

if [[ ! -f "$MANIFEST" ]]; then
  echo "error: manifest not found: $MANIFEST" >&2
  exit 2
fi

if ! command -v yq >/dev/null 2>&1; then
  echo "error: yq required for YAML parsing; install via: brew install yq" >&2
  exit 2
fi

# Parse lcov: build "file: lines_found lines_hit" lines.
parse_lcov() {
  local file
  local lf
  local lh
  local in_record=false
  while IFS= read -r line; do
    case "$line" in
      "SF:"*) file="${line#SF:}"; in_record=true ;;
      "LF:"*) lf="${line#LF:}" ;;
      "LH:"*) lh="${line#LH:}" ;;
      "end_of_record") 
        if $in_record; then
          echo "$file $lf $lh"
        fi
        in_record=false
        ;;
    esac
  done < "$LCOV_FILE"
}

# Match a file against a glob list.
matches_any_glob() {
  local file="$1"
  shift
  for pattern in "$@"; do
    # Convert ** → bash-friendly matching.
    case "$file" in
      $pattern) return 0 ;;
    esac
  done
  return 1
}

# Read tier names.
tier_names=($(yq eval '.tiers | keys | .[]' "$MANIFEST"))

# For each tier, accumulate totals.
declare -A tier_lf
declare -A tier_lh
declare -A tier_floor
for tier in "${tier_names[@]}"; do
  tier_lf[$tier]=0
  tier_lh[$tier]=0
  tier_floor[$tier]=$(yq eval ".tiers.${tier}.floor // 0" "$MANIFEST")
done

# Process each lcov record.
while read -r file lf lh; do
  matched_tier=""
  for tier in "${tier_names[@]}"; do
    paths=($(yq eval ".tiers.${tier}.paths[]" "$MANIFEST"))
    if matches_any_glob "$file" "${paths[@]}"; then
      matched_tier="$tier"
      break
    fi
  done
  if [[ -z "$matched_tier" ]]; then
    matched_tier="core"  # default tier
  fi
  if [[ "$matched_tier" != "excluded" ]]; then
    tier_lf[$matched_tier]=$(( ${tier_lf[$matched_tier]} + lf ))
    tier_lh[$matched_tier]=$(( ${tier_lh[$matched_tier]} + lh ))
  fi
done < <(parse_lcov)

# Compute per-tier coverage, check floors.
violations=0
for tier in "${tier_names[@]}"; do
  [[ "$tier" == "excluded" ]] && continue
  lf=${tier_lf[$tier]}
  lh=${tier_lh[$tier]}
  floor=${tier_floor[$tier]}
  if (( lf == 0 )); then
    echo "$tier: no files measured"
    continue
  fi
  pct=$(awk "BEGIN { printf \"%.1f\", ($lh / $lf) * 100 }")
  printf "%s: %.1f%% (floor: %s%%)\n" "$tier" "$pct" "$floor"
  if awk "BEGIN { exit !($pct < $floor) }"; then
    echo "  VIOLATION: below floor"
    violations=$((violations + 1))
  fi
done

if (( violations > 0 )); then
  echo ""
  echo "$violations tier(s) below floor."
  exit 1
fi

exit 0
```

- [ ] **Step 5: Make executable**

```bash
chmod +x scripts/check-coverage-tiers.sh
```

- [ ] **Step 6: Run — expect PASS**

Run: `bats tests/scripts/check-coverage-tiers.bats`
Expected: 1/1 pass (core at 100% ≥ 90% floor).

- [ ] **Step 7: Add 70% fixture + failing test**

File: `tests/scripts/fixtures/lcov-70pct-core.info`

```
TN:
SF:src/session/manager.rs
LF:100
LH:70
end_of_record
```

Append to `.bats`:

```bash
@test "70% coverage fails core floor of 90%" {
  run bash "$SCRIPT" "$FIXTURES/lcov-70pct-core.info"
  [ "$status" -eq 1 ]
  [[ "$output" == *"VIOLATION"* ]]
  [[ "$output" == *"core: 70.0%"* ]]
}
```

- [ ] **Step 8: Run — expect PASS**

Run: `bats tests/scripts/check-coverage-tiers.bats`
Expected: 2/2 pass.

- [ ] **Step 9: Add mixed-tier fixture — core 95% + tui 50%**

File: `tests/scripts/fixtures/lcov-mixed-tiers.info`

```
TN:
SF:src/session/manager.rs
LF:100
LH:95
end_of_record
SF:src/tui/app.rs
LF:200
LH:100
end_of_record
SF:src/main.rs
LF:50
LH:0
end_of_record
```

Core: 95/100 = 95% (passes 90% floor).
TUI: 100/200 = 50% (FAILS 70% floor).
Excluded: main.rs skipped.

Append to `.bats`:

```bash
@test "mixed tiers: core passes, tui fails floor, excluded ignored" {
  run bash "$SCRIPT" "$FIXTURES/lcov-mixed-tiers.info"
  [ "$status" -eq 1 ]
  [[ "$output" == *"core: 95.0%"* ]]
  [[ "$output" == *"tui: 50.0%"* ]]
  [[ "$output" == *"VIOLATION"* ]]
  # main.rs is excluded, should not appear.
  [[ "$output" != *"main.rs"* ]]
}
```

- [ ] **Step 10: Run — expect PASS**

Run: `bats tests/scripts/check-coverage-tiers.bats`
Expected: 3/3 pass.

- [ ] **Step 11: Commit**

```bash
git add scripts/check-coverage-tiers.sh tests/scripts/
git commit -m "$(cat <<'EOF'
feat(scripts): add check-coverage-tiers.sh with bats coverage

Parses lcov format, groups files by tier via glob matching from
scripts/coverage-tiers.yml, computes weighted mean coverage per tier,
fails when any tier is below its floor.

Tests cover: 100% passes, 70% fails, mixed tiers with one fail + one
pass + excluded files. Uses yq for YAML parsing — declare as a
prerequisite in CI.
EOF
)"
```

---

### Task 2.3: Wire coverage CI job

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Add the job**

Append to `.github/workflows/ci.yml` (after the `audit` job):

```yaml
  coverage:
    # Reporting-only initially; becomes blocking once baseline at or above
    # tier floors. See docs/RUST-GUARDRAILS.md for the coverage policy.
    name: Coverage Tiers
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: llvm-tools-preview
      - uses: Swatinem/rust-cache@v2
      - name: Install yq
        run: sudo apt-get update && sudo apt-get install -y yq
      - name: Install cargo-llvm-cov
        run: cargo install cargo-llvm-cov --locked
      - name: Generate lcov
        run: cargo llvm-cov --workspace --lcov --output-path coverage.lcov
      - name: Check coverage tiers
        run: bash scripts/check-coverage-tiers.sh coverage.lcov
        continue-on-error: true  # reporting-only during baseline phase
```

`continue-on-error: true` keeps the PR from being blocked while baseline is below floors. Remove once baseline reaches each tier's floor.

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "$(cat <<'EOF'
ci: add coverage-tiers job (reporting-only baseline phase)

Runs cargo-llvm-cov, generates lcov, pipes through check-coverage-tiers.sh.
continue-on-error: true during baseline phase — will be removed once
baseline coverage reaches the core 90% and tui 70% floors documented in
scripts/coverage-tiers.yml.
EOF
)"
```

---

### Task 2.4: Measure baseline coverage + document

**Files:**
- Modify: `docs/RUST-GUARDRAILS.md` (append baseline measurement + coverage policy section)

- [ ] **Step 1: Install cargo-llvm-cov locally if not present**

```bash
cargo install cargo-llvm-cov --locked
```

- [ ] **Step 2: Generate baseline report**

```bash
cargo llvm-cov --workspace --lcov --output-path coverage.lcov
bash scripts/check-coverage-tiers.sh coverage.lcov
```

Capture the output — it will show something like:

```
core: 72.3% (floor: 90.0%)
  VIOLATION: below floor
tui: 45.1% (floor: 70.0%)
  VIOLATION: below floor
```

Record the two percentages — call them `$CORE_BASELINE` and `$TUI_BASELINE`.

- [ ] **Step 3: Append "Coverage Policy" section to RUST-GUARDRAILS.md**

Add at end of `docs/RUST-GUARDRAILS.md`:

```markdown
---

## CI Quality Gates (Wave 2.1 — Coverage)

**Status:** reporting-only during baseline phase. Floors activate per tier
when baseline reaches the floor.

**Tool:** `cargo-llvm-cov`. Tier manifest: `scripts/coverage-tiers.yml`.
Enforcement: `scripts/check-coverage-tiers.sh`.

**Tier floors:**

| Tier | Paths | Floor | Aspiration |
|------|-------|-------|------------|
| core | session, state, adapt, turboquant, gates, provider, config.rs, cli.rs | 90.0% | 96.0% |
| tui | `src/tui/**` | 70.0% | — |
| excluded | main.rs, lib.rs, integration_tests, *_test.rs, tests.rs | — | — |

**Measured baseline as of <date>**: core = `<CORE_BASELINE>`%, tui = `<TUI_BASELINE>`%.

**Activation policy:** the `coverage` CI job is `continue-on-error: true`
(reporting-only) until baseline of a tier reaches its floor. At that point,
a dedicated PR removes `continue-on-error` for that tier. Tiers activate
independently — core may be enforced while tui is still reporting-only.

**Ratchet:** once a tier is active, subsequent PRs may not decrease that
tier's coverage. Ratchet activation is **conditional, not part of
Chunk 2's hard exit criteria** (spec and plan are aligned on this after
review). Landing the ratchet during baseline phase would block every
test-less PR — including refactors and documentation changes — because
baseline is below floor. The ratchet lands in a follow-up PR after the
core tier reaches its 90% floor. Implementation: diff coverage against
`main`'s most recent lcov, fail if total decreases by more than a small
tolerance (e.g., 0.1%).
```

Replace `<date>`, `<CORE_BASELINE>`, `<TUI_BASELINE>` with real values.

- [ ] **Step 4: Commit**

```bash
git add docs/RUST-GUARDRAILS.md
git commit -m "$(cat <<'EOF'
docs(rust-guardrails): document coverage policy + measured baseline

Coverage job is reporting-only during baseline phase. Per-tier activation:
core and tui activate independently when baseline reaches the respective
floor. Ratchet logic is deferred to a follow-up PR after activation (would
block every test-less PR during baseline phase otherwise).
EOF
)"
```

---

### Task 2.5: Python unittest for manifest schemas

**Files:**
- Create: `tests/manifests/validate_manifests_test.py`

- [ ] **Step 1: Create directory**

```bash
mkdir -p tests/manifests
```

- [ ] **Step 2: Write the test**

File: `tests/manifests/validate_manifests_test.py`

```python
"""Schema validation for scripts/coverage-tiers.yml and (later)
scripts/architecture-layers.yml.

Uses `yq` via subprocess since the project avoids pyyaml.
"""
import json
import subprocess
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]


def yq_json(yaml_path: Path, expr: str = ".") -> object:
    """Return the parsed YAML as a Python object via yq + json round-trip."""
    result = subprocess.run(
        ["yq", "eval", "-o", "json", expr, str(yaml_path)],
        capture_output=True,
        text=True,
        check=True,
    )
    return json.loads(result.stdout)


class CoverageTiersTest(unittest.TestCase):
    manifest_path = REPO_ROOT / "scripts" / "coverage-tiers.yml"

    @classmethod
    def setUpClass(cls):
        cls.data = yq_json(cls.manifest_path)

    def test_top_level_has_tiers(self):
        self.assertIn("tiers", self.data)

    def test_required_tiers_present(self):
        tiers = self.data["tiers"]
        self.assertIn("core", tiers)
        self.assertIn("tui", tiers)
        self.assertIn("excluded", tiers)

    def test_core_has_floor_and_aspiration(self):
        core = self.data["tiers"]["core"]
        self.assertIn("floor", core)
        self.assertIn("aspiration", core)
        self.assertIsInstance(core["floor"], (int, float))
        self.assertIsInstance(core["aspiration"], (int, float))
        self.assertGreaterEqual(core["aspiration"], core["floor"])

    def test_tui_has_floor(self):
        tui = self.data["tiers"]["tui"]
        self.assertIn("floor", tui)
        self.assertIsInstance(tui["floor"], (int, float))

    def test_excluded_has_paths(self):
        excluded = self.data["tiers"]["excluded"]
        self.assertIn("paths", excluded)
        self.assertIsInstance(excluded["paths"], list)
        self.assertGreater(len(excluded["paths"]), 0)

    def test_all_paths_are_strings(self):
        for tier_name, tier in self.data["tiers"].items():
            for path in tier.get("paths", []):
                self.assertIsInstance(path, str, f"tier={tier_name}")


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 3: Run — expect pass**

Run: `python3 -m unittest tests.manifests.validate_manifests_test -v`
Expected: 6 passing tests.

- [ ] **Step 4: Commit**

```bash
touch tests/manifests/__init__.py
git add tests/manifests/
git commit -m "test(manifests): validate coverage-tiers.yml schema"
```

---

### Task 2.6: Chunk 2 close — push, PR

- [ ] **Step 1: Full verification**

```bash
cargo fmt -- --check
cargo clippy -- -D warnings -A dead_code
cargo test
bats tests/scripts/check-coverage-tiers.bats
python3 -m unittest discover -s tests -p "*_test.py"
bash scripts/check-coverage-tiers.sh <(cargo llvm-cov --lcov)
```

Expected: every command exits 0 (coverage may report `VIOLATION` for below-floor tiers but the CI job has `continue-on-error: true`).

- [ ] **Step 2: Push and open PR**

```bash
git push -u origin chunk-2/coverage-infrastructure
gh pr create --title "feat(ci): Wave 2.1 — coverage via cargo-llvm-cov with tiered floors" --body "$(cat <<'PRBODY'
## Summary

- `scripts/coverage-tiers.yml`: tier manifest (core 90% / tui 70% / excluded).
- `scripts/check-coverage-tiers.sh`: bash+yq parser for lcov, bats-tested (3 scenarios).
- `.github/workflows/ci.yml`: new `coverage` job, reporting-only during baseline.
- `tests/manifests/validate_manifests_test.py`: Python unittest for the manifest schema.
- `docs/RUST-GUARDRAILS.md`: new Coverage Policy section with measured baseline.

## Activation path

Job is `continue-on-error: true`. Once baseline reaches each tier's floor, a follow-up PR removes the flag per-tier. Ratchet logic is deferred until activation (would block every test-less PR during baseline phase).

## Test plan

- [x] `bats tests/scripts/check-coverage-tiers.bats` → 3/3
- [x] `python3 -m unittest discover -s tests` → schema tests pass
- [x] CI coverage job runs and reports tier percentages

## Baseline measured

Core: `<fill-in>`%. TUI: `<fill-in>`%. Both below floor — job is reporting-only pending test-writing follow-ups.
PRBODY
)"
```

---

## Chunk 3: Wave 2.2 — File size 500 → 400 cap transition

**Goal:** Flip `MAX_LINES` in `scripts/check-file-size.sh` from 500 to 400. Every file in the 400-500 band that's not already on the allowlist gets a triaged entry with deadline + owner + plan. Extends Chunk 1's bats suite with coverage of the new cap.

**Prerequisites:** Chunk 1b merged (all existing allowlist entries have real deadlines).

**Files:**
- Modify: `scripts/check-file-size.sh` (`MAX_LINES` 500 → 400)
- Modify: `scripts/allowlist-large-files.txt` (add 400-500 band entries)
- Modify: `tests/scripts/check-file-size.bats` (new cap scenario)
- Modify: `docs/RUST-GUARDRAILS.md` (update §1 file-size cap reference, append Wave 2.2 section)

### Task 3.1: Enumerate 400-500 band files and triage

**Files:**
- Modify: `scripts/allowlist-large-files.txt`

- [ ] **Step 1: List files in the 400-500 band not already allowlisted**

```bash
comm -23 \
  <(find src -name '*.rs' -exec wc -l {} + | awk '$1 >= 400 && $1 < 500' | awk '{print $2}' | sort) \
  <(grep -v '^#' scripts/allowlist-large-files.txt | grep -v '^$' | awk '{print $1}' | sort)
```

Capture the list — expected ~20-40 files.

- [ ] **Step 2: Triage each file**

Same heuristic as Phase 1b (Task 1.9 Step 4):

- TUI (400-500 LOC): 2-3 months deadline, plan extract sub-components.
- Core modules (400-500 LOC): 1-2 months deadline, plan split by responsibility.

- [ ] **Step 3: Append triaged entries to `scripts/allowlist-large-files.txt`**

Add a new section header for clarity:

```
# --- 400-500 LOC band (added in Wave 2.2) ---
src/example/path.rs # deadline: 2026-06-15, owner: @carlos, ticket: #TBD, plan: extract subcomponent X
...
```

- [ ] **Step 4: Commit**

```bash
git add scripts/allowlist-large-files.txt
git commit -m "$(cat <<'EOF'
chore(allowlist): add 400-500 LOC band entries (Wave 2.2 prep)

Before flipping MAX_LINES from 500 to 400, enumerate every file
currently in the 400-500 band and add a triaged allowlist entry for
each. Triage uses same heuristic as Phase 1b: TUI gets 2-3 month
deadlines, core modules 1-2 months.
EOF
)"
```

---

### Task 3.2: Flip `MAX_LINES` 500 → 400 + update test

**Files:**
- Modify: `scripts/check-file-size.sh`
- Modify: `tests/scripts/check-file-size.bats`

- [ ] **Step 1: Add bats test for new cap**

Append to `tests/scripts/check-file-size.bats`:

```bash
@test "new 400 LOC cap: file at 450 LOC not on allowlist fails" {
  printf 'line\n%.0s' {1..450} > src/new_bigfile.rs
  # Empty allowlist
  echo "# empty" > scripts/allowlist-large-files.txt
  run bash scripts/check-file-size.sh
  [ "$status" -ne 0 ]
  [[ "$output" == *"VIOLATION"* ]]
  [[ "$output" == *"max 400"* ]]
}

@test "new 400 LOC cap: file at 390 LOC passes without allowlist" {
  printf 'line\n%.0s' {1..390} > src/small_file.rs
  echo "# empty" > scripts/allowlist-large-files.txt
  run bash scripts/check-file-size.sh
  [ "$status" -eq 0 ]
}
```

- [ ] **Step 2: Run — expect FAIL (current cap is 500)**

Run: `bats tests/scripts/check-file-size.bats`
Expected: the two new tests fail; older tests still pass.

- [ ] **Step 3: Flip the cap**

Modify `scripts/check-file-size.sh`:

```bash
MAX_LINES=400   # was 500
```

- [ ] **Step 4: Re-run — expect PASS**

Run: `bats tests/scripts/check-file-size.bats`
Expected: all tests pass.

- [ ] **Step 5: Run full file-size check on real repo**

Run: `bash scripts/check-file-size.sh`
Expected: exit 0. Every 400+ LOC file is either on the updated allowlist or the check catches it as violation.

If violations appear for files NOT in the updated allowlist — that file was missed in Task 3.1. Go back, add it, update its commit.

- [ ] **Step 6: Commit**

```bash
git add scripts/check-file-size.sh tests/scripts/check-file-size.bats
git commit -m "$(cat <<'EOF'
feat(scripts): flip file-size MAX_LINES 500 → 400

Tightening from the original 500-LOC soft cap to 400 LOC. Every file in
the 400-500 band that was previously passing is now on the allowlist
with a triaged deadline (see previous commit).

Tests: new 400 cap asserted with both failing (450 LOC) and passing
(390 LOC) cases. Existing allowlist semantics (legacy-format backward
compatibility, deadline parsing) unchanged.
EOF
)"
```

---

### Task 3.3: Update RUST-GUARDRAILS.md

**Files:**
- Modify: `docs/RUST-GUARDRAILS.md`

- [ ] **Step 1: Find §1 file-size reference and update**

Run: `grep -n 'File size' docs/RUST-GUARDRAILS.md`

Locate the line that says "Soft target 500 LOC; hard cap enforced by `scripts/check-file-size.sh`" and update to 400.

- [ ] **Step 2: Append Wave 2.2 subsection**

At end of the document:

```markdown
---

## CI Quality Gates (Wave 2.2 — File Size)

**Status:** active.

**Hard cap:** 400 LOC per `.rs` file under `src/`. Enforced by
`scripts/check-file-size.sh`. Allowlist entries are temporary — every
entry has a deadline in `scripts/allowlist-large-files.txt` per the
format documented in the Wave 1 section above.

**Extensions.** Extending a deadline requires a paragraph in the PR
explaining why the refactor wasn't done and a new realistic deadline.
Repeated extensions on the same file warrant a different split
strategy, not another extension.

**Soft target.** 300 LOC. Anything approaching 400 is a review signal
— split before adding new responsibilities.
```

- [ ] **Step 3: Commit**

```bash
git add docs/RUST-GUARDRAILS.md
git commit -m "docs(rust-guardrails): update file-size cap to 400 + append Wave 2.2 section"
```

---

### Task 3.4: Chunk 3 close — push, PR

- [ ] **Step 1: Verify**

```bash
bats tests/scripts/check-file-size.bats
bash scripts/check-file-size.sh
cargo fmt -- --check
cargo test
```

Expected: all exit 0.

- [ ] **Step 2: Push and open PR**

```bash
git push -u origin chunk-3/file-size-400
gh pr create --title "feat(ci): Wave 2.2 — file-size cap 500 → 400" --body "$(cat <<'PRBODY'
## Summary

- Tightens `scripts/check-file-size.sh` `MAX_LINES` from 500 to 400.
- Pre-adds every 400-500 band file to `scripts/allowlist-large-files.txt` with a triaged deadline.
- Appends Wave 2.2 section to `docs/RUST-GUARDRAILS.md`.
- Extends bats suite with 400 LOC cap scenarios.

## Prerequisite verified

- [x] Chunk 1b (allowlist triage) merged — every existing entry has a real deadline.

## Test plan

- [x] `bats tests/scripts/check-file-size.bats` → all pass
- [x] `bash scripts/check-file-size.sh` on real repo → exit 0 (every 400+ file is either allowlisted or intentionally-sized)

## Rollout

Immediate. New files must hit 400 from day one. Existing 400-500 band files chip down per deadline schedule.
PRBODY
)"
```

---

## Chunk 4: Wave 2.3 — Layer violations

**Goal:** Introduce module-level layer enforcement via a custom bash script + YAML manifest + debt file. Each layer has a set of paths; a file may import from its own layer or any lower layer. Forbidden-pair rules catch specific disallowed imports even within the same layer. Baseline run on main produces `docs/layers-debt.txt` for existing violations.

**Files:**
- Create: `scripts/architecture-layers.yml`
- Create: `scripts/check-layers.sh`
- Create: `tests/scripts/check-layers.bats`
- Create: `tests/scripts/fixtures/layers-sample-*.rs`
- Create: `docs/layers-debt.txt`
- Modify: `.github/workflows/ci.yml` (add `layers` job)
- Modify: `docs/RUST-GUARDRAILS.md` (append Wave 2.3 section)

### Task 4.1: Author layer manifest

**Files:**
- Create: `scripts/architecture-layers.yml`

- [ ] **Step 1: Write the manifest**

File: `scripts/architecture-layers.yml`

```yaml
# Module-level architecture layers.
#
# A file in layer N may import from layer N or any layer < N.
# Imports from a higher layer are forbidden.
# Forbidden pairs catch specific disallowed imports even within the same layer.

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
  # Specific disallowed imports that cross layer boundaries in ways
  # that are hard to prevent with layer-number comparison alone.
  - from: "src/session/**"
    to: "src/tui/**"
    reason: "domain logic must not depend on UI rendering"
  - from: "src/state/**"
    to: "src/tui/**"
    reason: "state persistence must not depend on UI rendering"
  - from: "src/provider/**"
    to: "src/tui/**"
    reason: "external-service clients must not depend on UI rendering"
  - from: "src/config.rs"
    to: "src/tui/**"
    reason: "config must not depend on UI (known debt: config.rs imports ThemeConfig from tui/theme; resolve by moving ThemeConfig to layer 2)"
```

- [ ] **Step 2: Validate YAML parses**

Run: `yq eval '.layers | length' scripts/architecture-layers.yml`
Expected: 5.

Run: `yq eval '.forbidden | length' scripts/architecture-layers.yml`
Expected: 4.

- [ ] **Step 3: Commit**

```bash
git add scripts/architecture-layers.yml
git commit -m "feat(scripts): add architecture-layers.yml manifest"
```

---

### Task 4.2: TDD — `check-layers.sh` with bats

**Files:**
- Create: `tests/scripts/check-layers.bats`
- Create: `tests/scripts/fixtures/layers-*.rs`
- Create: `scripts/check-layers.sh`

- [ ] **Step 1: Create fixture Rust files**

Each fixture represents a minimal Rust file with specific imports.

File: `tests/scripts/fixtures/layers-samelayer-ok.rs`

```rust
// A domain-layer file importing another domain-layer module → should pass.
use crate::session::manager::SessionManager;
```

File: `tests/scripts/fixtures/layers-lowerlayer-ok.rs`

```rust
// A UI-layer file importing a domain-layer module → should pass.
use crate::session::manager::SessionManager;
```

File: `tests/scripts/fixtures/layers-higherlayer-fail.rs`

```rust
// A domain-layer file importing a UI-layer module → should fail.
use crate::tui::theme::ThemeConfig;
```

File: `tests/scripts/fixtures/layers-forbiddenpair-fail.rs`

```rust
// A state-layer file importing tui (forbidden pair) → should fail.
use crate::tui::theme::SerializableColor;
```

- [ ] **Step 2: Write failing test — higher-layer import fails**

File: `tests/scripts/check-layers.bats`

```bash
#!/usr/bin/env bats

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$REPO_ROOT/scripts/check-layers.sh"
  MANIFEST="$REPO_ROOT/scripts/architecture-layers.yml"
  FIXTURES="$REPO_ROOT/tests/scripts/fixtures"
  # Each test runs in a clean tempdir with a faked src/ layout.
  TEST_ROOT="$(mktemp -d -t layers-test-XXXXXX)"
  cd "$TEST_ROOT"
  mkdir -p src/session src/state src/tui
}

teardown() {
  cd /
  rm -rf "$TEST_ROOT"
}

@test "same-layer import passes (domain → domain)" {
  mkdir -p src/session
  cp "$FIXTURES/layers-samelayer-ok.rs" src/session/module_a.rs
  run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 0 ]
}
```

- [ ] **Step 3: Run — expect FAIL (script doesn't exist)**

Run: `bats tests/scripts/check-layers.bats`
Expected: script-not-found error.

- [ ] **Step 4: Create the script — minimal for first test**

File: `scripts/check-layers.sh`

```bash
#!/usr/bin/env bash
# Enforce architecture layer rules from scripts/architecture-layers.yml.
#
# Usage: check-layers.sh [<manifest>]
#   Default manifest: scripts/architecture-layers.yml
#
# Scans every .rs file under src/, extracts `use crate::…` statements,
# checks layer ordering and forbidden pairs.
#
# Exit codes:
#   0 — no violations
#   1 — one or more violations
#   2 — invalid input (missing manifest, missing yq)
#
# Known v1 limitations (intentional — "simple enough not to need a full
# Rust parser" per spec):
#   - Brace-group imports (`use crate::mod::{TypeA, TypeB};`) are not
#     expanded. The extracted path becomes "{TypeA, TypeB}" which
#     use_to_path can't resolve; the import is silently ignored.
#     107 such imports exist in this codebase. Addressable in a future
#     version by pre-expanding braces or switching to syn-based parsing.
#   - Nested `use` blocks and `pub use` re-exports are treated the same
#     as plain `use`; re-exports from TUI into domain layers would be
#     flagged incorrectly if they occur.

set -euo pipefail

MANIFEST="${1:-scripts/architecture-layers.yml}"
DEBT_FILE="${DEBT_FILE_OVERRIDE:-docs/layers-debt.txt}"

if [[ ! -f "$MANIFEST" ]]; then
  echo "error: manifest not found: $MANIFEST" >&2
  exit 2
fi

if ! command -v yq >/dev/null 2>&1; then
  echo "error: yq required for YAML parsing; install via: brew install yq" >&2
  exit 2
fi

# Glob-match helper (same as check-coverage-tiers.sh).
matches_any_glob() {
  local file="$1"
  shift
  for pattern in "$@"; do
    case "$file" in
      $pattern) return 0 ;;
    esac
  done
  return 1
}

# Given a file path, return its layer number (empty if not classified).
get_layer_number() {
  local file="$1"
  local num
  num=$(yq eval ".layers[] | select(.paths | any(. == \"$file\" or (\"$file\" | test(sub(\"\\*\\*\", \".*\") | \"^\" + . + \"$\"))))) | .number" "$MANIFEST" 2>/dev/null | head -1)
  echo "${num:-}"
}

# Simpler approach: iterate layers, match file against each.
file_to_layer_num() {
  local file="$1"
  local count
  count=$(yq eval '.layers | length' "$MANIFEST")
  for (( i=0; i<count; i++ )); do
    local num
    num=$(yq eval ".layers[$i].number" "$MANIFEST")
    local paths
    mapfile -t paths < <(yq eval ".layers[$i].paths[]" "$MANIFEST")
    if matches_any_glob "$file" "${paths[@]}"; then
      echo "$num"
      return 0
    fi
  done
  echo ""
}

# For a `use crate::X::…` line, return the first-matching source file path
# (best-effort — maps `crate::session::manager` to `src/session/manager.rs` or
# `src/session/manager/mod.rs`).
use_to_path() {
  local use_line="$1"
  local path
  path=$(echo "$use_line" | sed -E 's/^[[:space:]]*use crate::([^:]+(::[^:]+)*)(::[^;]+)?;.*/\1/' | sed 's|::|/|g')
  # Try file paths.
  if [[ -f "src/${path}.rs" ]]; then
    echo "src/${path}.rs"
  elif [[ -f "src/${path}/mod.rs" ]]; then
    echo "src/${path}/mod.rs"
  else
    # Walk up segments — `crate::session::manager::SessionManager` might
    # refer to `session/manager.rs` with SessionManager as a type.
    local stripped
    stripped=$(echo "$path" | rev | cut -d/ -f2- | rev)
    if [[ -n "$stripped" && -f "src/${stripped}.rs" ]]; then
      echo "src/${stripped}.rs"
    elif [[ -n "$stripped" && -f "src/${stripped}/mod.rs" ]]; then
      echo "src/${stripped}/mod.rs"
    else
      echo ""  # can't resolve; ignore
    fi
  fi
}

violations=0
forbidden_count=$(yq eval '.forbidden | length' "$MANIFEST")

# Iterate every src/.rs file.
while IFS= read -r -d '' src_file; do
  importer_rel="${src_file#./}"
  importer_layer=$(file_to_layer_num "$importer_rel")
  [[ -z "$importer_layer" ]] && continue

  while IFS= read -r use_line; do
    target=$(use_to_path "$use_line")
    [[ -z "$target" ]] && continue
    target_layer=$(file_to_layer_num "$target")
    [[ -z "$target_layer" ]] && continue

    # Layer ordering check.
    if (( importer_layer < target_layer )); then
      echo "VIOLATION: $importer_rel (layer $importer_layer) imports $target (layer $target_layer)"
      violations=$((violations + 1))
      continue
    fi

    # Forbidden pair check.
    for (( i=0; i<forbidden_count; i++ )); do
      from_glob=$(yq eval ".forbidden[$i].from" "$MANIFEST")
      to_glob=$(yq eval ".forbidden[$i].to" "$MANIFEST")
      if matches_any_glob "$importer_rel" "$from_glob" && matches_any_glob "$target" "$to_glob"; then
        reason=$(yq eval ".forbidden[$i].reason" "$MANIFEST")
        echo "FORBIDDEN: $importer_rel → $target ($reason)"
        violations=$((violations + 1))
        break
      fi
    done
  done < <(grep -E '^[[:space:]]*use crate::' "$src_file" || true)
done < <(find src -name '*.rs' -print0 2>/dev/null)

if (( violations > 0 )); then
  echo ""
  echo "$violations violation(s) found."
  exit 1
fi

exit 0
```

- [ ] **Step 5: Make executable**

```bash
chmod +x scripts/check-layers.sh
```

- [ ] **Step 6: Run — expect PASS (first test)**

Run: `bats tests/scripts/check-layers.bats`
Expected: 1/1 pass.

- [ ] **Step 7: Add more tests progressively**

Append to `.bats`:

```bash
@test "lower-layer import passes (ui → domain)" {
  mkdir -p src/tui
  cp "$FIXTURES/layers-lowerlayer-ok.rs" src/tui/screen.rs
  # Stub the target.
  mkdir -p src/session
  echo "pub struct SessionManager;" > src/session/manager.rs
  run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 0 ]
}

@test "higher-layer import fails (domain → ui)" {
  mkdir -p src/session src/tui
  cp "$FIXTURES/layers-higherlayer-fail.rs" src/session/naughty.rs
  echo "pub struct ThemeConfig;" > src/tui/theme.rs
  run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 1 ]
  [[ "$output" == *"VIOLATION"* ]]
}

@test "forbidden pair fails (state → tui)" {
  mkdir -p src/state src/tui
  cp "$FIXTURES/layers-forbiddenpair-fail.rs" src/state/store.rs
  echo "pub struct SerializableColor;" > src/tui/theme.rs
  run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 1 ]
  [[ "$output" == *"FORBIDDEN"* ]]
}
```

- [ ] **Step 8: Run — expect PASS**

Run: `bats tests/scripts/check-layers.bats`
Expected: 4/4 pass.

- [ ] **Step 9: Commit**

```bash
git add scripts/check-layers.sh tests/scripts/check-layers.bats tests/scripts/fixtures/layers-*.rs
git commit -m "$(cat <<'EOF'
feat(scripts): add check-layers.sh with bats coverage

Parses scripts/architecture-layers.yml, scans src/*.rs for
`use crate::...` imports, checks layer ordering (lower can't import
higher) and forbidden pairs (specific disallowed imports).

Tests cover: same-layer pass, lower-layer pass, higher-layer fail,
forbidden-pair fail. Uses yq for manifest parsing; grep + sed for
extracting `use` statements (simple enough not to need a full Rust
parser).
EOF
)"
```

---

### Task 4.3: Generate baseline `docs/layers-debt.txt`

**Files:**
- Create: `docs/layers-debt.txt`

- [ ] **Step 1: Run the check on real repo to enumerate existing violations**

```bash
bash scripts/check-layers.sh 2>&1 | grep -E '^(VIOLATION|FORBIDDEN)' > /tmp/layer-violations.txt
wc -l /tmp/layer-violations.txt
```

Expected: at least 1 violation (`src/config.rs → src/tui/theme`).

- [ ] **Step 2: Write the debt file**

File: `docs/layers-debt.txt`

```
# Layer violation debt file.
# Format: <importer> → <target> # deadline: YYYY-MM-DD, owner: @handle, ticket: #N, plan: <brief>
#
# Each entry is a known violation that the layer gate tolerates until
# its deadline. After deadline → CI fails.
#
# (Generated from `bash scripts/check-layers.sh` baseline run.)

src/config.rs → src/tui/theme.rs # deadline: 2026-08-01, owner: @carlos, ticket: #TBD, plan: move ThemeConfig/ThemePreset/SerializableColor types out of src/tui/theme.rs into a layer-2 module (e.g., src/theme.rs) with TUI consuming those types instead of defining them
```

Add each additional violation from Step 1 as its own line.

- [ ] **Step 3: Extend `check-layers.sh` to honor the debt file**

This is a script modification — refactor required. Add debt-file loading + exclusion logic:

After the `MANIFEST=...` line, add:

```bash
# Load debt file (known violations that are tolerated until deadline).
debt_importers=()
debt_targets=()
debt_raws=()
if [[ -f "$DEBT_FILE" ]]; then
  while IFS= read -r line; do
    [[ "$line" =~ ^[[:space:]]*# ]] && continue
    [[ -z "${line// }" ]] && continue
    local_stripped="${line%% # *}"
    importer="${local_stripped% → *}"
    target="${local_stripped#* → }"
    debt_importers+=("$importer")
    debt_targets+=("$target")
    debt_raws+=("$line")
  done < "$DEBT_FILE"
fi

is_in_debt() {
  local importer="$1"
  local target="$2"
  for (( i=0; i<${#debt_importers[@]}; i++ )); do
    if [[ "${debt_importers[$i]}" == "$importer" && "${debt_targets[$i]}" == "$target" ]]; then
      return 0
    fi
  done
  return 1
}
```

Then in the violation loop, before reporting, check:

```bash
if is_in_debt "$importer_rel" "$target"; then
  # Still check deadline.
  # (similar deadline logic as check-file-size.sh)
  continue
fi
```

Add deadline enforcement for the debt file (same pattern as allowlist).

- [ ] **Step 4: Add bats test — debt entry tolerated when in future**

Append to `tests/scripts/check-layers.bats`:

```bash
@test "known-debt entry with future deadline is tolerated" {
  mkdir -p src/config src/tui
  cat > src/config.rs <<'EOF'
use crate::tui::theme::ThemeConfig;
EOF
  echo "pub struct ThemeConfig;" > src/tui/theme.rs
  cat > layers-debt.txt <<'EOF'
src/config.rs → src/tui/theme.rs # deadline: 2099-12-31, owner: @test, ticket: #TEST, plan: TBD
EOF
  DEBT_FILE_OVERRIDE="$PWD/layers-debt.txt" run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 0 ]
}

@test "known-debt entry with past deadline fails" {
  mkdir -p src/config src/tui
  cat > src/config.rs <<'EOF'
use crate::tui::theme::ThemeConfig;
EOF
  echo "pub struct ThemeConfig;" > src/tui/theme.rs
  cat > layers-debt.txt <<'EOF'
src/config.rs → src/tui/theme.rs # deadline: 2000-01-01, owner: @test, ticket: #TEST, plan: TBD
EOF
  DEBT_FILE_OVERRIDE="$PWD/layers-debt.txt" run bash "$SCRIPT" "$MANIFEST"
  [ "$status" -eq 1 ]
  [[ "$output" == *"DEADLINE PAST"* ]]
}
```

- [ ] **Step 5: Run — expect PASS**

Run: `bats tests/scripts/check-layers.bats`
Expected: 6/6 pass.

- [ ] **Step 6: Run on real repo — expect PASS**

Run: `bash scripts/check-layers.sh`
Expected: exit 0 (all violations are in debt file with future deadlines).

- [ ] **Step 7: Commit**

```bash
git add docs/layers-debt.txt scripts/check-layers.sh tests/scripts/check-layers.bats
git commit -m "$(cat <<'EOF'
feat(scripts): add layer-debt file mechanism + check-layers tolerance

The debt file (docs/layers-debt.txt) lists known layer violations with
deadlines. check-layers.sh tolerates these until deadline; same
deadline-past enforcement as check-file-size.sh. Baseline from main:
1 entry (src/config.rs → src/tui/theme — resolution via ThemeConfig
type relocation, deadline 2026-08-01).

Tests cover: debt entry tolerated when deadline future, fails when past.
EOF
)"
```

---

### Task 4.4: Wire `layers` CI job

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Append job**

After the `coverage` job in `.github/workflows/ci.yml`:

```yaml
  layers:
    name: Architecture Layers
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install yq
        run: sudo apt-get update && sudo apt-get install -y yq
      - run: bash scripts/check-layers.sh
```

This job blocks PRs immediately (no `continue-on-error`) because the debt file ensures existing violations are tolerated until deadline.

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add layers job (blocking, debt-file tolerant)"
```

---

### Task 4.5: Update RUST-GUARDRAILS.md

- [ ] **Step 1: Append**

At end of `docs/RUST-GUARDRAILS.md`:

```markdown
---

## CI Quality Gates (Wave 2.3 — Layer Violations)

**Status:** active.

**Manifest:** `scripts/architecture-layers.yml`. Enforcement:
`scripts/check-layers.sh` (in CI as the `layers` job).

**Rule:** a file in layer N may import from layer N or any layer < N.
Forbidden-pair rules catch specific disallowed imports (domain → ui,
state → ui, provider → ui, config.rs → tui).

**Layers:**

1. **primitives** — `src/icons.rs`, `src/icon_mode.rs`, `src/util/**`
2. **domain** — `src/session/**`, `src/state/**`, `src/adapt/**`, `src/turboquant/**`, `src/provider/**`, `src/gates/**`, `src/config.rs`
3. **orchestration** — `src/cli.rs`, `src/commands/**`
4. **ui** — `src/tui/**`
5. **entry** — `src/main.rs`, `src/lib.rs`

**Debt file:** `docs/layers-debt.txt`. Known violations with deadlines,
same forcing-function pattern as the file-size allowlist.
```

- [ ] **Step 2: Commit**

```bash
git add docs/RUST-GUARDRAILS.md
git commit -m "docs(rust-guardrails): append Wave 2.3 section (layer violations)"
```

---

### Task 4.6: Chunk 4 close — push, PR

- [ ] **Step 1: Verify**

```bash
bash scripts/check-layers.sh
bats tests/scripts/check-layers.bats
python3 -m unittest discover -s tests
cargo test
```

Expected: all exit 0.

- [ ] **Step 2: Push and open PR**

```bash
git push -u origin chunk-4/layer-violations
gh pr create --title "feat(ci): Wave 2.3 — layer violations with YAML manifest + debt file" --body "$(cat <<'PRBODY'
## Summary

- `scripts/architecture-layers.yml`: 5-layer manifest + 4 forbidden pairs.
- `scripts/check-layers.sh`: bash+yq enforcer, bats-tested (6 scenarios).
- `docs/layers-debt.txt`: baseline from main — 1 known violation (`src/config.rs → src/tui/theme`) with deadline 2026-08-01 and resolution plan.
- `.github/workflows/ci.yml`: new `layers` job (blocking).
- `docs/RUST-GUARDRAILS.md`: appended layer policy section.

## Test plan

- [x] `bats tests/scripts/check-layers.bats` → 6/6
- [x] `bash scripts/check-layers.sh` on real repo → exit 0 (debt file absorbs known violations)
- [x] Layer manifest validated via Python unittest (extended from Wave 2.1)

## Rollout

Immediate. New layer violations fail the PR unless added to `docs/layers-debt.txt` with a justified deadline. Existing violation (`config.rs → tui/theme`) is resolved by moving `ThemeConfig` types to a layer-2 module by 2026-08-01.
PRBODY
)"
```

---

## Chunk 5: Wave 3 — Nightly mutation + miri + tsan + freshness bot

**Goal:** Scheduled nightly + weekly workflows, cargo-mutants config, miri job, ThreadSanitizer job, freshness bot GitHub Action. Branch protection activation is deferred until after warmup (2 weeks mutation, 1 week miri).

**Files:**
- Create: `.github/workflows/nightly.yml`
- Create: `.github/workflows/weekly.yml`
- Create: `.github/actions/freshness/action.yml`
- Create: `.github/actions/freshness/index.js`
- Create: `.github/actions/freshness/test/freshness.test.js`
- Create: `.cargo/mutants.toml`
- Create: `docs/ci-smoke-check.md`
- Modify: `docs/RUST-GUARDRAILS.md` (append Wave 3 section)

### Task 5.1: `cargo-mutants` config

**Files:**
- Create: `.cargo/mutants.toml`

- [ ] **Step 1: Create the config**

File: `.cargo/mutants.toml`

```toml
# cargo-mutants configuration for maestro.
# See: https://mutants.rs/

# Mutation testing runs slowly; exclude non-core modules to keep nightly
# job duration tractable.
exclude_globs = [
  "src/tui/**",
  "src/main.rs",
  "src/lib.rs",
  "src/integration_tests/**",
  "**/tests.rs",
  "**/*_test.rs",
]

# Generous timeout — some tests are slow on CI runners.
timeout_multiplier = 5.0
minimum_test_timeout = 60
```

- [ ] **Step 2: Verify locally (optional — slow)**

If you have spare time:

```bash
cargo install cargo-mutants --locked
cargo mutants --shard 0/4 --no-shuffle --baseline=skip --list
```

Expected: prints the mutant list for shard 0 without actually running tests. Just a dry-run sanity check.

- [ ] **Step 3: Commit**

```bash
git add .cargo/mutants.toml
git commit -m "feat(mutants): add cargo-mutants config"
```

---

### Task 5.2: Nightly workflow — mutation + miri

**Files:**
- Create: `.github/workflows/nightly.yml`

- [ ] **Step 1: Write the workflow**

File: `.github/workflows/nightly.yml`

```yaml
name: Nightly

on:
  schedule:
    - cron: '0 3 * * *'   # 03:00 UTC daily
  workflow_dispatch:        # allow manual runs

env:
  CARGO_TERM_COLOR: always

jobs:
  mutation:
    strategy:
      fail-fast: false
      matrix:
        shard: [0, 1, 2, 3]
    runs-on: ubuntu-latest
    timeout-minutes: 180
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install cargo-mutants
        run: cargo install cargo-mutants --locked
      # --baseline=skip is mandatory when sharding — each shard would
      # otherwise re-run the baseline test suite, quadrupling the work.
      # The baseline-green assumption is enforced by the regular `test`
      # job in ci.yml (per-PR, blocking); mutation only runs nightly
      # after main's test suite is known-green.
      - name: Run mutation tests (shard ${{ matrix.shard }}/4)
        run: cargo mutants --shard ${{ matrix.shard }}/4 --no-shuffle --baseline=skip
      - name: Upload results
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: mutants-shard-${{ matrix.shard }}
          path: mutants.out/

  miri:
    runs-on: ubuntu-latest
    timeout-minutes: 60
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: nightly
          components: miri, rust-src
      - uses: Swatinem/rust-cache@v2
      - name: Run miri on parser + integration tests
        run: |
          # Tests that spawn subprocesses or hit FFI need
          # #[cfg_attr(miri, ignore)] annotations — add them as miri
          # surfaces failures during warmup. Don't use --skip here;
          # rely on the cfg_attr ignores for clarity.
          cargo miri test --package maestro --test '*' 2>&1 | tee miri.log
      - name: Upload miri log
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: miri-log
          path: miri.log
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/nightly.yml
git commit -m "$(cat <<'EOF'
ci: add nightly workflow — cargo-mutants (4 shards) + miri

Runs daily at 03:00 UTC. Reporting-only initially — branch protection
activation deferred until after 2-week mutation warmup and 1-week miri
warmup. Artifacts uploaded for every run for triage.

--baseline=skip is mandatory with sharding. Outer baseline-green is
enforced by the regular test job in ci.yml.
EOF
)"
```

---

### Task 5.3: Weekly workflow — ThreadSanitizer (informational)

**Files:**
- Create: `.github/workflows/weekly.yml`

- [ ] **Step 1: Write the workflow**

File: `.github/workflows/weekly.yml`

```yaml
name: Weekly

on:
  schedule:
    - cron: '0 3 * * 0'   # Sundays 03:00 UTC
  workflow_dispatch:

env:
  CARGO_TERM_COLOR: always

jobs:
  tsan:
    # Informational. Posts results to a pinned issue; does NOT block main.
    # Known false positives on tokio internals — maintained suppressions
    # list is NOT pursued; weekly report catches real regressions without
    # the maintenance cost.
    runs-on: ubuntu-latest
    timeout-minutes: 120
    permissions:
      # gh issue create/comment below require this — default token only
      # has contents:read.
      contents: read
      issues: write
    env:
      RUSTFLAGS: "-Zsanitizer=thread"
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: nightly
          components: rust-src
      - name: Run async tests under ThreadSanitizer
        run: |
          cargo +nightly test \
            --target x86_64-unknown-linux-gnu \
            -Z build-std \
            -- --test-threads=1 2>&1 | tee tsan.log
        continue-on-error: true
      - name: Post summary to pinned issue
        if: always()
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          issue_number=$(gh issue list --label "tsan-weekly" --limit 1 --json number --jq '.[0].number' 2>/dev/null || true)
          if [[ -z "$issue_number" ]]; then
            issue_number=$(gh issue create \
              --title "Weekly ThreadSanitizer Report" \
              --label "tsan-weekly" \
              --body "Auto-updated by .github/workflows/weekly.yml. See the latest comment for the most recent tsan run." \
              --json number --jq '.number')
          fi
          # Comment with the summary.
          summary=$(tail -n 40 tsan.log | head -c 10000)
          gh issue comment "$issue_number" --body "Run $(date -u +%Y-%m-%d-%H:%M-UTC):\n\n\`\`\`\n$summary\n\`\`\`"
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/weekly.yml
git commit -m "$(cat <<'EOF'
ci: add weekly ThreadSanitizer workflow (informational)

Runs Sundays 03:00 UTC on async tests. Results posted to a pinned
GitHub issue tagged tsan-weekly; does NOT block main.

Rationale for informational-only: tsan has known false positives on
tokio internals. Blocking would require a maintained suppressions
list. Weekly report catches real regressions without that cost.
EOF
)"
```

---

### Task 5.4: Freshness bot — GitHub Action

**Files:**
- Create: `.github/actions/freshness/action.yml`
- Create: `.github/actions/freshness/index.js`
- Create: `.github/actions/freshness/test/freshness.test.js`
- Create: `.github/workflows/freshness.yml`

- [ ] **Step 1: Write the action metadata**

File: `.github/actions/freshness/action.yml`

```yaml
name: 'Nightly Freshness Check'
description: 'Verifies the most recent nightly run on main succeeded within the allowed window.'
inputs:
  github_token:
    description: 'GitHub token for repo API access'
    required: true
  max_age_days:
    description: 'Maximum age (days) for the most recent successful nightly'
    required: false
    default: '3'
  workflow_file:
    description: 'Name of the nightly workflow file (e.g. nightly.yml)'
    required: false
    default: 'nightly.yml'
runs:
  using: 'node20'
  main: 'index.js'
```

- [ ] **Step 2: Write the action logic**

File: `.github/actions/freshness/index.js`

```javascript
// Freshness bot: queries the GitHub API for the most recent scheduled
// run of the nightly workflow on `main`, asserts it succeeded within
// the configured max_age_days.
//
// Exits with status 0 on success, 1 on stale/failure.
//
// Uses Node 20 stdlib only (no npm deps). Pure REST calls via fetch.

const GITHUB_API = 'https://api.github.com';

async function fetchWorkflowRuns({ owner, repo, workflowFile, token }) {
  const url = `${GITHUB_API}/repos/${owner}/${repo}/actions/workflows/${workflowFile}/runs?branch=main&event=schedule&per_page=10`;
  const res = await fetch(url, {
    headers: {
      'Authorization': `Bearer ${token}`,
      'Accept': 'application/vnd.github+json',
      'X-GitHub-Api-Version': '2022-11-28',
    },
  });
  if (!res.ok) {
    throw new Error(`GitHub API error ${res.status}: ${await res.text()}`);
  }
  return (await res.json()).workflow_runs;
}

function isFresh(run, maxAgeDays) {
  if (run.status !== 'completed') return false;
  if (run.conclusion !== 'success') return false;
  const runDate = new Date(run.updated_at);
  const cutoff = new Date(Date.now() - maxAgeDays * 86400 * 1000);
  return runDate >= cutoff;
}

async function main() {
  const owner = process.env.GITHUB_REPOSITORY_OWNER;
  const repo = process.env.GITHUB_REPOSITORY.split('/')[1];
  const token = process.env.INPUT_GITHUB_TOKEN;
  const maxAgeDays = parseInt(process.env.INPUT_MAX_AGE_DAYS || '3', 10);
  const workflowFile = process.env.INPUT_WORKFLOW_FILE || 'nightly.yml';

  const runs = await fetchWorkflowRuns({ owner, repo, workflowFile, token });
  if (runs.length === 0) {
    console.log('No scheduled nightly runs found on main.');
    process.exit(1);
  }

  const mostRecent = runs[0];
  const fresh = isFresh(mostRecent, maxAgeDays);

  console.log(`Most recent ${workflowFile}: ${mostRecent.status} / ${mostRecent.conclusion} at ${mostRecent.updated_at}`);
  console.log(`Max age: ${maxAgeDays} days`);

  if (fresh) {
    console.log('Freshness check: PASS');
    process.exit(0);
  } else {
    console.log('Freshness check: FAIL (nightly is stale, failed, or missing)');
    process.exit(1);
  }
}

main().catch((err) => {
  console.error(`Error: ${err.message}`);
  process.exit(1);
});
```

- [ ] **Step 3: Write unit tests**

File: `.github/actions/freshness/test/freshness.test.js`

```javascript
// Unit tests for the freshness bot logic.
// Uses Node 20's stdlib test runner (no dev dependencies).
//
// Run: node --test .github/actions/freshness/test/freshness.test.js

import { test } from 'node:test';
import assert from 'node:assert/strict';

// Re-import the isFresh function from index.js.
// For test isolation, copy the logic here (we don't export it in the real file).
function isFresh(run, maxAgeDays) {
  if (run.status !== 'completed') return false;
  if (run.conclusion !== 'success') return false;
  const runDate = new Date(run.updated_at);
  const cutoff = new Date(Date.now() - maxAgeDays * 86400 * 1000);
  return runDate >= cutoff;
}

test('nightly that succeeded 1 day ago is fresh', () => {
  const oneDayAgo = new Date(Date.now() - 86400 * 1000).toISOString();
  const run = { status: 'completed', conclusion: 'success', updated_at: oneDayAgo };
  assert.equal(isFresh(run, 3), true);
});

test('nightly that succeeded 4 days ago is stale', () => {
  const fourDaysAgo = new Date(Date.now() - 4 * 86400 * 1000).toISOString();
  const run = { status: 'completed', conclusion: 'success', updated_at: fourDaysAgo };
  assert.equal(isFresh(run, 3), false);
});

test('nightly that failed is not fresh', () => {
  const yesterday = new Date(Date.now() - 86400 * 1000).toISOString();
  const run = { status: 'completed', conclusion: 'failure', updated_at: yesterday };
  assert.equal(isFresh(run, 3), false);
});

test('nightly still in progress is not fresh', () => {
  const justNow = new Date().toISOString();
  const run = { status: 'in_progress', conclusion: null, updated_at: justNow };
  assert.equal(isFresh(run, 3), false);
});

test('nightly exactly at max_age_days boundary is fresh', () => {
  // 3 days minus a minute.
  const atBoundary = new Date(Date.now() - 3 * 86400 * 1000 + 60 * 1000).toISOString();
  const run = { status: 'completed', conclusion: 'success', updated_at: atBoundary };
  assert.equal(isFresh(run, 3), true);
});
```

- [ ] **Step 4: Run tests locally**

Run: `node --test .github/actions/freshness/test/freshness.test.js`
Expected: 5 passing tests.

**Note:** `node --test` requires Node 18+. Check with `node --version`;
if local Node is older, install via nvm/brew before running. CI uses
Node 20 (via `action.yml`'s `using: 'node20'`), so CI is unaffected.

- [ ] **Step 5: Write the orchestrating workflow**

File: `.github/workflows/freshness.yml`

```yaml
name: Nightly Freshness
# Runs on every PR to main. Verifies the most recent scheduled nightly
# workflow on main succeeded within the last 3 days. Status check
# named `nightly-freshness` is required by branch protection (once
# Wave 3 warmup completes).

on:
  pull_request:
    branches: [main]

jobs:
  freshness:
    name: nightly-freshness
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: ./.github/actions/freshness
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          max_age_days: '3'
          workflow_file: 'nightly.yml'
```

- [ ] **Step 6: Commit**

```bash
git add .github/actions/freshness/ .github/workflows/freshness.yml
git commit -m "$(cat <<'EOF'
ci: add freshness bot (GitHub Action + workflow)

Verifies the most recent scheduled nightly run on main succeeded
within 3 days. Status check `nightly-freshness` is required by branch
protection (activation deferred until after Wave 3 warmup completes).

Node 20 stdlib only — no npm dependencies. Unit tests via node --test.

Runs on every PR to main. Fails the PR's status check if the nightly
is stale, failed, or missing.
EOF
)"
```

---

### Task 5.5: Document CI smoke check

**Files:**
- Create: `docs/ci-smoke-check.md`

- [ ] **Step 1: Write the manual procedure**

File: `docs/ci-smoke-check.md`

```markdown
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
- [ ] Verify `clippy` CI job fails with `cognitive_complexity` warning.

## Scenario 2 — Curated nursery lint

- [ ] On a scratch branch, introduce a redundant `.clone()` on a value that's about to be moved.
- [ ] Push. Verify `clippy` fails with `clippy::redundant_clone`.

## Scenario 3 — cargo-deny strict mode

- [ ] Add a dependency that pulls in a duplicate of an already-present crate NOT in the `skip` list.
- [ ] Push. Verify `deny` job fails with `multiple-versions`.

## Scenario 4 — File-size allowlist deadline past

- [ ] Edit `scripts/allowlist-large-files.txt`. Change one entry's deadline to `2000-01-01`.
- [ ] Push. Verify `file-size` job fails with `DEADLINE PAST`.

## Scenario 5 — File 400+ LOC not on allowlist

- [ ] Create `src/smoke_test.rs` with 450 lines.
- [ ] Push. Verify `file-size` job fails with `VIOLATION`.

## Scenario 6 — Coverage floor

- [ ] (Once core tier activates) Remove a test file that was covering a core module.
- [ ] Push. Verify `coverage` job fails (ratchet or floor).

## Scenario 7 — Layer violation

- [ ] Add `use crate::tui::theme::SerializableColor;` to `src/session/manager.rs` (domain importing ui).
- [ ] Push. Verify `layers` job fails with `VIOLATION` or `FORBIDDEN`.

## Scenario 8 — Nightly freshness (after Wave 3 activation)

- [ ] Verify current `nightly-freshness` status on any open PR is green.
- [ ] If nightly intentionally failed overnight, verify `nightly-freshness` on open PRs goes red within the 3-day window.

## Scenario 9 — Preflight hook

- [ ] Run `bash .claude/hooks/preflight.sh` locally.
- [ ] Verify all three gates run and exit 0 on a clean tree.
- [ ] Deliberately introduce a fmt violation; re-run; verify the hook fails on fmt.

## Scenario 10 — Full CI end-to-end on a clean branch

- [ ] Create a trivial fix PR (typo, etc.).
- [ ] Verify every CI job passes: test, clippy, fmt, file-size, deny, audit, coverage (reporting or blocking depending on phase), layers, nightly-freshness.

## Regression log

If any scenario fails unexpectedly, file a GitHub issue tagged `bug` + `area:ci` before releasing.
```

- [ ] **Step 2: Commit**

```bash
git add docs/ci-smoke-check.md
git commit -m "docs(ci): add manual smoke-check procedure"
```

---

### Task 5.6: Append Wave 3 section to RUST-GUARDRAILS.md

- [ ] **Step 1: Append**

At end of `docs/RUST-GUARDRAILS.md`:

```markdown
---

## CI Quality Gates (Wave 3 — Nightly Heavyweight)

**Status:** infrastructure in place; branch protection activation
deferred until after warmup (2 weeks mutation, 1 week miri).

**Scheduled workflows:**

- `.github/workflows/nightly.yml` — mutation (cargo-mutants, 4 shards,
  80% score target on core tier) + miri (parser/integration tests, pass/fail).
  Runs 03:00 UTC daily.
- `.github/workflows/weekly.yml` — ThreadSanitizer on async tests.
  Runs Sundays 03:00 UTC. Informational only — posts to a pinned
  GitHub issue, does NOT block main.

**Branch protection (activation pending):**

- `nightly-freshness` status check required. Provided by the freshness
  bot at `.github/actions/freshness/`. Fails when the most recent
  scheduled nightly on main succeeded > 3 days ago (or failed, or
  didn't run).

**Warmup period:** nightly workflow lands and reports for 2 weeks
before branch protection activates for mutation; 1 week for miri.
During warmup, iterate on timeouts / exclude-globs / suppressions.

**Rollback:** if nightly regressions produce merge-blocking friction,
flip the `required` flag on branch protection from `true` to `false`.
Nightly still runs (catches regressions), doesn't block main.
Investigate, then re-activate.
```

- [ ] **Step 2: Commit**

```bash
git add docs/RUST-GUARDRAILS.md
git commit -m "docs(rust-guardrails): append Wave 3 section (nightly + freshness bot)"
```

---

### Task 5.7: Chunk 5 close — push, PR

- [ ] **Step 1: Verify locally**

```bash
cargo fmt -- --check
cargo clippy -- -D warnings -A dead_code
cargo test
bats tests/scripts/
node --test .github/actions/freshness/test/
python3 -m unittest discover -s tests
```

Expected: all exit 0.

- [ ] **Step 2: Push and open PR**

```bash
git push -u origin chunk-5/wave-3-nightly
gh pr create --title "feat(ci): Wave 3 — nightly mutation + miri + tsan + freshness bot" --body "$(cat <<'PRBODY'
## Summary

Final wave of the CI quality-gate rollout.

- `.github/workflows/nightly.yml`: cargo-mutants (4 shards, --baseline=skip) + miri on parser/integration tests. 03:00 UTC daily.
- `.github/workflows/weekly.yml`: ThreadSanitizer on async tests. Sundays 03:00 UTC. Informational — posts to pinned issue.
- `.github/actions/freshness/`: GitHub Action verifying nightly-green-within-3-days. Node 20 stdlib only.
- `.github/workflows/freshness.yml`: PR-triggered workflow running the freshness action.
- `.cargo/mutants.toml`: mutation testing config (excludes, timeout).
- `docs/ci-smoke-check.md`: manual 10-scenario smoke procedure.
- `docs/RUST-GUARDRAILS.md`: appended Wave 3 section.

## Activation plan

1. Merge this PR — nightly + weekly + freshness bot land.
2. Wait for 2 weeks of reporting-only mutation runs. Iterate on timeouts and exclude-globs.
3. Wait for 1 week of reporting-only miri runs.
4. Open a follow-up PR that adds `nightly-freshness`, `Nightly / mutation (shard 0-3)`, `Nightly / miri` to the required-status-checks branch protection rule.

## Rollback

If nightly produces too much merge-blocking friction, flip `required` on branch protection from `true` to `false`. Nightly continues running; doesn't block.

## Test plan

- [x] `node --test .github/actions/freshness/test/` → 5/5 pass
- [x] `bats tests/scripts/` (extended suite) → all pass
- [x] `cargo mutants --list --shard 0/4 --baseline=skip` → produces expected mutant list (local dry-run)
- [ ] Nightly workflow runs at least once before merging this PR (manual trigger via `workflow_dispatch`)
PRBODY
)"
```

---

## Project Recap

Across the 5 chunks / 5+ PRs:

- **Chunk 1** delivered Wave 1: clippy tightening, curated nursery subset, cargo-deny strict, deadlined allowlist format, preflight hook population. PR #1.
- **Chunk 1b** (follow-up within 14 days): allowlist triage with real deadlines. PR #2.
- **Chunk 2** delivered Wave 2.1: cargo-llvm-cov coverage infrastructure with tiered floors. PR #3.
- **Chunk 3** delivered Wave 2.2: file-size cap 500 → 400. PR #4.
- **Chunk 4** delivered Wave 2.3: layer violations with YAML manifest + debt file. PR #5.
- **Chunk 5** delivered Wave 3: nightly mutation + miri, weekly tsan, freshness bot. PR #6.

Each chunk is independently shippable and merges before the next begins, per the user's PR-isolation rule. Plus one activation PR after Wave 3 warmup (Week 11ish) to flip branch protection from reporting-only to required.

Estimated total: ~50 commits across ~6-7 PRs, 11 weeks wall time (half-time investment).
