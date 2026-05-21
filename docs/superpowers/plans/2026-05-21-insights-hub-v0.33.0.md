# Insights Hub (v0.33.0) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Merge five read-only dashboards (Cost / Tokens / TurboQuant / Agent Graph / Project Stats) into a single tabbed `InsightsHub` screen behind `behavior.new_ux=true`. Smallest blast radius migration phase.

**Architecture:** A new `InsightsHub` screen exposes 5 tabs through the `TabsModel` trait introduced in v0.32.0. Each existing dashboard's draw body is reused — only the outer chrome moves to the hub. Legacy `TuiMode::CostDashboard`/`TokenDashboard`/`TurboquantDashboard`/`AgentGraph`/`DependencyGraph` are marked `#[deprecated]` after migration (removed in v0.35.5).

**Tech Stack:** ratatui · insta. Depends on v0.32.0 chrome + Screen-trait `tabs()`.

**Spec:** `docs/superpowers/specs/2026-05-21-tui-ux-redesign-sidebar-ia-design.md` — Section 3 (Insights bucket), Section 5 (chrome).

---

### Task 1 (UX-1-01): InsightsHub scaffold + 5 tab routes

**Files:**
- Create: `src/tui/screens/insights/mod.rs`, `src/tui/screens/insights/draw.rs`, `src/tui/screens/insights/tabs.rs`
- Modify: `src/tui/screens/mod.rs` (export)
- Test: `src/tui/snapshot_tests/insights_hub.rs`

- [ ] **Step 1: Failing snapshot test**

```rust
use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};
use crate::tui::screens::insights::{InsightsHub, InsightsTab};
use crate::tui::theme::Theme;

fn render(tab: InsightsTab) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(120, 30)).unwrap();
    let theme = Theme::dark();
    let mut hub = InsightsHub::new();
    hub.set_active(tab);
    t.draw(|f| hub.draw_for_test(f, f.area(), &theme)).unwrap();
    t
}

#[test] fn cost_tab() { let t = render(InsightsTab::Cost); assert_snapshot!(t.backend()); }
#[test] fn tokens_tab() { let t = render(InsightsTab::Tokens); assert_snapshot!(t.backend()); }
#[test] fn turboquant_tab() { let t = render(InsightsTab::Turboquant); assert_snapshot!(t.backend()); }
#[test] fn agents_tab() { let t = render(InsightsTab::Agents); assert_snapshot!(t.backend()); }
#[test] fn stats_tab() { let t = render(InsightsTab::Stats); assert_snapshot!(t.backend()); }
```

Add `pub mod insights_hub;` to `src/tui/snapshot_tests/mod.rs`.

- [ ] **Step 2: Confirm fail**

Run: `cargo test --lib tui::snapshot_tests::insights_hub`
Expected: compile error.

- [ ] **Step 3: Implement Hub skeleton with empty bodies**

`src/tui/screens/insights/mod.rs`:

```rust
pub mod draw;
pub mod tabs;

use crossterm::event::{Event, KeyCode};
use ratatui::{Frame, layout::Rect};
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::KeymapProvider;
use crate::tui::screens::{Screen, ScreenAction, TabsModel};
use crate::tui::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InsightsTab { Cost, Tokens, Turboquant, Agents, Stats }

const TABS: &[&str] = &["Cost", "Tokens", "TurboQuant", "Agent Graph", "Project Stats"];

pub struct InsightsHub { pub active: InsightsTab }

impl InsightsHub {
    pub fn new() -> Self { Self { active: InsightsTab::Cost } }
    pub fn set_active(&mut self, t: InsightsTab) { self.active = t; }
    pub fn draw_for_test(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        draw::draw_hub(f, area, theme, self);
    }
}

impl TabsModel for InsightsHub {
    fn tabs(&self) -> &[&'static str] { TABS }
    fn active(&self) -> usize {
        match self.active {
            InsightsTab::Cost => 0, InsightsTab::Tokens => 1, InsightsTab::Turboquant => 2,
            InsightsTab::Agents => 3, InsightsTab::Stats => 4,
        }
    }
    fn select(&mut self, idx: usize) {
        self.active = match idx { 0 => InsightsTab::Cost, 1 => InsightsTab::Tokens,
            2 => InsightsTab::Turboquant, 3 => InsightsTab::Agents, _ => InsightsTab::Stats };
    }
}

impl KeymapProvider for InsightsHub {
    fn keymap_bindings(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Tab", "Next tab"), ("Shift+Tab", "Prev tab"), ("1..5", "Jump tab")]
    }
}

impl Screen for InsightsHub {
    fn handle_input(&mut self, e: &Event, _m: InputMode) -> ScreenAction {
        if let Event::Key(k) = e {
            if let KeyCode::Char(c @ '1'..='5') = k.code {
                self.select((c as u8 - b'1') as usize);
            }
        }
        ScreenAction::None
    }
    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) { draw::draw_hub(f, area, theme, self); }
    fn tabs(&self) -> Option<&dyn TabsModel> { Some(self) }
    fn breadcrumb(&self) -> &'static str { "Insights" }
    fn footer_hints(&self) -> Vec<(&'static str, &'static str)> { self.keymap_bindings() }
}
```

`src/tui/screens/insights/draw.rs`:

```rust
use ratatui::{Frame, layout::{Constraint, Direction, Layout, Rect}, style::{Modifier, Style},
              text::{Line, Span}, widgets::Paragraph};
use crate::tui::screens::insights::{InsightsHub, InsightsTab, TABS};
use crate::tui::screens::TabsModel;
use crate::tui::theme::Theme;

pub fn draw_hub(f: &mut Frame, area: Rect, theme: &Theme, hub: &InsightsHub) {
    let rows = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)]).split(area);
    draw_tabs_strip(f, rows[0], hub.active(), theme);
    let body = rows[1];
    match hub.active {
        InsightsTab::Cost => f.render_widget(Paragraph::new("[Cost tab — body filled by UX-1-02]"), body),
        InsightsTab::Tokens => f.render_widget(Paragraph::new("[Tokens tab — body filled by UX-1-03]"), body),
        InsightsTab::Turboquant => f.render_widget(Paragraph::new("[TurboQuant tab — body filled by UX-1-04]"), body),
        InsightsTab::Agents => f.render_widget(Paragraph::new("[Agents tab — body filled by UX-1-05]"), body),
        InsightsTab::Stats => f.render_widget(Paragraph::new("[Stats tab — body filled by UX-1-06]"), body),
    }
}

fn draw_tabs_strip(f: &mut Frame, area: Rect, active_idx: usize, _theme: &Theme) {
    let spans: Vec<Span> = TABS.iter().enumerate().flat_map(|(i, label)| {
        let style = if i == active_idx { Style::default().add_modifier(Modifier::REVERSED) } else { Style::default() };
        vec![Span::styled(format!(" {} ", label), style), Span::raw(" ▎ ")]
    }).collect();
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
```

Export from `src/tui/screens/mod.rs`: `pub mod insights;` + `pub use insights::InsightsHub;`.

- [ ] **Step 4: Accept snapshots**

```
cargo test --lib tui::snapshot_tests::insights_hub
cargo insta accept
cargo test --lib tui::snapshot_tests::insights_hub
```

- [ ] **Step 5: Commit**

```bash
git add src/tui/screens/insights/ src/tui/screens/mod.rs \
        src/tui/snapshot_tests/insights_hub.rs src/tui/snapshot_tests/mod.rs \
        src/tui/snapshot_tests/snapshots/
git commit -m "feat(tui): InsightsHub scaffold + 5 tabs (UX-1-01)"
```

---

### Task 2 (UX-1-02): Migrate CostDashboard body → Cost tab

**Files:**
- Modify: `src/tui/cost_dashboard.rs` (extract draw body into a reusable `fn draw_cost_body(f, area, theme, &CostState)`)
- Create: `src/tui/screens/insights/cost.rs` (calls `draw_cost_body`)
- Modify: `src/tui/screens/insights/draw.rs` (Cost arm calls into new module)

- [ ] **Step 1: Locate current draw body**

Run: `grep -n "fn draw" src/tui/cost_dashboard.rs` to find the existing draw entry point.

- [ ] **Step 2: Extract `draw_cost_body`**

In `src/tui/cost_dashboard.rs`, extract the inner body into a free fn:

```rust
pub fn draw_cost_body(f: &mut ratatui::Frame, area: ratatui::layout::Rect, theme: &crate::tui::theme::Theme, state: &CostState) {
    // existing body moved here verbatim
}
```

`src/tui/screens/insights/cost.rs`:

```rust
use ratatui::{Frame, layout::Rect};
use crate::tui::cost_dashboard::{CostState, draw_cost_body};
use crate::tui::theme::Theme;

pub fn draw(f: &mut Frame, area: Rect, theme: &Theme, state: &CostState) {
    draw_cost_body(f, area, theme, state);
}
```

In `src/tui/screens/insights/draw.rs` Cost arm:

```rust
InsightsTab::Cost => crate::tui::screens::insights::cost::draw(f, body, theme, &app_cost_state()),
```

(For test purposes, accept a `CostState::default()` injected on the hub.)

- [ ] **Step 3: Update hub to carry state**

Add to `InsightsHub`: `pub cost_state: CostState` (uses Default impl on `CostState`).

- [ ] **Step 4: Refresh snapshot**

```
cargo test --lib tui::snapshot_tests::insights_hub::cost_tab
cargo insta accept
```

- [ ] **Step 5: Commit**

```bash
git add src/tui/cost_dashboard.rs src/tui/screens/insights/ \
        src/tui/snapshot_tests/snapshots/
git commit -m "refactor(tui): CostDashboard body → InsightsHub::Cost (UX-1-02)"
```

---

### Task 3 (UX-1-03): Migrate TokenDashboard body → Tokens tab

Same shape as Task 2. Apply to `src/tui/token_dashboard.rs`.

- [ ] **Step 1:** Locate draw body in `token_dashboard.rs`.
- [ ] **Step 2:** Extract `draw_token_body(f, area, theme, &TokenState)`.
- [ ] **Step 3:** Create `src/tui/screens/insights/tokens.rs` and wire from `draw.rs` Tokens arm.
- [ ] **Step 4:** Refresh snapshot:

```
cargo test --lib tui::snapshot_tests::insights_hub::tokens_tab
cargo insta accept
```

- [ ] **Step 5:** Commit:

```bash
git add src/tui/token_dashboard.rs src/tui/screens/insights/ \
        src/tui/snapshot_tests/snapshots/
git commit -m "refactor(tui): TokenDashboard body → InsightsHub::Tokens (UX-1-03)"
```

---

### Task 4 (UX-1-04): Migrate TurboquantDashboard body → TurboQuant tab

Same shape. Apply to `src/tui/turboquant_dashboard.rs`.

- [ ] **Step 1:** Locate draw body.
- [ ] **Step 2:** Extract `draw_turboquant_body(f, area, theme, &TurboquantState)`.
- [ ] **Step 3:** Create `src/tui/screens/insights/turboquant.rs` and wire.
- [ ] **Step 4:** Refresh snapshot:

```
cargo test --lib tui::snapshot_tests::insights_hub::turboquant_tab
cargo insta accept
```

- [ ] **Step 5:** Commit:

```bash
git add src/tui/turboquant_dashboard.rs src/tui/screens/insights/ \
        src/tui/snapshot_tests/snapshots/
git commit -m "refactor(tui): TurboquantDashboard body → InsightsHub::TurboQuant (UX-1-04)"
```

---

### Task 5 (UX-1-05): Merge AgentGraph + DependencyGraph → Agents tab with sub-toggle

**Files:**
- Modify: `src/tui/agent_graph/`, `src/tui/dep_graph.rs`
- Create: `src/tui/screens/insights/agents.rs`

- [ ] **Step 1: Failing test for the sub-toggle**

```rust
#[test]
fn agents_tab_subtoggle_switches_view() {
    use crate::tui::screens::insights::{InsightsHub, InsightsTab};
    use crate::tui::screens::insights::agents::AgentSubview;
    let mut hub = InsightsHub::new();
    hub.set_active(InsightsTab::Agents);
    hub.agents_subview = AgentSubview::Bipartite;
    assert_eq!(hub.agents_subview, AgentSubview::Bipartite);
    hub.agents_subview = AgentSubview::Dependency;
    assert_eq!(hub.agents_subview, AgentSubview::Dependency);
}
```

- [ ] **Step 2: Add sub-toggle state**

`src/tui/screens/insights/agents.rs`:

```rust
use ratatui::{Frame, layout::Rect};
use crate::tui::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AgentSubview { Bipartite, Dependency }

pub fn draw(f: &mut Frame, area: Rect, theme: &Theme, sub: AgentSubview) {
    match sub {
        AgentSubview::Bipartite => crate::tui::agent_graph::draw_bipartite(f, area, theme),
        AgentSubview::Dependency => crate::tui::dep_graph::draw_dep_graph(f, area, theme),
    }
}
```

Add to `InsightsHub`: `pub agents_subview: AgentSubview` (default Bipartite).

In `draw.rs` Agents arm: `crate::tui::screens::insights::agents::draw(f, body, theme, hub.agents_subview);`

- [ ] **Step 3: Extract `draw_bipartite` and `draw_dep_graph`** as free fns from existing screens.

- [ ] **Step 4: Refresh snapshot**

```
cargo test --lib tui::snapshot_tests::insights_hub::agents_tab
cargo insta accept
```

- [ ] **Step 5: Commit**

```bash
git add src/tui/agent_graph/ src/tui/dep_graph.rs src/tui/screens/insights/agents.rs \
        src/tui/screens/insights/ src/tui/snapshot_tests/snapshots/
git commit -m "refactor(tui): AgentGraph + DependencyGraph → Agents tab with sub-toggle (UX-1-05)"
```

---

### Task 6 (UX-1-06): Migrate ProjectStats body → Stats tab

Same shape as Task 2.

- [ ] **Step 1:** Locate draw body in `src/tui/screens/project_stats/`.
- [ ] **Step 2:** Extract `draw_project_stats_body`.
- [ ] **Step 3:** Create `src/tui/screens/insights/stats.rs` and wire.
- [ ] **Step 4:** Refresh snapshot:

```
cargo test --lib tui::snapshot_tests::insights_hub::stats_tab
cargo insta accept
```

- [ ] **Step 5:** Commit:

```bash
git add src/tui/screens/project_stats/ src/tui/screens/insights/ \
        src/tui/snapshot_tests/snapshots/
git commit -m "refactor(tui): ProjectStats → InsightsHub::Stats (UX-1-06)"
```

---

### Task 7 (UX-1-07): Mark 5 legacy TuiMode variants `#[deprecated]`

**Files:**
- Modify: `src/tui/app/types.rs`

- [ ] **Step 1: Add deprecation annotations**

```rust
pub enum TuiMode {
    // ... other variants ...
    #[deprecated(note = "Use InsightsHub via behavior.new_ux=true")]
    CostDashboard,
    #[deprecated(note = "Use InsightsHub via behavior.new_ux=true")]
    TokenDashboard,
    #[deprecated(note = "Use InsightsHub via behavior.new_ux=true")]
    TurboquantDashboard,
    #[deprecated(note = "Use InsightsHub via behavior.new_ux=true")]
    AgentGraph,
    #[deprecated(note = "Use InsightsHub via behavior.new_ux=true")]
    DependencyGraph,
    // ...
}
```

- [ ] **Step 2: Add `#[allow(deprecated)]` where legacy dispatcher still uses them**

In `src/tui/ui.rs` (legacy branch) and `src/tui/screen_dispatch.rs`, add `#[allow(deprecated)]` at function-level where these variants are still matched.

- [ ] **Step 3: Build clean (no new warnings under `cargo build`)**

Run: `cargo build 2>&1 | rg "deprecated"`
Expected: 0 unexpected hits.

- [ ] **Step 4: Commit**

```bash
git add src/tui/app/types.rs src/tui/ui.rs src/tui/screen_dispatch.rs
git commit -m "chore(tui): deprecate 5 legacy insights TuiMode variants (UX-1-07)"
```

---

## Milestone Dependency Graph

```
Level 0:
• UX-1-01 scaffold (depends on v0.32.0 UX-0-11)

Level 1 (parallel):
• UX-1-02 Cost
• UX-1-03 Tokens
• UX-1-04 TurboQuant
• UX-1-05 Agents
• UX-1-06 Stats

Level 2:
• UX-1-07 deprecate (depends on Level 1)

Sequence: UX-1-01 → (UX-1-02 ∥ UX-1-03 ∥ UX-1-04 ∥ UX-1-05 ∥ UX-1-06) → UX-1-07
```
