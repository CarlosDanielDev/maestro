# ADR 001 — Agent Graph Visualization in ratatui Canvas

- **Status:** Accepted (productionized in #526)
- **Date:** 2026-04-29
- **Tracking issue:** [#513](https://github.com/CarlosDanielDev/maestro/issues/513)
- **Spike branch:** `spike/explore-ratatui-canvas` (prototype is throwaway — never merged to main)

---

## Problem

Maestro's TUI currently renders concurrent Claude sessions as a grid of side-by-side panels. Non-technical users have reported that the panel layout is *static and hard to parse* once 3 or more sessions are active: they cannot see, at a glance, **which agents are touching the same files** or **which files are contested**. The panels show *what each agent is doing*; they do not show *how the agents relate to one another or to the work product*.

The reference visual is the [octogent web preview](https://github.com/hesamsheikh/octogent/blob/main/static/images/preview_1.jpg) — a force-directed graph of agents and the artifacts they produce. The constraint we accept is that maestro is a terminal UI: there is no SVG canvas, no animation runtime, and no mouse. We must do this with ratatui's `Canvas` widget or fall back to a simpler view.

This ADR records the spike's findings and a Go / No-Go verdict for a follow-up feature.

---

## Options Considered

| # | Option | Notes |
|---|---|---|
| 1 | **Stay with grid panels (do nothing)** | Cheapest. Loses the relational information the user is asking for. |
| 2 | **Force-directed (Fruchterman–Reingold, 1991)** | Industry standard. Settles in 50–200 iterations. Non-deterministic across runs without a seed; iteration cost is per-redraw unless cached. |
| 3 | **Layered / Sugiyama DAG** | Classical algorithm for hierarchical graphs. Multi-pass (rank → order → coordinate). Wrong fit: our graph is bipartite + bidirectional, not hierarchical. |
| 4 | **Plain circular** (all nodes on one ring) | Zero-iteration, deterministic, simple. Loses the `Agent` vs. `File` semantic — they all look identical. |
| 5 | **Concentric / radial bipartite** (agents inner ring, files outer ring) | Zero-iteration, deterministic, single pass. Bipartite structure matches the `Agent ↔ File` relation we want to communicate. **Recommended.** |
| 6 | **Pre-baked static layout** (hand-tuned positions) | Trivial. Doesn't generalize past the demo. Useless for live data. |

We rejected force-directed (#2) for the spike because:

1. Iteration cost is incompatible with maestro's "rebuild every frame" render path (`src/tui/ui.rs:109`). At 10 nodes the cost is invisible; at 50+ it stalls.
2. Snapshot tests (the chosen test strategy for the follow-up feature) require deterministic output. Force-directed needs an explicit seed and pinned iteration count to be testable, which is doable but adds complexity the spike does not need.
3. The graph is small *by design* (the issue's Out-of-Scope section caps node count at 10 agents / 50 edges); the algorithmic advantages of force-directed only emerge above that.

We document #2 as the obvious upgrade if the follow-up needs to scale past ~15 nodes.

---

## Chosen Approach

**Deterministic concentric/radial bipartite layout, single pass, `ratatui::widgets::canvas::Canvas` with `Marker::Braille`.**

Algorithm:

1. Partition input nodes into `Agent`s and `File`s.
2. Place agents on an inner ring of radius `r_a` centered in the viewport.
3. Place files on an outer ring of radius `r_f > r_a`, sorted by **barycenter** (the average angular position of the agents that touch the file) to minimise edge crossings. This is one heuristic pass — no further iterations.
4. Compute angular positions as `i · 2π / n` per ring.
5. Render edges as straight `Line` shapes in the `Canvas`'s virtual `[-1.0, 1.0]` coordinate space; render nodes as a `Rectangle` plus a `Context::print(...)` label one cell below.

Properties:

- **Deterministic** — identical inputs produce identical pixel output. Snapshot-testable without seed plumbing.
- **O(n + e)** per redraw. Layout cost at 10 nodes / 20 edges is **< 1 ms** on the prototype (measured via `tracing::debug!("layout: {} µs", elapsed)`); see Go/No-Go signal #2.
- **Bipartite by construction** — the visual reads as "agents on the inside, files on the outside, lines between them" without a legend.
- **Honest about scale** — the layout becomes unreadable above ~12 nodes in 80×24, which matches the issue's Out-of-Scope cap.

---

## Data Model

A thin in-memory adapter built from `&[Session]` (defined at `src/session/types.rs:202`). **No new persistent data layer is needed for the spike.** Existing `Session::files_touched: Vec<String>` (`src/session/types.rs:223`) is the only input.

```rust
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum NodeId {
    Agent(uuid::Uuid),
    File(std::path::PathBuf),
}

pub(crate) enum NodeKind {
    Agent { status: SessionStatus, label: String },
    File { basename: String },
}

pub(crate) struct GraphNode {
    pub id:    NodeId,
    pub kind:  NodeKind,
    pub label: String,
}

pub(crate) struct GraphEdge {
    pub from: NodeId,
    pub to:   NodeId,
    // `label` and `kind` intentionally omitted in the spike. Only one edge type
    // exists today (Agent → File "touches"). Document for the follow-up.
}

#[derive(Clone, Copy)]
pub(crate) struct Positioned {
    pub id_idx: usize,   // index into the input `nodes` slice
    pub x:      f64,     // virtual coords in [-1.0, 1.0]
    pub y:      f64,
}
```

**Adapter:** `pub(crate) fn build_graph(sessions: &[&Session]) -> (Vec<GraphNode>, Vec<GraphEdge>)` — signature updated in #527 to accept a slice of references, avoiding a copy of each `Session` at the call site.

- One `GraphNode::Agent` per `Session`.
- One `GraphNode::File` per **unique** path across all `Session::files_touched` values (deduplicated by full path).
- One `GraphEdge` from each agent to each file in its `files_touched`.

Items are `pub(crate)` only — none of this is part of maestro's public API. The follow-up feature inherits the contract; cleanup after this spike is a single directory delete.

### Data gap (soft blocker for richer follow-ups)

The current `Vec<String>` shape is sufficient for the headline use case (which agent touches which file), but **insufficient** if the follow-up needs:

- **Edge weights** — recency of touch, line-count delta, write-vs-read distinction. Requires a structured `FileTouchEvent { path, kind, at }` and a parser change in `src/session/parser.rs`.
- **Tool nodes** — tool calls are stored only as strings in `ActivityEntry::message` (`src/session/types.rs:325`), parsed by prefix matching in `Session::has_tool_calls` (`src/session/types.rs:449`). Promoting "Tool" to a graph node type requires the parser to emit structured tool-call events first.
- **Fork edges** — `Session::parent_session_id` and `Session::child_session_ids` already exist (`src/session/types.rs:240`). The spike does not visualise them, but the follow-up can with no parser changes.

The follow-up issue's DOR should explicitly call out which of these the feature needs and block on the corresponding parser change if any.

---

## Layout Trait Boundary

The layout engine is hidden behind a trait so the follow-up's snapshot tests can fake it:

```rust
pub(crate) struct Viewport { pub width: u16, pub height: u16 }

pub(crate) trait Layout {
    /// Pure: no I/O, no terminal access. Returns positions in virtual [-1.0, 1.0].
    fn position(
        &self,
        nodes:    &[GraphNode],
        edges:    &[GraphEdge],
        viewport: Viewport,
    ) -> Vec<Positioned>;
}

pub(crate) struct ConcentricLayout;
impl Layout for ConcentricLayout { /* ... */ }
```

Test strategy for the follow-up (recorded here so the follow-up DOR can copy it verbatim):

1. **`insta` snapshots** of `ratatui::backend::TestBackend::buffer()` at fixed terminal sizes (80×24, 120×40). `insta` is already in `[dev-dependencies]` — no new dep.
2. **`FakeLayout`** that returns hard-coded `Positioned` values, so renderer tests do not depend on `ConcentricLayout`'s trigonometry.
3. **Property-style tests** for `ConcentricLayout` itself: invariant "no two nodes at the same position", "all positions in `[-1.0, 1.0]`", "ring radii respect `r_f > r_a`".

---

## Rendering Primitives

- **Widget:** `ratatui::widgets::canvas::Canvas` with `x_bounds([-1.0, 1.0])` and `y_bounds([-1.0, 1.0])`.
- **Marker:** `Marker::Braille` (2×4 sub-cell resolution) when UTF-8 nerd-font is available; `Marker::Block` when ASCII mode is forced.
- **Edge labels:** **skipped in the spike** — collision-avoidance for mid-edge labels is its own design problem and is not in the issue's acceptance criteria. Documented under "Out of Scope (this spike)".
- **Nodes:**
  - Agent: small `Rectangle` (1×1 virtual unit), colour = status colour (`Running` → green, `Errored` → red, `Completed` → muted), label `S-XXXX` (first 4 chars of `Uuid`) or `#<issue>` if the session has one.
  - File: same `Rectangle` in a neutral colour, label = basename (last path component). On collision, fall back to last 2 path components separated by `/`.

> **Addendum (#568, 2026-05-01):** The original render path anchored every file label's leftmost cell at the node marker, causing labels on the left half of the ring to grow rightward into the graph interior and labels on the right half to overflow the canvas border. `place_file_label()` in `label_placement.rs` corrects this: right-half labels anchor at the marker and grow outward (rightward); left-half labels right-anchor at the marker and grow outward (leftward); markers within the `|p.x| ≤ FILE_LABEL_DEAD_BAND` center band remain centered. Labels that would overshoot the available outward span are truncated with an ellipsis via `truncate_with_ellipsis()`. No architectural change — this is a render-path bug fix with no impact on the layout algorithm, data model, or trait boundary.

### A note on "color themes"

The issue's permanent Out-of-Scope list excludes "color themes". We read this as **user-configurable themes**, not "no colour at all" — status colours are necessary for legibility. The follow-up MUST NOT add a theme system; it MAY hard-code status colours from the existing `Theme` struct.

---

## Single-Agent Fallback

When fewer than 2 agents are present, **do not render an empty or near-empty graph.** Show a centred card via the existing `centered_rect` helper (`src/tui/help.rs`) with content:

```
   ▶  S-1234  RUNNING
       Files: main.rs, config.rs

   1 agent active — graph view activates at 2+ agents
```

This is the action-oriented empty state called for in [user feedback memory `feedback_user_visible_feedback`]. The user always knows *why* the graph is dormant.

---

## Finished-Agent State Policy

**Dim then remove.** When a session enters `Completed`, `Errored`, or `Killed`:

1. The node is rendered in `text_muted` colour and its status icon switches to `✔` / `✗`.
2. After ~5 redraw ticks the node is removed from the graph and the layout re-balances.

Rationale:

- **Dim only** clutters the graph as more sessions finish.
- **Remove immediately** loses the "what just happened?" moment.
- **Dim → remove** preserves transient context without permanent noise.

The spike fakes the 5-tick window with a counter. The follow-up needs `chrono::DateTime` reads for accuracy across slow redraws (a wall-clock window is more honest than a tick window).

---

## Minimum Terminal Size

**80×24 hard floor.** Below that, the graph is not rendered at all; a single line is drawn:

```
Agent graph requires 80×24 (current: WxH). Press [g] to switch back to panel view.
```

Quality bands:

| Size | Quality |
|------|---------|
| < 80 × 24 | Disabled (message above). |
| 80 × 24 – 100 × 30 | Legible but tight. Outer ring touches screen edges. |
| ≥ 100 × 30 | Comfortable padding; recommended. |

---

## Non-UTF-8 Fallback

Maestro already has a UTF-8 / nerd-font detection path: `crate::icon_mode::use_nerd_font()` (`src/icon_mode.rs:28`). The graph view reuses it.

- **`use_nerd_font() == true`:** `Marker::Braille`, status icons from `SessionStatus::symbol`.
- **`use_nerd_font() == false`:** `Marker::Block`, ASCII status markers (`R`/`E`/`C`), bracketed node labels (`[S1]`, `[F:main.rs]`). Diagonal edges look like staircases — accept this honestly; no attempt at line-art smoothing.

A `--ascii-graph` CLI override is **not** added by the spike. The follow-up may add one if user testing finds the auto-detection insufficient.

---

## Go / No-Go Verdict

**VERDICT: Go (concentric + braille).**

Filled in after running `cargo run --example agent_graph_spike --features spike` at 80×24, 120×40, and 40×20. Each signal is independently verifiable; a single failure in signals 1, 2, or 4 flips the verdict to No-Go.

| # | Signal | Result | Notes |
|---|---|---|---|
| 1 | 3 agents + 2 files render legibly in 80×24 with no overlap | ✅ | Headline AC. Verified manually at 80×24. |
| 2 | Concentric layout computed in < 1 ms for 10 nodes | ✅ | Well under budget at 10/20 nodes/edges (no per-redraw stall). |
| 3 | Edge crossings ≤ 4 in the 5-node test case **OR** labels readable at default font | ✅ | Barycenter sort keeps crossings to ≤ 1 in the prototype's fake data. |
| 4 | ASCII fallback is at least minimally legible | ✅ | Diagonals are staircased but nodes and shape are clear; documented honestly. |
| 5 | Single-agent card looks no worse than the existing detail panel | ✅ | Centered card with a hint string is strictly an improvement over an empty graph. |

**Follow-up issue:** to be opened after this ADR merges; the issue title is `feat(tui): agent graph visualization (concentric layout)` and its DOR will copy the trait boundary, data model, and Go/No-Go signals from this ADR as the contract.

If a future spike re-evaluation flips any signal to ❌, this ADR's `Status` field MUST be amended to `Superseded by ADR NNN` rather than edited in place.

---

## Out of Scope

### Permanent (per issue #513)
- Real-time animation
- Mouse interaction
- User-configurable colour themes
- Web rendering
- Node types beyond `{Agent, File}`

### Out of scope for this spike (deferred to follow-up)
- Mid-edge labels with collision avoidance
- Force-directed layout
- Wall-clock-based finished-agent dim window (spike uses a tick counter)
- Runtime terminal-capability detection beyond the existing `icon_mode` flag
- Integration with `TuiMode` enum and the main render dispatcher (`src/tui/ui.rs`) — **delivered in #527** (`TuiMode::AgentGraph`, render-time gate in `draw()`, defense-in-depth gate in `App::navigate_to`, `mode_hints` arm)
- Tool-call nodes / fork edges (require parser changes — see Data Gap)

---

## Prototype

**Location:** `examples/agent_graph_spike.rs` (entry point) + `src/tui/agent_graph/{mod,model,layout,render}.rs` (gated behind `--features spike`).

**Why `examples/` and not `src/bin/`:** `cargo build` does *not* build examples — they require `cargo build --examples` or `cargo run --example`. This makes accidental shipping impossible. By contrast, `src/bin/foo.rs` is auto-discovered as an additional binary.

**Feature gate:** A new `[features] spike = []` block in `Cargo.toml` and `#[cfg(feature = "spike")]` on the `agent_graph` module declaration. Any reference to `feature = "spike"` outside the spike branch is a smoking gun for accidental main-line landing.

**Run command:**

```sh
cargo run --example agent_graph_spike --features spike
# Resize terminal to 80×24, 120×40, 40×20 for manual smoke test.
# Press `q` to quit. No other input is handled.
```

**Cleanup after this ADR merges:**

```sh
git rm examples/agent_graph_spike.rs
git rm -r src/tui/agent_graph/
# Remove the `[features] spike = []` block and the `mod agent_graph;`
# declaration from src/tui/mod.rs.
```

The cleanup is a single commit with no `main`-line code touched outside `Cargo.toml` and `src/tui/mod.rs` — both delete-only changes.

> Cleanup executed in #526. Spike code lifted to `src/tui/agent_graph/` with phase-offset and aspect-ratio bug fixes; `[features] spike` and `examples/agent_graph_spike.rs` removed.

**Fake data:** 3 agents + 3 unique files, hand-built `Session` structs (skipping `Session::new` to avoid `intent::classify_intent`). One file is shared between two `Running` agents to exercise the barycenter sort, and one agent is `Completed` to exercise the dim-and-remove policy.

---

## References

- Issue [#513](https://github.com/CarlosDanielDev/maestro/issues/513)
- ratatui `Canvas` widget: <https://docs.rs/ratatui/0.29/ratatui/widgets/canvas/struct.Canvas.html>
- Octogent visual reference: <https://github.com/hesamsheikh/octogent/blob/main/static/images/preview_1.jpg>
- Fruchterman, T. M. J., & Reingold, E. M. (1991). *Graph drawing by force-directed placement.* Software: Practice and Experience, 21(11), 1129–1164.
- Maestro session model: `src/session/types.rs:202` (`Session` struct)
- Maestro icon-mode plumbing: `src/icon_mode.rs:28` (`use_nerd_font`)
- Maestro existing graph-view precedent: `src/tui/dep_graph.rs`
