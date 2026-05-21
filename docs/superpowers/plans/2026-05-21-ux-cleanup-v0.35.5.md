# UX Cleanup (v0.35.5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans.

**Goal:** Remove the legacy TUI chrome that survived behind `behavior.new_ux=false`, drop the flag itself, and delete unused snapshots and test fixtures.

**Architecture:** Three subtraction commits. Order matters: legacy TuiMode variants go first (so callers can't reference them), then the flag (so no branch depends on it), then snapshot housekeeping.

**Tech Stack:** Rust 2024 · `cargo build --workspace` for compile gate.

**Spec:** Section 8 (Phase 6).

---

### Task 1 (UX-6-01): Remove legacy Landing / Dashboard / Overview / SessionSwitcher TuiMode variants

**Files:**
- Modify: `src/tui/app/types.rs` (delete variants)
- Delete: `src/tui/landing.rs`, `src/tui/session_switcher.rs` (after callers migrated)
- Modify: `src/tui/ui.rs` (delete `draw_legacy` branch — see Task 2)
- Modify: `src/tui/screen_dispatch.rs` (drop matches)
- Modify: every `#[allow(deprecated)]` attribute added in v0.33.0 UX-1-07 — remove the attribute now that the variants are gone

- [ ] **Step 1: Inventory legacy uses**

Run: `rg "TuiMode::(Overview|Landing|Dashboard|SessionSwitcher|CostDashboard|TokenDashboard|TurboquantDashboard|AgentGraph|DependencyGraph)" src/`

Confirm every hit is inside the legacy-only `draw_legacy` path or an outdated test fixture.

- [ ] **Step 2: Delete variants from `TuiMode` enum**

In `src/tui/app/types.rs`, remove these arms:

```rust
Overview,
Landing,
Dashboard,
SessionSwitcher,
CostDashboard,
TokenDashboard,
TurboquantDashboard,
AgentGraph,
DependencyGraph,
```

- [ ] **Step 3: Delete obsolete screen modules**

Remove files:
- `src/tui/landing.rs` (if not referenced; the `LandingScreen` struct from `src/tui/screens/landing/` may still be referenced by tests — verify before delete)
- `src/tui/session_switcher.rs`
- `src/tui/cost_dashboard.rs` (after extraction in UX-1-02 left only the body fn; the rest is now dead)
- `src/tui/token_dashboard.rs`
- `src/tui/turboquant_dashboard.rs`
- `src/tui/dep_graph.rs`
- `src/tui/agent_graph/` (only the now-unused parent; keep the bipartite renderer if InsightsHub::Agents tab calls it)

For each, confirm with `rg "<symbol>" src/` before deleting.

- [ ] **Step 4: Build clean**

Run: `cargo build`
Expected: PASS with no warnings about unused or dead code.

- [ ] **Step 5: Commit**

```bash
git add src/tui/app/types.rs src/tui/ui.rs src/tui/screen_dispatch.rs src/tui/landing.rs \
        src/tui/session_switcher.rs src/tui/cost_dashboard.rs src/tui/token_dashboard.rs \
        src/tui/turboquant_dashboard.rs src/tui/dep_graph.rs src/tui/agent_graph/
git commit -m "chore(tui): remove legacy TuiMode variants + screens (UX-6-01)"
```

---

### Task 2 (UX-6-02): Remove behavior.new_ux flag + legacy dispatcher branch

**Files:**
- Modify: `src/config.rs` (drop `new_ux` field from `BehaviorConfig`)
- Modify: `src/tui/ui.rs` (delete `draw_legacy` and the dispatcher branch)
- Modify: `CHANGELOG.md` (announce removal)
- Modify: `docs/user-guide/ux-spine.md` (drop the opt-out paragraph)

- [ ] **Step 1: Failing test for absence of the flag**

In `src/config.rs`:

```rust
#[test]
fn new_ux_flag_is_gone() {
    // Compiles iff BehaviorConfig has no new_ux field.
    let s: &str = "[behavior]\ncaveman_mode = false";
    let cfg: BehaviorConfig = toml::from_str(s).unwrap();
    // No new_ux access anywhere — if this comments out, the test still must compile.
    assert!(!cfg.caveman_mode);
}
```

- [ ] **Step 2: Drop the field**

In `BehaviorConfig`:

```rust
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct BehaviorConfig {
    pub caveman_mode: bool,
}
```

- [ ] **Step 3: Delete the dispatcher branch**

In `src/tui/ui.rs`:

```rust
pub fn draw(f: &mut Frame, app: &mut App, theme: &Theme) {
    draw_new_ux(f, app, theme);
}
// remove fn draw_legacy entirely
```

- [ ] **Step 4: Update docs**

In `CHANGELOG.md` under `## [v0.35.5]`:

```markdown
### Removed
- `behavior.new_ux` config flag (the new sidebar UX is now the only path). Legacy chrome code paths fully removed.
```

In `docs/user-guide/ux-spine.md`, remove the opt-out paragraph.

- [ ] **Step 5: Build + test + refresh landing snapshots (CHANGELOG coupling)**

```
cargo build
cargo test
cargo insta accept   # for any landing snapshot drift from CHANGELOG change
```

- [ ] **Step 6: Commit**

```bash
git add src/config.rs src/tui/ui.rs CHANGELOG.md docs/user-guide/ux-spine.md \
        src/tui/snapshot_tests/snapshots/
git commit -m "chore(config): remove behavior.new_ux flag + legacy draw branch (UX-6-02)"
```

---

### Task 3 (UX-6-03): Delete unused snapshot tests for retired screens

**Files:**
- Delete: `src/tui/snapshot_tests/landing.rs` (after gathering any still-useful tests into a different module)
- Delete: outdated snapshots under `src/tui/snapshot_tests/snapshots/` (those whose source files were removed in UX-6-01)
- Modify: `src/tui/snapshot_tests/mod.rs` to drop module declarations

- [ ] **Step 1: List orphan snapshots**

Run:

```bash
cd /Users/carlos/projects/maestro
ls src/tui/snapshot_tests/snapshots/ | sort
rg "fn " src/tui/snapshot_tests/ | awk -F: '{print $1}' | sort -u
```

Cross-reference. Snapshots without a backing test become orphans.

- [ ] **Step 2: Delete orphan snapshot files**

For each orphan:

```bash
git rm src/tui/snapshot_tests/snapshots/<orphan>.snap
```

- [ ] **Step 3: Delete now-empty test modules**

If `src/tui/snapshot_tests/landing.rs` has no surviving tests, delete it and drop `pub mod landing;` from `src/tui/snapshot_tests/mod.rs`. Same for `agent_graph_dispatcher`, `agent_graph_keybinding_hint`, any other module whose source was removed.

- [ ] **Step 4: Build + test (full suite)**

```
cargo test
```
Expected: PASS with no orphan snapshot warnings (`insta` reports them otherwise).

- [ ] **Step 5: Commit**

```bash
git add -A src/tui/snapshot_tests/
git commit -m "chore(tui): delete orphan snapshot tests for retired screens (UX-6-03)"
```

---

## Milestone Dependency Graph

```
Level 0:
• UX-6-01 (depends on v0.35.0 UX-5-04 GA flip)

Level 1:
• UX-6-02 (depends on UX-6-01)

Level 2:
• UX-6-03 (depends on UX-6-01)

Sequence: UX-6-01 → UX-6-02 ∥ UX-6-03
```
