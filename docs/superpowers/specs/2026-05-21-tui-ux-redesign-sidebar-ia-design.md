# Maestro TUI UX Redesign — Sidebar IA + Wizard Spine

**Status:** Draft
**Date:** 2026-05-21
**Owner:** Carlos Daniel
**Brainstorm transcript:** brainstorming skill, 6 clarifying questions + 6 design sections

---

## 1. Problem Statement

Maestro v0.28.x ships ~30 `TuiMode` variants reachable from a flat letter-hotkey landing menu. Tools (Cost / Token / TurboQuant dashboards, Issues / Milestones / Roadmap / PRD, Sessions / SessionSwitcher / Detail / Fullscreen, Adapt, Settings, multiple wizards) sit at the same level with no semantic grouping. Users cannot tell at a glance what Maestro is fundamentally _for_ (running sessions and interactions); the long menu hides the primary path. There is no consistent chrome across screens — each screen draws its own header, borders, footer, and activity log. Wizards are bespoke implementations sharing no base trait.

### Goals

1. Surface Maestro's primary path (Sessions, Interactions) on launch.
2. Give every screen the same outer chrome (banner, header, sidebar, footer, activity log strip).
3. Group ~30 screens into 5 semantic buckets reachable from a persistent sidebar.
4. Standardize wizards behind one trait so step UX is consistent across IssueWizard, MilestoneWizard, TeamWizard, AdaptWizard.
5. Ship incrementally behind a runtime flag, without freezing the 42 in-flight issues.

### Non-goals

- Theming overhaul (themes already work).
- Replacing ratatui or moving away from a TUI.
- Removing keyboard-driven UX in favor of mouse — mouse remains optional.
- Changing the data model (sessions, issues, milestones, settings).

---

## 2. Brainstorm Decisions (locked)

| Question | Decision |
|---|---|
| Blast radius | B — Spine + IA reshuffle (group screens into buckets; chrome consistent; some merges) |
| Categories | A — 5 buckets: **Run · Plan · Review · Insights · System** |
| Sidebar grammar | C — Hybrid accordion (single bucket expanded at a time) |
| Keyboard contract | C — Ctrl+letter global hotkeys + Tab focus rings + Ctrl+P palette |
| Default landing | B — Home dashboard with summary cards (Sessions / Cost / Suggestions) |
| Cutover strategy | A — Spine first + one hub per phase, behind `behavior.new_ux` flag |
| Wizard standardization | Added as Phase 0.5 — own milestone v0.32.5 |
| Milestone shape | Phase 0 = single milestone v0.32.0; cleanup = own milestone v0.35.5 |
| Adapt | Sidebar row inside Run |
| Prompt | Inside Run (not System) |
| Project Stats | Inside Insights (not Plan) |
| Sidebar width | Fixed 22 cols (3-col rail when collapsed) |
| Bypass banner | Above sidebar, full width |
| Activity Log default | Hidden, toggle with `Ctrl+L` |
| Breadcrumb format | `MAESTRO v0.X » Bucket » Tool` |
| Tab visual | Background highlight box for active tab |
| Footer hints | Global + screen-local |
| Plan global key | `Ctrl+N` |
| Tab cycling | Skip collapsed sidebar |
| Esc in narrow rail | Expands sidebar back |

---

## 3. Tool Inventory and Bucket Assignment

Mapping current `TuiMode` variants to new buckets. Wizards and confirms are modals, not sidebar items.

### Home (always above buckets)

| Sidebar item | New screen | Replaces |
|---|---|---|
| Home | `HomeScreen` | `Landing`, `Dashboard` |

### Run

| Sidebar item | New screen / tab | Current `TuiMode`(s) |
|---|---|---|
| Sessions | `RunHub::Sessions` (list + drilldown) | `Overview`, `SessionSwitcher`, `Detail`, `Fullscreen`, `LogViewer`, `SessionSummary`, `CompletionSummary`, `HollowRetry`, `ConfirmKill` |
| Interactions | `RunHub::Interactions` | new (v0.30 work, #732..#743) |
| Queue | `RunHub::Queue` | `QueueConfirmation`, `QueueExecution`, `ContinuousPause` |
| Adapt | `RunHub::Adapt` (launches modal) | `AdaptWizard`, `AdaptFollowUp` |
| Prompt | `RunHub::Prompt` | `PromptInput` |

### Plan

| Sidebar item | New screen | Current `TuiMode`(s) |
|---|---|---|
| Issues | `PlanHub::Issues` | `IssueBrowser` (+ `IssueWizard` as modal) |
| Milestones | `PlanHub::Milestones` (sub-tabs List/Health) | `MilestoneView`, `MilestoneHealth` (+ `MilestoneWizard` as modal) |
| Roadmap | `PlanHub::Roadmap` | `Roadmap` |
| PRD | `PlanHub::PRD` | `Prd` |

### Review

| Sidebar item | New screen | Current `TuiMode`(s) |
|---|---|---|
| PRs | `ReviewHub::PRs` | `PrReview` |
| CI Errors | `ReviewHub::CiErrors` (+ `GateOutputViewer` drilldown) | `CiErrorReview`, `GateOutputViewer` |
| Release Notes | `ReviewHub::Releases` | `ReleaseNotes` |

### Insights (hub with tabs)

| Tab | Current `TuiMode` |
|---|---|
| Cost | `CostDashboard` |
| Tokens | `TokenDashboard` |
| TurboQuant | `TurboquantDashboard` |
| Agent Graph | `AgentGraph`, `DependencyGraph` (sub-toggle) |
| Project Stats | `ProjectStats` |

### System

| Sidebar item | New screen | Current `TuiMode`(s) |
|---|---|---|
| Settings | `SystemHub::Settings` (inner tabs preserved) | `Settings` |
| Teams | `SystemHub::Teams` (list + launch modal) | `TeamWizard` |

### Modals (not in sidebar)

`ConfirmKill`, `ConfirmExit`, `BypassWarning`, `Sanitize`, `IssueWizard`, `MilestoneWizard`, `TeamWizard`, `AdaptWizard`, `AdaptFollowUp`.

### Retired entirely

`Overview`, `Landing`, `Dashboard`, `SessionSwitcher`, `Sanitize` (as TuiMode), bespoke `*Dashboard` TuiModes once Insights tabs ship.

---

## 4. Sidebar Contract

### Visual

```
┌─ Bypass banner (when active, full width) ─────────────────────────────┐
├─ Header bar ──────────────────────────────────────────────────────────┤
│ MAESTRO v0.32  »  Run  »  Sessions   ✦ 0 agents · $12.40/$50 · TQ:ON  │
├──────────────┬────────────────────────────────────────────────────────┤
│              │ ┌─ Tabs strip (tabbed hubs only) ───────────────────┐  │
│   Sidebar    │ │  Cost  ▎ Tokens ▎ TurboQuant ▎ Agents ▎ Stats     │  │
│   (22 cols)  │ └────────────────────────────────────────────────────┘ │
│              │                                                        │
│              │   Content pane (screen body)                           │
│              │                                                        │
├──────────────┴────────────────────────────────────────────────────────┤
│ Footer hints: [Ctrl+R] Run · [Ctrl+P] Palette · [?] Help · [q] Quit   │
└───────────────────────────────────────────────────────────────────────┘
[ Activity Log strip — toggleable Ctrl+L ]
```

### Sidebar (expanded)

```
┌──────────────────────┐
│ MAESTRO v0.32.0      │  ← brand row
├──────────────────────┤
│ ◌ Home               │
│ ▼ Run                │  ← active bucket
│   • Sessions         │  ← selected item (bullet)
│     Interactions     │
│     Queue            │
│     Adapt            │
│     Prompt           │
│ ▶ Plan               │
│ ▶ Review             │
│ ▶ Insights           │
│ ▶ System             │
├──────────────────────┤
│ 0 agents · $12.40/$50│  ← live stats
└──────────────────────┘
```

### Sidebar (collapsed rail, `Ctrl+B`)

```
┌───┐
│ H │
│ R │  ← active highlighted
│ N │
│ V │
│ I │
│ S │
└───┘
```

### Rules

| Rule | Behavior |
|---|---|
| Single bucket open | Exactly one bucket expanded at a time. Opening another collapses prior. |
| Selected item bullet | `•` precedes the active item. |
| Collapsed bucket | `▶ Bucket`. Expanded: `▼ Bucket` + nested rows. |
| Width | Fixed 22 cols expanded, 3 cols collapsed. |
| Activity Log | Moved off bottom-of-every-screen. Toggleable via `Ctrl+L`, opens 6-line strip at bottom of content pane. |
| Bypass banner | Full-width strip above sidebar. |
| Footer stats | Live: agents count · cost · TQ on/off · update badge. |

### Rust state

```rust
// src/tui/navigation/sidebar_state.rs (new)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket { Home, Run, Plan, Review, Insights, System }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolId {
    Home,
    RunSessions, RunInteractions, RunQueue, RunAdapt, RunPrompt,
    PlanIssues, PlanMilestones, PlanRoadmap, PlanPrd,
    ReviewPrs, ReviewCi, ReviewReleases,
    InsightsCost, InsightsTokens, InsightsTurboquant, InsightsAgents, InsightsStats,
    SystemSettings, SystemTeams,
}

pub struct SidebarState {
    pub active_bucket: Bucket,
    pub active_tool: ToolId,
    pub collapsed: bool,
}
```

Lives in `App`. Screen impls read it; they emit `ScreenAction::SelectTool(ToolId)` to change selection — they do not mutate the state directly.

---

## 5. Screen Contract (Chrome)

### Trait changes

```rust
pub trait Screen: KeymapProvider {
    fn handle_input(&mut self, event: &Event, mode: InputMode) -> ScreenAction;
    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme);

    // NEW — defaults so existing screens compile:
    fn tabs(&self) -> Option<&dyn TabsModel> { None }
    fn breadcrumb(&self) -> &'static str { "" }
    fn footer_hints(&self) -> Vec<(&'static str, &'static str)> { Vec::new() }

    fn desired_input_mode(&self) -> Option<InputMode> { None }
}

pub trait TabsModel {
    fn tabs(&self) -> &[&'static str];
    fn active(&self) -> usize;
    fn select(&mut self, idx: usize);
}
```

### Chrome composer

`src/tui/chrome.rs` (new) composes per-frame:

1. Optional `bypass_banner` strip (1 line).
2. `header` strip (1 line: breadcrumb + live stats).
3. Split horizontally: `sidebar` (22 cols or 3 cols) + `body`.
4. Body splits vertically: optional `tabs` strip (1 line) + `content`.
5. `footer` strip (1 line: global + screen-local hints).
6. Optional `activity_log` strip (6 lines) above footer when toggled.

Screen body owns only the content pane area. Screens stop drawing their own headers, borders, footers, activity logs.

### Modal stack

```rust
// src/tui/app/mod.rs
pub struct App {
    pub modal_stack: Vec<Box<dyn Modal>>,
    // ... existing fields
}

pub trait Modal {
    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme);
    fn handle_input(&mut self, event: &Event) -> ModalAction;
}

pub enum ModalAction { None, Pop, Submit(ModalResult), Cancel }
```

Modal renders centered over the dimmed frame. `Esc` pops top of stack. All wizards become `Modal` impls.

### Standard content sub-grammar

When a screen has list + detail, use `widgets::split_pane::SplitPane`:

```
┌─ list (≥40% width) ──┬─ detail (≥40% width) ─┐
│ row 1                │ rendered detail        │
│ row 2 (selected)     │                        │
│ row 3                │                        │
└──────────────────────┴────────────────────────┘
```

Most screens collapse to a single content pane.

---

## 6. Keyboard Contract

### Global hotkeys

| Key | Action |
|---|---|
| `Ctrl+H` | Home |
| `Ctrl+R` | Run bucket |
| `Ctrl+N` | Plan bucket (P reserved for palette) |
| `Ctrl+V` | Review bucket |
| `Ctrl+I` | Insights bucket |
| `Ctrl+S` | System bucket |
| `Ctrl+P` | Command palette |
| `Ctrl+B` | Toggle sidebar (full ↔ rail) |
| `Ctrl+L` | Toggle Activity Log strip |
| `?` | Help overlay |
| `q` | Quit (with confirm) |
| `Esc` | Pop modal · collapse bucket · expand rail · leave region |

### Focus rings

| Key | Action |
|---|---|
| `Tab` | Next region: Sidebar → Tabs → Content (skip empty + skip collapsed sidebar) |
| `Shift+Tab` | Previous region |
| `j/k` or `↓/↑` | Move within focused region |
| `h/l` or `←/→` | Sidebar: collapse / expand bucket. Tabs: prev / next tab. |
| `Enter` | Activate focused item |
| `g/G` | Top / bottom (content region only) |

### Command palette

`Ctrl+P` opens fuzzy palette across:
1. All tools (e.g. "cost dashboard", "issues").
2. Verbs (e.g. "launch session", "create milestone").
3. Active sessions by issue # / title.
4. Open issues by number.

Result rows: `[icon] Title · breadcrumb · hotkey-hint`. `Enter` runs; `Tab` reveals subverbs.

### Text-input mode (wizards)

When `InputMode::Text` active:
- Letter keys → input field.
- `Ctrl+*` global hotkeys still work.
- `Esc` cancels wizard.
- `Ctrl+Enter` submits.

### Screen-local conventions (preserved)

`Enter` open · `n` new · `/` filter · `r` refresh · `1..9` jump to tab N.

### Collision policy

1. Rarely-used local hotkey colliding with new global → drop.
2. Core local (e.g. Settings `s` Save) → keep local; global rebinds (Settings still reached via `Ctrl+S` bucket key).
3. New globals are always `Ctrl+`-prefixed.

---

## 7. Wizard Spine (Phase 0.5)

### Problem

Today's wizards (IssueWizard, MilestoneWizard, TeamWizard, AdaptWizard) share patterns but no code:
- Each step page implements its own header/footer/keymap.
- Step counters (`1/10`, `1/7`) drawn inconsistently.
- Validation reporting differs per wizard.
- Navigation keys (Next/Back/Cancel) differ.
- Field rendering reimplemented per wizard (some use Settings schema renderer, others bespoke).

### Solution

Introduce a `Wizard` trait + `WizardFrame` widget. Every wizard becomes a list of `WizardStep` impls. Frame draws step counter, title, body slot, validation banner, nav footer. Validation lifecycle standardized.

```rust
// src/tui/widgets/wizard/mod.rs (new)
pub trait Wizard {
    fn title(&self) -> &'static str;
    fn steps(&self) -> &[Box<dyn WizardStep>];
    fn current_step(&self) -> usize;
    fn advance(&mut self) -> WizardResult;
    fn rewind(&mut self) -> WizardResult;
    fn cancel(&mut self);
    fn submit(&mut self) -> WizardResult;
}

pub trait WizardStep {
    fn header(&self) -> &'static str;            // "Step 3/7: Issue Type"
    fn render_body(&self, f: &mut Frame, area: Rect, theme: &Theme);
    fn handle_input(&mut self, event: &Event) -> WizardStepAction;
    fn validate(&self) -> Vec<ValidationError>;  // empty = ready to advance
    fn footer_hints(&self) -> Vec<(&'static str, &'static str)>;
}

pub enum WizardResult { Continue, Done(ModalResult), Cancelled }
pub enum WizardStepAction { None, Submit, Back, Cancel }
```

### Frame

```
┌─ Modal (centered over dimmed frame) ─────────────────────┐
│ Issue Wizard                                  Step 3/10  │
│ ──────────────────────────────────────────────────────── │
│ Classification                                           │
│                                                          │
│   (•) feat — new functionality                           │
│   ( ) fix — defect repair                                │
│   ( ) chore — maintenance                                │
│                                                          │
│ ──────────────────────────────────────────────────────── │
│ ⚠ Validation: pick a classification before continuing.   │
│ ──────────────────────────────────────────────────────── │
│ [←] Back  [→] Next  [Ctrl+Enter] Submit  [Esc] Cancel    │
└──────────────────────────────────────────────────────────┘
```

### Migration

Phase 0.5 issues:

1. Define `Wizard` + `WizardStep` traits + `WizardFrame` widget.
2. Reuse Settings `schema_tab` field renderer for typed fields (text, number, dropdown, toggle).
3. Migrate IssueWizard step-by-step.
4. Migrate MilestoneWizard.
5. Migrate TeamWizard.
6. Migrate AdaptWizard.
7. Retire bespoke wizard scaffolding (`screens::wizard_fields.rs` and friends — keep schema reusable parts).
8. Snapshot tests for one representative step of each wizard.

---

## 8. Phasing and Flag Rollout

### Feature flag

```toml
# maestro.toml
[behavior]
new_ux = false   # default false until Phase 5; flipped in v0.35.0
```

Read at startup, NOT live-toggleable. Per CLAUDE.md `behavior.*` namespace policy: style toggle only, no security control gated on it.

`tui::ui::draw` branches once at top: `new_ux ? chrome::draw_new(app, f) : legacy::draw_old(app, f)`. No compile-time `cfg`; both code paths coexist until Phase 6.

### Phases

| Phase | Milestone | Theme |
|---|---|---|
| 0 | v0.32.0 — UX Spine | Sidebar, chrome, palette, modal stack, Home cards |
| 0.5 | v0.32.5 — Wizard Spine | Wizard trait + 4 wizards migrated |
| 1 | v0.33.0 — Insights Hub | Cost/Tokens/TurboQuant/Agents/Stats merged |
| 2 | v0.33.5 — Run Hub | Sessions/Interactions/Queue/Adapt/Prompt |
| 3 | v0.34.0 — Plan Hub | Issues/Milestones/Roadmap/PRD |
| 4 | v0.34.5 — Review Hub | PRs/CI/Releases |
| 5 | v0.35.0 — System Hub + GA | Settings/Teams + flag default flip + docs |
| 6 | v0.35.5 — UX Cleanup | Remove legacy code paths + flag |

### Coexistence with in-flight 42 issues

- **v0.29.0 #809** — landed before this redesign starts. No conflict.
- **v0.29.5 (15 open)** — Cost/Token/Quota plumbing (#769..#776). Backend work. Feeds Insights tab content (Phase 1). SHOULD land before v0.33.0.
- **v0.30.0 (12 open)** — Interactive Iteration Sessions. UX-2-07 (Interactions tab) blocks on this milestone's #738.
- **v0.30.5 (6 open)** — Subscription Transport. Pure backend.
- **v0.31.0 (8 open)** — Role-Routed Orchestration. #821 (Teams tab ranking surface) MUST land before v0.35.0 UX-5-03.

### Risk register

| Risk | Mitigation |
|---|---|
| 42 in-flight issues touch old screens | Flag default `false`. In-flight PRs target legacy chrome until Phase 5 flip. |
| CHANGELOG↔snapshot coupling (memory `project_changelog_snapshot_coupling`) | Each phase ships landing-snapshot update if it touches the CHANGELOG release section. Pre-check hook surfaces drift. |
| Snapshot churn explosion | New snapshots gated by `new_ux=true` test fixture. Old snapshots pristine until Phase 6. |
| User accidentally enables half-finished `new_ux` | Flag undocumented in user-facing UI until Phase 5. Documented internally in CHANGELOG and CLAUDE.md. |
| Wizard standardization fragility | Phase 0.5 migrates one wizard per PR; existing wizard stays in tree until its migration lands. |

---

## 9. Issue Manifest

Stable spec IDs: `UX-P-NN` where P = phase digit (0, 0a for 0.5, 1..6), NN = serial within phase. Real GH issue numbers assigned at `gh issue create` time and recorded alongside.

### Milestone v0.32.0 — UX Spine (Phase 0)

| Spec ID | Title | Blocked By | Key files |
|---|---|---|---|
| UX-0-01 | feat(tui): Bucket + ToolId enums + SidebarState struct | none | `src/tui/app/types.rs`, `src/tui/navigation/sidebar_state.rs` (new) |
| UX-0-02 | feat(tui): widgets::sidebar accordion renderer + snapshot tests | UX-0-01 | `src/tui/widgets/sidebar.rs` (new) |
| UX-0-03 | feat(tui): chrome composer (banner + header + footer + split) | UX-0-01 | `src/tui/chrome.rs` (new), `src/tui/ui.rs` |
| UX-0-04 | feat(tui): Screen trait — add `tabs()`/`breadcrumb()`/`footer_hints()` defaults | UX-0-01 | `src/tui/screens/mod.rs` |
| UX-0-05 | feat(tui): modal stack (App.modal_stack + dim overlay rendering) | UX-0-04 | `src/tui/app/mod.rs`, `src/tui/screens/mod.rs` |
| UX-0-06 | feat(tui): command palette widget + fuzzy matcher (ADR for `nucleo` vs `fuzzy-matcher`) | UX-0-01 | `src/tui/widgets/palette.rs` (new), `Cargo.toml` |
| UX-0-07 | feat(tui): global hotkeys `Ctrl+H/R/N/V/I/S/P/B/L` | UX-0-01, UX-0-05 | `src/tui/input_handler.rs`, `src/tui/navigation/keymap.rs` |
| UX-0-08 | feat(tui): focus rings — Tab cycles Sidebar↔Tabs↔Content; skip collapsed sidebar | UX-0-07 | `src/tui/navigation/focus.rs` |
| UX-0-09 | feat(config): `behavior.new_ux` flag + dispatcher branch in `ui.rs` | UX-0-03, UX-0-07, UX-0-08 | `src/config.rs`, `src/tui/ui.rs` |
| UX-0-10 | feat(tui): Home screen — Sessions / Cost / Suggestions cards | UX-0-03, UX-0-09 | `src/tui/screens/home/` (new) |
| UX-0-11 | test(tui): chrome + sidebar snapshot suite at 80×24 / 120×40 / 200×60 | UX-0-02, UX-0-03, UX-0-10 | `src/tui/snapshot_tests/chrome.rs` (new) |

**Dependency graph for milestone description:**

```
Level 0 — no deps:
• UX-0-01 enums + state

Level 1 — depends on UX-0-01:
• UX-0-02 sidebar renderer
• UX-0-03 chrome composer
• UX-0-04 Screen trait extension
• UX-0-06 palette widget

Level 2:
• UX-0-05 modal stack (depends on UX-0-04)
• UX-0-07 global hotkeys (depends on UX-0-01, UX-0-05)

Level 3:
• UX-0-08 focus rings (depends on UX-0-07)

Level 4:
• UX-0-09 flag + dispatcher (depends on UX-0-03, UX-0-07, UX-0-08)

Level 5:
• UX-0-10 Home screen (depends on UX-0-03, UX-0-09)

Level 6:
• UX-0-11 snapshot suite (depends on UX-0-02, UX-0-03, UX-0-10)

Sequence: UX-0-01 → (UX-0-02 ∥ UX-0-03 ∥ UX-0-04 ∥ UX-0-06) → UX-0-05 → UX-0-07 → UX-0-08 → UX-0-09 → UX-0-10 → UX-0-11
```

### Milestone v0.32.5 — Wizard Spine (Phase 0.5)

| Spec ID | Title | Blocked By | Key files |
|---|---|---|---|
| UX-0a-01 | feat(tui): Wizard + WizardStep traits + WizardFrame widget | UX-0-05 | `src/tui/widgets/wizard/mod.rs` (new) |
| UX-0a-02 | feat(tui): reuse Settings schema field renderer in WizardFrame body slot | UX-0a-01 | `src/tui/screens/settings/schema_tab/`, `src/tui/widgets/wizard/fields.rs` |
| UX-0a-03 | refactor(tui): migrate IssueWizard onto Wizard trait | UX-0a-02 | `src/tui/screens/issue_wizard/` |
| UX-0a-04 | refactor(tui): migrate MilestoneWizard onto Wizard trait | UX-0a-02 | `src/tui/screens/milestone_wizard/` |
| UX-0a-05 | refactor(tui): migrate TeamWizard onto Wizard trait | UX-0a-02 | `src/tui/screens/team_wizard/` |
| UX-0a-06 | refactor(tui): migrate AdaptWizard onto Wizard trait | UX-0a-02 | `src/tui/screens/adapt/` |
| UX-0a-07 | chore(tui): retire bespoke wizard scaffolding in `screens::wizard_fields.rs` (keep schema-reusable parts) | UX-0a-03..06 | `src/tui/screens/wizard_fields.rs` |
| UX-0a-08 | test(tui): one snapshot per wizard at a representative step | UX-0a-03..06 | `src/tui/snapshot_tests/wizards.rs` (new) |

**Dependency graph:**

```
Level 0:
• UX-0a-01 (depends on v0.32.0 UX-0-05 modal stack)

Level 1:
• UX-0a-02 (depends on UX-0a-01)

Level 2 (parallel):
• UX-0a-03 IssueWizard
• UX-0a-04 MilestoneWizard
• UX-0a-05 TeamWizard
• UX-0a-06 AdaptWizard

Level 3:
• UX-0a-07 retire scaffold (depends on Level 2)
• UX-0a-08 snapshots (depends on Level 2)

Sequence: UX-0a-01 → UX-0a-02 → (UX-0a-03 ∥ UX-0a-04 ∥ UX-0a-05 ∥ UX-0a-06) → (UX-0a-07 ∥ UX-0a-08)
```

### Milestone v0.33.0 — Insights Hub (Phase 1)

| Spec ID | Title | Blocked By | Key files |
|---|---|---|---|
| UX-1-01 | feat(tui): InsightsHub scaffold + 5 tab routes (Cost / Tokens / TurboQuant / Agents / Stats) | v0.32.0 UX-0-11 | `src/tui/screens/insights/` (new) |
| UX-1-02 | refactor(tui): migrate CostDashboard body → InsightsHub::Cost | UX-1-01 | `src/tui/cost_dashboard.rs`, `src/tui/screens/insights/cost.rs` |
| UX-1-03 | refactor(tui): migrate TokenDashboard body → InsightsHub::Tokens | UX-1-01 | `src/tui/token_dashboard.rs`, `src/tui/screens/insights/tokens.rs` |
| UX-1-04 | refactor(tui): migrate TurboquantDashboard body → InsightsHub::TurboQuant | UX-1-01 | `src/tui/turboquant_dashboard.rs`, `src/tui/screens/insights/turboquant.rs` |
| UX-1-05 | refactor(tui): merge AgentGraph + DependencyGraph → InsightsHub::Agents (sub-toggle) | UX-1-01 | `src/tui/agent_graph/`, `src/tui/dep_graph.rs`, `src/tui/screens/insights/agents.rs` |
| UX-1-06 | refactor(tui): migrate ProjectStats body → InsightsHub::Stats | UX-1-01 | `src/tui/screens/project_stats/`, `src/tui/screens/insights/stats.rs` |
| UX-1-07 | chore(tui): mark 5 legacy TuiMode variants `#[deprecated]` | UX-1-02..06 | `src/tui/app/types.rs` |

**Sequence:** `UX-1-01 → (UX-1-02 ∥ UX-1-03 ∥ UX-1-04 ∥ UX-1-05 ∥ UX-1-06) → UX-1-07`

### Milestone v0.33.5 — Run Hub (Phase 2)

| Spec ID | Title | Blocked By | Key files |
|---|---|---|---|
| UX-2-01 | feat(tui): RunHub scaffold + 5 subviews | v0.32.0 UX-0-11 | `src/tui/screens/run/` |
| UX-2-02 | refactor(tui): Sessions list view replaces Overview + SessionSwitcher | UX-2-01 | `src/tui/session_switcher.rs`, `src/tui/screens/run/sessions.rs` |
| UX-2-03 | refactor(tui): Detail screen reachable as Sessions drilldown | UX-2-02 | `src/tui/detail.rs` |
| UX-2-04 | feat(tui): Queue tab folds QueueConfirmation + QueueExecution | UX-2-01 | `src/tui/screens/queue_confirmation.rs`, `src/tui/screens/run/queue.rs` |
| UX-2-05 | refactor(tui): Adapt tab launches AdaptWizard as modal | UX-2-01, v0.32.5 UX-0a-06 | `src/tui/screens/adapt/`, `src/tui/screens/run/adapt.rs` |
| UX-2-06 | refactor(tui): Prompt tab embeds PromptInput | UX-2-01 | `src/tui/screens/prompt_input/`, `src/tui/screens/run/prompt.rs` |
| UX-2-07 | feat(tui): Interactions tab (depends on v0.30 #738) | UX-2-01, v0.30.0 #738 | `src/tui/screens/run/interactions.rs` |

**Sequence:** `UX-2-01 → (UX-2-02 → UX-2-03) ∥ UX-2-04 ∥ UX-2-05 ∥ UX-2-06 ∥ UX-2-07`

### Milestone v0.34.0 — Plan Hub (Phase 3)

| Spec ID | Title | Blocked By | Key files |
|---|---|---|---|
| UX-3-01 | feat(tui): PlanHub scaffold + Issues / Milestones / Roadmap / PRD tabs | v0.32.0 UX-0-11 | `src/tui/screens/plan/` |
| UX-3-02 | refactor(tui): IssueBrowser body → PlanHub::Issues (wizard remains modal) | UX-3-01, v0.32.5 UX-0a-03 | `src/tui/screens/issue_browser/`, `src/tui/screens/plan/issues.rs` |
| UX-3-03 | refactor(tui): MilestoneView + MilestoneHealth → PlanHub::Milestones (sub-tabs List/Health) | UX-3-01, v0.32.5 UX-0a-04 | `src/tui/screens/milestone.rs`, `src/tui/screens/milestone_health/`, `src/tui/screens/plan/milestones.rs` |
| UX-3-04 | refactor(tui): Roadmap body → PlanHub::Roadmap | UX-3-01 | `src/tui/screens/roadmap/` |
| UX-3-05 | refactor(tui): Prd body → PlanHub::PRD | UX-3-01 | `src/tui/screens/prd/` |

**Sequence:** `UX-3-01 → (UX-3-02 ∥ UX-3-03 ∥ UX-3-04 ∥ UX-3-05)`

### Milestone v0.34.5 — Review Hub (Phase 4)

| Spec ID | Title | Blocked By | Key files |
|---|---|---|---|
| UX-4-01 | feat(tui): ReviewHub scaffold + PRs / CI Errors / Releases tabs | v0.32.0 UX-0-11 | `src/tui/screens/review/` |
| UX-4-02 | refactor(tui): PrReview body → ReviewHub::PRs | UX-4-01 | `src/tui/screens/pr_review/` |
| UX-4-03 | refactor(tui): CiErrorReview body → ReviewHub::CI; GateOutputViewer = drilldown | UX-4-01 | `src/tui/screens/ci_error_review.rs`, `src/tui/screens/gate_output_viewer.rs` |
| UX-4-04 | refactor(tui): ReleaseNotes body → ReviewHub::Releases | UX-4-01 | `src/tui/screens/release_notes/` |

**Sequence:** `UX-4-01 → (UX-4-02 ∥ UX-4-03 ∥ UX-4-04)`

### Milestone v0.35.0 — System Hub + GA flip (Phase 5)

| Spec ID | Title | Blocked By | Key files |
|---|---|---|---|
| UX-5-01 | feat(tui): SystemHub scaffold + Settings / Teams tabs | v0.32.0 UX-0-11 | `src/tui/screens/system/` |
| UX-5-02 | refactor(tui): Settings → SystemHub::Settings (inner tabs preserved) | UX-5-01 | `src/tui/screens/settings/` |
| UX-5-03 | feat(tui): Teams promoted from wizard-only → list screen + launch modal | UX-5-01, v0.31.0 #821, v0.32.5 UX-0a-05 | `src/tui/screens/team_wizard/`, `src/tui/screens/system/teams.rs` |
| UX-5-04 | feat(config): flip `behavior.new_ux` default → true; CHANGELOG entry | UX-5-01, UX-5-02, UX-5-03, v0.34.5, v0.34.0, v0.33.5, v0.33.0 | `src/config.rs`, `CHANGELOG.md` |
| UX-5-05 | docs(ux): user guide + screenshots for new spine | UX-5-04 | `docs/user-guide/ux-spine.md`, `docs/screenshots/` |

**Sequence:** `UX-5-01 → (UX-5-02 ∥ UX-5-03) → UX-5-04 → UX-5-05`

### Milestone v0.35.5 — UX Cleanup (Phase 6)

| Spec ID | Title | Blocked By | Key files |
|---|---|---|---|
| UX-6-01 | chore(tui): remove legacy Landing / Dashboard / Overview / SessionSwitcher TuiMode variants | v0.35.0 UX-5-04 | `src/tui/app/types.rs`, `src/tui/landing.rs`, ... |
| UX-6-02 | chore(config): remove `behavior.new_ux` flag + legacy dispatcher branch | UX-6-01 | `src/config.rs`, `src/tui/ui.rs` |
| UX-6-03 | chore(tui): delete unused snapshot tests for retired screens | UX-6-01 | `src/tui/snapshot_tests/` |

**Sequence:** `UX-6-01 → UX-6-02 ∥ UX-6-03`

---

## 10. Testing Strategy

| Layer | Approach |
|---|---|
| Unit | `Bucket`/`ToolId` transitions; `SidebarState::next/prev`; fuzzy palette ranking. |
| Snapshot | `insta` snapshots of chrome at 80×24 / 120×40 / 200×60. Sidebar collapsed + expanded. Each hub tab. |
| Integration | `src/integration_tests/` — global hotkey jumps fire correct `ScreenAction::SelectTool`. Modal stack push/pop. |
| Behavior | Toggle `behavior.new_ux=true` test fixture launches new chrome; `false` launches legacy. Both pass existing snapshots. |
| Manual | Manual smoke per phase against terminal sizes; `?` help renders correctly; `Ctrl+P` palette indexes all tools. |

Snapshots that depend on `new_ux=true` live in a separate fixture file to prevent legacy snapshot drift.

---

## 11. Open Questions (deferred)

1. Fuzzy-match crate choice (`nucleo` vs `fuzzy-matcher`) — ADR in UX-0-06.
2. Activity Log strip height — fixed 6 lines, or configurable? Defaults to 6, configurable in v0.35.0.
3. Sidebar mouse support — out of scope until post-GA.
4. Dark/light theme behavior on new chrome — assumed unchanged; verify in v0.32.0 snapshots.
5. Whether sidebar `Ctrl+B`-collapsed rail shows live stats — out of scope; rail shows bucket letters only.

---

## 12. Acceptance — overall (Definition of Done for v0.35.0 GA)

- All 7 milestones closed.
- `behavior.new_ux` default `true`.
- Every previous `TuiMode` reachable through new chrome.
- Every wizard runs through `WizardFrame`.
- `Ctrl+P` palette indexes every tool + active session + verb.
- Snapshot suite green at three terminal sizes.
- `docs/user-guide/ux-spine.md` published with screenshots.
- CHANGELOG `v0.35.0` entry approved; landing-snapshot updated alongside per `project_changelog_snapshot_coupling`.
