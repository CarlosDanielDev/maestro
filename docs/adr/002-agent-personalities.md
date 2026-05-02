# ADR 002 — Agent Personalities (Roles, Sprites, Visual Identity)

- **Status:** Accepted (spike — prototype is throwaway, never lifted to production by this ADR)
- **Date:** 2026-04-29
- **Tracking issue:** [#536](https://github.com/CarlosDanielDev/maestro/issues/536)
- **Spike branch:** `spike/agent-personalities` (prototype is throwaway — gated behind `--features spike` and never built by `cargo build`)

---

## Problem

The agent-graph view (shipped in #527, animated in #529) gives users a relational picture of which agents touch which files. Visual identity is still anonymous: every agent renders as the same 4-cell rectangle with a label like `⠋ #520`. With three or more concurrent sessions the user can read *what each agent is doing right now* but cannot tell, **at a glance, what kind of work the agent is doing** — coordinating, implementing, reviewing, writing docs, fixing CI.

The reference visual is the [octogent](https://github.com/hesamsheikh/octogent) Pac-Man-family of pixel-art ghost sprites: a small set of role-colored characters (magenta API, green Core, orange Docs, red DevOps, yellow Coordinator, white Frontend) that read instantly even at low resolution. Maestro is a terminal UI — there is no SVG canvas, no animation runtime — so adapting that idea to ratatui requires answering several open questions before we can ship any real renderer:

1. What does *role* / *personality* mean for a maestro session — derived from the prompt, user-tagged, auto-classified, or hybrid?
2. What is the canonical role list? How many, named what?
3. What is the sprite design language? Cell footprint, character set, how role color is applied (foreground / background / both)?
4. What is the per-role color palette and the ASCII fallback when `icon_mode::use_nerd_font() == false`?
5. Where does role data live on `Session` (stored field, derived helper, or computed at render time)?
6. What is the migration path for sessions that predate the field?

This ADR records the spike's findings and a Go / No-Go verdict for a follow-up feature.

---

## Current Behavior

- Every agent renders as an identical 4-cell `Rectangle` plus a `#NNN` label in the agent-graph view (`src/tui/agent_graph/render.rs:99-200`).
- `Session` has no role / personality concept (`src/session/types.rs:202-289`).
- `Session::mode` is the only role-adjacent field, and it is always `"orchestrator"` today.
- Node color is derived from `SessionStatus` only (via `node_animation_style` in `src/tui/agent_graph/animation.rs`).

---

## Options Considered

### Role taxonomy

| # | Option | Notes |
|---|---|---|
| 1 | **No taxonomy — keep anonymous nodes (do nothing)** | Cheapest. Loses the categorical information the user is asking for. |
| 2 | **2 roles (Orchestrator vs. Worker)** | Trivially small. Maps to `Session::mode` today. Loses the docs/review/devops distinctions the user wants. |
| 3 | **5 roles (Orchestrator / Implementer / Reviewer / Docs / DevOps)** | Aligns with maestro's existing subagent registry (`subagent-architect`, `subagent-qa`, `subagent-security-analyst`, `subagent-docs-analyst`) and `SessionStatus::CiFix` / `ConflictFix` classes. **Recommended.** |
| 4 | **6 roles (add Frontend, per octogent palette)** | Adds a category that has no real workload in maestro today (maestro is a CLI tool, not a frontend stack). Parked as future-considered. |
| 5 | **Open-ended user-tag taxonomy** | Maximum flexibility, but the user must invent and remember names. Loses the "everyone shares the same vocabulary" property that makes the colors readable. |

We rejected the open-ended approach (#5) because every team would invent a different vocabulary; the ghost-color metaphor only works if the palette is shared. We rejected the 2-role minimum (#2) because the issue's headline use case is *distinguishing reviewers from implementers from devops* — exactly the cases #2 collapses.

### Role derivation

| # | Option | Notes |
|---|---|---|
| 1 | **Auto-classify from prompt only** | No user friction. Opaque: the user has no mental model of why their session became orange. No escape hatch when the classifier is wrong. |
| 2 | **User-assigned via `--role` CLI flag only** | Maximum control. Most maestro sessions are auto-spawned by `subagent-orchestrator` with no human at the keyboard, so most sessions collapse to the default and the feature is invisible. |
| 3 | **Manual via `maestro.toml` only** | Wrong scope: `maestro.toml` is global config, not per-session. There is no single "role" answer at the config level. |
| 4 | **Hybrid — explicit override > prompt classifier > default** | Combines the auto-spawn coverage of #1 with the escape hatch of #2. Reuses `src/session/intent.rs`'s keyword-classifier idiom unchanged. **Recommended.** |

### Sprite footprint

| # | Option | Notes |
|---|---|---|
| 1 | **4×3 cells** | Issue's floor. Barely enough to tell a ghost from a rectangle. |
| 2 | **6×6 cells** | Issue's ceiling. Recognizable Pac-Man-family silhouette. **Recommended.** |
| 3 | **Variable per role** | Forces the layout engine to know each role's bounding box, breaks the "renderer treats all sprites identically" invariant. |

### ASCII fallback

| # | Option | Notes |
|---|---|---|
| (a) | **Single-character role glyph + role color** | Too few characters to disambiguate (Orchestrator vs. Operations both want `O`). |
| (b) | **3-character role abbreviation in role color, inside the existing rectangle** | Survives both color loss (label is readable) and font loss (already ASCII). **Recommended.** |
| (c) | **Hide sprite, use only a role-colored rectangle** | Invisible to users with color blindness. Effectively "no change" from current state. |

---

## Chosen Approach

**5-role taxonomy + hybrid derivation + 6×6 foreground-only sprite + 3-character abbrev fallback.**

Each axis is independently testable and independently revisable. The combination matches octogent's visual idiom while degrading gracefully on terminals that lack UTF-8 nerd-font support.

Properties:

- **Deterministic** — given the same prompt and the same keyword corpus, `derive_role` always returns the same `Role`.
- **Compile-time uniform sprite shape** — `Sprite([[char; 6]; 6])` is enforced by the type system; variable-size sprites cannot accidentally land.
- **Two independent fallback dimensions** — color loss and font loss are addressed separately. Color-blind users see the abbreviation; font-deprived users see a colored rectangle.
- **No stored field on `Session` from this spike** — the prototype hard-codes role per session in the example binary; the stored field is the follow-up's job. Keeps the merge-to-main blast radius bounded to a single ADR file.

---

## Role Taxonomy

Five roles. Six was the issue's hint, but **Frontend** has no meaningful workload in maestro today and is parked. Five gives the spike enough signal without inventing fictional categories.

| Role | Color (16-color name) | ANSI / xterm | One-line description | Mapping rationale |
|---|---|---|---|---|
| `Orchestrator` | Yellow | ANSI 33 | Coordinates other sessions; spawns work; merges PRs. | Matches `Session::mode = "orchestrator"`. Yellow = "Coordinator" from octogent. |
| `Implementer` | Green | ANSI 32 | Writes code: edits files, adds features, runs `cargo test`. The default role. | Largest category in maestro's session log. Green = "Core" from octogent. |
| `Reviewer` | Magenta | ANSI 35 | Reads code, runs gates, posts PR reviews. | Maps to `subagent-security-analyst` and `subagent-qa`. Magenta = "API" in octogent's palette; semantically closest to "scrutinizing surface" here. |
| `Docs` | Orange | xterm 208 | Writes `.md` files, updates ADRs, regenerates `directory-tree.md`. | Maps to `subagent-docs-analyst`. Orange = "Docs" verbatim from octogent. |
| `DevOps` | Red | ANSI 31 | CI fixes, conflict resolution, dependency bumps, infrastructure. | Maps to `SessionStatus::CiFix` and `SessionStatus::ConflictFix` activity. Red = "DevOps" verbatim from octogent and matches the "danger" semantic. |

**Canonical Rust enum** (the prototype implements; the follow-up promotes to `src/session/role.rs`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    #[default]
    Implementer,
    Orchestrator,
    Reviewer,
    Docs,
    DevOps,
}
```

`Implementer` is the default because (a) it is the largest category, (b) `Default` falls out for serde backward compat — matching the `SessionIntent::Work` precedent at `src/session/intent.rs:8-16`, (c) "unknown prompt → Implementer" is the safest miscategorization: any session that does work gets a green sprite, the most common color, so misfires are visually invisible.

### Future-considered roles

- **Frontend** — no JS/CSS workload in maestro itself. If a dogfooding target ever ships a web layer, add it.
- **API** — magenta API doesn't correspond to anything maestro produces today. Reviewer takes the magenta slot; if API design ever becomes a session category, recolor Reviewer or split the slot.
- **Spawning** — this is a `SessionStatus`, not a role. A session is spawning *into* a role. Conflating them would break the "role is the agent's job, status is what they're doing right now" invariant.

---

## Sprite Design Language

### Cell footprint

- **Minimum 4×3** — issue's floor. Barely a silhouette.
- **Maximum 6×6** — issue's ceiling. Recognizable Pac-Man-family ghost in 80×24.
- **Recommended:** **6×6 uniform across all roles.** Variable footprints would force the layout engine to know each role's bounding box. Pad smaller designs with spaces to 6×6.

### Character set

Two layers, both inside the Geometric Shapes Unicode block (U+25xx) so they render in any UTF-8 terminal — no nerd-font extension required for the body itself, only for accent-mode glyphs that already live inside `icon_mode`'s scope.

**Body fill:**
- `█` (full block, U+2588) — dense body
- `▓` (dark shade, U+2593) — body texture (mid-row)
- ` ` (space) — eye sockets and underside fringe

**Accent dots** (eyes / personality marks; small set so all sprites read as the same family):
- `●` (U+25CF) — eye
- `○` (U+25CB) — hollow eye (`Docs` "thoughtful")
- `▼` (U+25BC) — fringe tooth (`DevOps` danger)
- `◆` (U+25C6) — accent (`Orchestrator` crown)

### How role color is applied

**Foreground only.** No background fills.

1. **Background fills clash** with the existing `node_animation_style` background-flash on transitions (`src/tui/agent_graph/animation.rs:97`). Two sources fighting for the same cell makes both unreadable.
2. **Foreground-only matches octogent's reference**, which uses the role color as the sprite's primary fill, not its surroundings.
3. **ASCII fallback degrades cleanly** — a colored rectangle in foreground looks like a small colored bar; a colored rectangle in background looks like a stamp of shame.

### Per-role sprite grids

Each row is a six-element char array. Spaces are explicit. The body uses `█`; accents are documented above. Total cell count is identical (6×6 = 36 cells) so layout treats every role uniformly.

**Orchestrator (Yellow `◆` — "the conductor with a crown")**

```
Row 0: [' ', '◆', '█', '█', '◆', ' ']    crown peaks
Row 1: ['█', '█', '█', '█', '█', '█']    dome
Row 2: ['█', '●', '█', '█', '●', '█']    eyes
Row 3: ['█', '█', '█', '█', '█', '█']    mid
Row 4: ['█', '█', '█', '█', '█', '█']    belly
Row 5: ['█', ' ', '█', '█', ' ', '█']    fringe (2 legs)
```

**Implementer (Green — "default ghost, no decoration")**

```
Row 0: [' ', ' ', '█', '█', ' ', ' ']    crown
Row 1: [' ', '█', '█', '█', '█', ' ']    dome
Row 2: ['█', '●', '█', '█', '●', '█']    eyes
Row 3: ['█', '█', '█', '█', '█', '█']    mid
Row 4: ['█', '█', '█', '█', '█', '█']    belly
Row 5: ['█', ' ', '█', '█', ' ', '█']    fringe
```

**Reviewer (Magenta — "monocle on the right eye, narrowed left eye")**

```
Row 0: [' ', ' ', '█', '█', ' ', ' ']
Row 1: [' ', '█', '█', '█', '█', ' ']
Row 2: ['█', '▓', '█', '█', '●', '█']    narrowed L, focused R
Row 3: ['█', '█', '█', '█', '◆', '█']    monocle
Row 4: ['█', '█', '█', '█', '█', '█']
Row 5: ['█', ' ', '█', '█', ' ', '█']
```

**Docs (Orange — "hollow eyes = thoughtful; pen-stripe across the belly")**

```
Row 0: [' ', ' ', '█', '█', ' ', ' ']
Row 1: [' ', '█', '█', '█', '█', ' ']
Row 2: ['█', '○', '█', '█', '○', '█']    hollow eyes
Row 3: ['█', '█', '█', '█', '█', '█']
Row 4: ['█', '▓', '▓', '▓', '▓', '█']    pen-stripe
Row 5: ['█', ' ', '█', '█', ' ', '█']
```

**DevOps (Red — "fanged fringe — handle with care")**

```
Row 0: [' ', ' ', '█', '█', ' ', ' ']
Row 1: [' ', '█', '█', '█', '█', ' ']
Row 2: ['█', '●', '█', '█', '●', '█']
Row 3: ['█', '█', '█', '█', '█', '█']
Row 4: ['█', '█', '█', '█', '█', '█']
Row 5: ['▼', '█', '▼', '▼', '█', '▼']    teeth (4 fangs)
```

**Visual differentiation invariants** (every sprite obeys):

- Row 5 contains the leg / fringe pattern (every ghost has a base).
- Rows 2–3 contain the eyes (every ghost has a face).
- Per-role uniqueness lives in row 0 (Orchestrator crown, DevOps blank), row 4 (Docs pen-stripe), row 5 (DevOps teeth, others classic legs), or eye choice (Reviewer monocle, Docs hollow).

---

## Role Derivation

**Chosen: Hybrid — explicit override > prompt classifier > default.**

Resolution order at session creation time:

1. **Explicit `--role <name>` CLI flag** (clap `ValueEnum` for `Role`). Highest priority.
2. **Per-prompt keyword classification** — `derive_role(prompt: &str) -> Role`. Reuses the `src/session/intent.rs` idiom verbatim: case-insensitive substring matching against per-role keyword lists. Returns `Implementer` as the unmatched default.
3. **`Default::default()` → `Implementer`** for backward compat when the JSON state file predates the field.

**Trade-off explicitly named:** ETC + Flexibility, accepting reduced Determinism. The classifier's keyword corpus is a living config; two different keyword tables → two different colorings of the same prompt. The `--role` override is the user's escape hatch when the classifier disagrees. This matches the pattern in `intent.rs:212-215`.

**Property-test gate (specified for the follow-up, not this spike):**

- ≥80% accuracy on a 25-prompt seed corpus (5 prompts per role).
- Classifier mapping documented in `derive_role`'s rustdoc so a tester knows the rules without reading the function body.
- The intent classifier targets >90%; role classifier gets a lower bar because there are 5 categories instead of 2 — random baseline is 20%, not 50%.

---

## Data Model

**Verdict: stored field with derived fallback.**

The follow-up adds:

```rust
// src/session/types.rs (FOLLOW-UP, not this spike)
pub struct Session {
    // ...existing fields...

    /// Role classification for the agent. Stored explicitly so the renderer
    /// has a O(1) lookup. Defaults to Implementer for old JSON state files
    /// that predate this field (see Role::default()).
    #[serde(default)]
    pub role: super::role::Role,
}

// src/session/role.rs (NEW MODULE in follow-up)
pub fn derive_role(prompt: &str) -> Role { /* ... */ }

// Resolution at Session::new()
impl Session {
    pub fn new(prompt: String, /* ... */, role_override: Option<Role>) -> Self {
        let role = role_override.unwrap_or_else(|| derive_role(&prompt));
        // ...
    }
}
```

### Why stored, not pure-derived at render time

1. **Cost.** Render-time derivation re-runs the keyword classifier every redraw tick. With ≤10 sessions × 50 fps that is 500 classifier calls/sec — cheap, but wasteful.
2. **Stability.** A session's prompt text never changes after creation. Re-deriving on every read is reasoning twice about the same input.
3. **Override.** A user `--role` override has nowhere to live if the role is purely derived.

### Why also derived (the helper survives), not stored-only

`#[serde(default)]` on the stored field means old JSON files with no `role` get `Role::default() = Implementer`. That is safe (most sessions are implementers), but it loses information for old sessions whose prompts were obviously orchestrator/docs work. The follow-up MAY ship a one-shot migration that runs `derive_role` against every existing session's `prompt` and back-fills the field.

### Migration story

- **Default migration** (zero work, follow-up ships this): missing field → `Role::default()` → `Implementer`. Old sessions show as green; future sessions are correctly classified at creation. This is the right default — old finished sessions are about to fade out anyway (per ADR-001's dim-then-remove policy).
- **Optional back-fill** (deferred): a `state::store` migration that walks `MaestroState.sessions`, calls `derive_role(&prompt)` for any session with `role == Default`, and writes back. Safe because the field has a `Default` and the back-fill is idempotent.

---

## ASCII Fallback

**Chosen: 3-character role abbreviation in role color, inside the existing rectangle.**

| Role | Abbrev |
|---|---|
| Orchestrator | `ORC` |
| Implementer | `IMP` |
| Reviewer | `REV` |
| Docs | `DOC` |
| DevOps | `OPS` |

Three characters is the minimum that disambiguates at a glance and still fits inside the existing 4-cell node rectangle (the same shape used today by `node_style`). One character collapses Orchestrator and Operations onto the same `O`. Hiding the sprite entirely (`(c)`) leaves a color-only signal, which fails for color-blind users.

### Implementation seam

The fallback is gated by `crate::icon_mode::use_nerd_font()` (`src/icon_mode.rs:28`). The render path is:

- `use_nerd_font() == true` → 6×6 sprite grid (`Paragraph` with 6 fg-styled `Line`s).
- `use_nerd_font() == false` → existing 4-cell colored `Rectangle` + 3-char colored abbreviation label one row below.

The branching mirrors `SessionStatus::symbol()` at `src/session/types.rs:70-76`.

---

## Test Strategy (for the Follow-Up)

The spike has no production tests — this section specifies the tests the follow-up issue inherits. Copy verbatim into the follow-up DOR.

1. **Insta snapshot per role × per icon-mode = 5 × 2 = 10 snapshots.**
   - Path: `src/tui/snapshot_tests/agent_personalities/<role>_<mode>.snap`.
   - Backend: `ratatui::backend::TestBackend` at fixed 80×24.
   - Asserts: rendered sprite for one named session, role-colored fg, ASCII fallback when forced.
   - Pattern: identical to the existing `src/tui/snapshot_tests/` precedent (CI enforces `INSTA_UPDATE=no`).

2. **Property test for `derive_role` — ≥80% accuracy on a 25-prompt corpus.**
   - Pattern: identical to `intent.rs:512-564` (`classifier_accuracy_on_spec_corpus_is_above_90_percent`).
   - Corpus split: 5 prompts per role.
   - Failure mode: a single misclassification within tolerance is silent; > 5 misclassifications fails the build.

3. **serde round-trip for the new `Session::role` field.**
   - Pattern: identical to `intent` round-trip at `types.rs:1246-1259`.
   - Three tests: `role_field_round_trips_via_serde`, `role_defaults_to_implementer_when_absent_in_json`, `role_serializes_as_snake_case_implementer`.

4. **`transition_to` non-interaction test.**
   - Assert that `transition_to(Status::Running, ...)` does NOT mutate `session.role`. One test, ~5 lines. Catches the "role-on-status-change" anti-pattern that would silently break sprite stability mid-render.

5. **Sprite renderer invariants.**
   - All sprites are exactly 6×6 (compile-time `[[char; 6]; 6]` enforces 6×6).
   - Role abbreviation is exactly 3 ASCII bytes (one assertion per role).

6. **Manual smoke (NOT automated, documented in the follow-up DOR):**
   - Spawn 5 sessions covering all 5 roles in the same `maestro` run.
   - Verify the agent-graph view shows 5 visually distinguishable nodes.
   - Re-run with `MAESTRO_ASCII_ICONS=1` and verify the ASCII fallback shows 5 distinguishable colored 3-letter labels.

**Guardrail citations the follow-up's blueprint MUST include:**

- §6 serialization: `Role` enum carries `#[serde(rename_all = "snake_case")]` and `Default`; new `Session::role` field carries `#[serde(default)]` for backward compat.
- §7 testing: insta snapshots in `src/tui/snapshot_tests/`; trait-based fakes for any test that needs a fake-but-realistic `Session`.
- §11 observability: classifier emits `tracing::debug!(prompt = %prompt, role = ?role, "derived role")` (not `println!` / `dbg!`).
- §2 errors: derivation is infallible. The `--role` override uses clap's built-in `ValueEnum` parser, so invalid values get a clap error — not a panic.

---

## Go / No-Go Verdict

**VERDICT: Go (5 roles, hybrid derivation, 6×6 foreground sprite, 3-char abbrev fallback).**

The five Go signals (orchestrator runs the prototype to fill in ✅/❌; a single failure in signals 1, 2, or 4 flips the verdict to No-Go):

| # | Signal | Result | Verdict-flipping? |
|---|---|---|---|
| 1 | Two named 6×6 sprites render side-by-side, distinguishable, in 80×24 nerd-font mode | ✅ | Yes |
| 2 | ASCII fallback (`MAESTRO_ASCII_ICONS=1`) shows two distinguishable role abbreviations in distinct colors | ✅ | Yes |
| 3 | `Sprite` data structure is fixed-size and 36 chars exactly | ✅ (compile-time invariant) | No (caught at `cargo check`) |
| 4 | The 5-role enum + 5 sprites fit inside the file-size cap (≤220 LOC each) and trip no clippy warnings | ✅ (`cargo clippy --features spike` clean) | Yes |
| 5 | `derive_role` produces ≥3 distinct `Role` values across the prototype's hard-coded prompt corpus | ✅ (sanity check) | No (exhaustive test arrives in the follow-up) |

If a future re-evaluation flips any verdict-flipping signal to ❌, this ADR's `Status` field MUST be amended to `Superseded by ADR NNN` rather than edited in place.

### Follow-up issue

- **Title:** `feat(tui): agent personality sprites in agent-graph view`
- **DOR seed (one paragraph):** Add a `Role` enum (5 variants: Orchestrator, Implementer, Reviewer, Docs, DevOps) with a `derive_role(prompt: &str) -> Role` keyword classifier mirroring `src/session/intent.rs`. Add a `#[serde(default)] pub role: Role` field to `Session` with a `--role` clap override on `maestro run`. Replace the agent-graph node rectangle with a 6×6 sprite when `icon_mode::use_nerd_font()` is true; fall back to the existing rectangle plus a 3-character role-colored abbreviation label otherwise. Tests: insta snapshots (5 roles × 2 modes), classifier accuracy (≥80% on 25-prompt corpus), serde round-trip + default backward compat, `transition_to` does not mutate role. Cite `docs/RUST-GUARDRAILS.md` §6 serialization, §7 testing, §11 observability. Reference `docs/adr/002-agent-personalities.md` for the sprite grids and color palette.

The follow-up issue is opened after this ADR merges and references the same milestone.

---

## Out of Scope

### Permanent (per issue #536, never reopens for argument)

- **Animated sprites** (spawn frames, death frames, walking cycles). Static art only.
- **Per-role personality voice in chat output.** Sprites are visual; the LLM's prose stays neutral.
- **Sprite editor UI.** No in-app character grid editing.
- **User-uploaded custom sprites.** Hard-coded set in the binary; no plugin system.
- **Color themes for roles.** Inherits ADR-001's stance: status colors must stay legible; per-user re-themes are scope creep.

### Out of scope for this spike (deferred to the follow-up)

- Stored `Session::role` field with backward-compat back-fill migration. Default-on-deserialize is sufficient for the spike's analysis.
- Additional roles (Frontend / API / Tester). The canonical list is 5; the follow-up may extend if a real session category demands it.
- `--role` CLI flag implementation. The prototype hard-codes both sprites' roles; clap wiring is the follow-up's job.
- Property-test corpus tuning past the 25-prompt seed. Mature corpus growth is incremental, not spike work.
- Integration with `App::navigate_to` and the wider TUI dispatcher. The spike's code paths are gated by `feature = "spike"` and never run in production builds.
- Dependency on `Session::intent` (Work-vs-Consultation) for derivation. Orthogonal axes — a Consultation session can still be a Reviewer; future heuristic might combine, not now.

---

## Prototype

**Location:** `examples/agent_personalities_spike.rs` (entry point) plus `src/tui/agent_personalities/{mod,role,sprite,palette,render}.rs`, all gated behind `--features spike`.

**Why `examples/` and not `src/bin/`:** `cargo build` does *not* build examples — they require `cargo build --examples`. Accidental shipping is impossible. By contrast, `src/bin/foo.rs` is auto-discovered as an additional binary.

**Feature gate:** A `[features] spike = []` block in `Cargo.toml` and `#[cfg(feature = "spike")]` on the `agent_personalities` module declaration. Any reference to `feature = "spike"` outside the spike branch is a smoking gun for accidental main-line landing.

**Run command:**

```sh
cargo run --example agent_personalities_spike --features spike
# Resize terminal to 80×24 for the headline AC.
# Press `q` to quit. No other input is handled.

# ASCII fallback test:
MAESTRO_ASCII_ICONS=1 cargo run --example agent_personalities_spike --features spike
# Expect: two colored rectangles with 3-letter labels (ORC, IMP), distinct colors.
```

**Cleanup after this ADR merges** (single commit, delete-only on production code):

```sh
git rm examples/agent_personalities_spike.rs
git rm -r src/tui/agent_personalities/
# Edit Cargo.toml to remove the `[features] spike = []` block.
# Edit src/tui/mod.rs to remove the cfg-gated `mod agent_personalities;` line.
# Delete the throwaway branch:
git branch -D spike/agent-personalities
git push origin --delete spike/agent-personalities
```

The cleanup is a single commit touching `Cargo.toml` (delete-only) and `src/tui/mod.rs` (delete-only) plus the directory deletes. No production logic moves in either direction. ADR-001's identical pattern landed cleanly in #526; this spike inherits that proof.

**Fake data:** 2 sessions hand-built in the example's `main`:

- One `Session { mode: "orchestrator", prompt: "coordinate the merge of #527 and #528" }` → expected `Role::Orchestrator`.
- One `Session { mode: "orchestrator", prompt: "implement #529 — loading animations" }` → expected `Role::Implementer`.

Two sessions, not five, because two is the minimum to demonstrate "sprites are visually distinguishable from each other" — the headline AC. Five would force the layout engine to handle wrapping in 80×24, which is the follow-up's problem.

---

## References

- Issue [#536](https://github.com/CarlosDanielDev/maestro/issues/536)
- Reference visual: octogent — <https://github.com/hesamsheikh/octogent>
- Predecessor ADR: `docs/adr/001-agent-graph-viz.md` (structural template, identical spike-cleanup pattern)
- Maestro classifier idiom: `src/session/intent.rs:8-216` (`SessionIntent`, `classify_intent`)
- Maestro icon-mode plumbing: `src/icon_mode.rs:28` (`use_nerd_font`)
- Maestro current node rendering: `src/tui/agent_graph/render.rs:99-200`
- Maestro session model: `src/session/types.rs:202-289` (`Session` struct)
- Maestro guardrails: `docs/RUST-GUARDRAILS.md` §2 errors, §6 serialization, §7 testing, §11 observability

---

## Addendum — Viewport-Derived Sprite Sizing (#576)

The original `draw_sprite_on_canvas` hard-coded `ROW_STEP = 0.1` and `X_OFFSET = -0.078` in canvas units, calibrated to the 80×24 minimum viewport. On larger viewports the canvas-cell size shrinks (cell height ≈ 0.034 at 60 inner rows) but the constants stayed fixed, so:

- `ctx.print` floored each sprite row to a different cell and 1- to 3-row gaps appeared between consecutive sprite rows — the sprite read as 4 disconnected chunks (hat / body / torso / legs) instead of a single character.
- The 6-row sprite spanned ~0.6 canvas units regardless of viewport, occupying ~60% of canvas height on 200×60 — pulling visual attention away from the graph.
- Horizontal centering drifted on wider viewports because `X_OFFSET = -0.078` represented "3 cell widths" only at 80 columns.

### Decision

**Approach A** (viewport-derived constants), per the trade-off triangle Simplicity + Flexibility (small Performance cost — one f64 division per frame, negligible at human tick rates). Approach B (Paragraph overlay) was rejected because it would split rendering across two coordinate systems for no additional benefit.

`draw_sprite_on_canvas` now takes `inner_cols` and `inner_rows` and derives:

- `row_step = 2.0 / (inner_rows - 1)` — exactly one terminal row per sprite row.
- `x_offset = -2.5 * 2.0 / (inner_cols - 1)` — half of a 6-cell row, centered on `cx`.

The `(inner_cols - 1)` and `(inner_rows - 1)` denominators mirror ratatui 0.29's label-print mapping (`Canvas::render` in `widgets/canvas.rs`) — see the `(width - 1)` / `(height - 1)` resolution computed for label rendering. Using `(inner_cols)` / `(inner_rows)` directly produces accumulated drift over six rows and collisions on the bottom rows on large viewports.

`LABEL_RADIUS_SPRITE` was replaced with `(2.5 + SPRITE_LABEL_BUFFER_CELLS) * cell_h` computed at the call site, where `SPRITE_LABEL_BUFFER_CELLS = 1.5` keeps the agent's `#NNN` label visually one to two cells outside the sprite top across all supported viewports. The constant 0.40 was the right value at 80×24 only.

### Properties

- **Contiguous on every viewport** — rows 80×24 → 200×60 produce six adjacent terminal rows with no gaps. Verified by `sprite_no_gap_between_rows_at_*` and three new size-pinned snapshots.
- **Bounded height budget** — sprite always renders in 6 cells (≈ 16% of inner rows on 120×40, well under the 30% AC).
- **Horizontally centered within ±1 cell** — verified by `sprite_horizontally_centered_within_1_cell_at_*`.
- **No new dependencies** — pure refactor of existing `f64` math.

### Side Effects

- All nerd-font sprite snapshots were re-baselined. The agent label sits one cell closer to the sprite at 120×40 and 200×60 because the dynamic `label_radius` shrinks with the cell size; on 80×24 the visual is essentially unchanged.
- File labels and edge animation rendering are unaffected — they live in independent code paths.
