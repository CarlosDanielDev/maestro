# Review Hub (v0.34.5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans.

**Goal:** Group review tools — PRs / CI Errors / Release Notes — into a single `ReviewHub` with tabs. CI tab uses `GateOutputViewer` as drilldown.

**Architecture:** Same hub pattern. PRs tab is default. CI Errors and Release Notes follow.

**Tech Stack:** ratatui · insta. Depends on v0.32.0 chrome.

**Spec:** Section 3 (Review bucket).

---

### Task 1 (UX-4-01): ReviewHub scaffold + 3 tabs

**Files:**
- Create: `src/tui/screens/review/mod.rs`, `src/tui/screens/review/draw.rs`
- Modify: `src/tui/screens/mod.rs`
- Test: `src/tui/snapshot_tests/review_hub.rs`

- [ ] **Step 1: Failing snapshot test per tab**

```rust
use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};
use crate::tui::screens::review::{ReviewHub, ReviewTab};
use crate::tui::theme::Theme;

fn render(tab: ReviewTab) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(120, 30)).unwrap();
    let theme = Theme::dark();
    let mut hub = ReviewHub::new();
    hub.set_active(tab);
    t.draw(|f| hub.draw_for_test(f, f.area(), &theme)).unwrap();
    t
}

#[test] fn prs_tab() { let t = render(ReviewTab::Prs); assert_snapshot!(t.backend()); }
#[test] fn ci_tab() { let t = render(ReviewTab::Ci); assert_snapshot!(t.backend()); }
#[test] fn releases_tab() { let t = render(ReviewTab::Releases); assert_snapshot!(t.backend()); }
```

Add `pub mod review_hub;` in `src/tui/snapshot_tests/mod.rs`.

- [ ] **Step 2: Confirm fail**

- [ ] **Step 3: Implement skeleton**

`src/tui/screens/review/mod.rs`:

```rust
pub mod draw;

use crossterm::event::{Event, KeyCode};
use ratatui::{Frame, layout::Rect};
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::KeymapProvider;
use crate::tui::screens::{Screen, ScreenAction, TabsModel};
use crate::tui::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReviewTab { Prs, Ci, Releases }

const TABS: &[&str] = &["PRs", "CI Errors", "Release Notes"];

pub struct ReviewHub { pub active: ReviewTab }

impl ReviewHub {
    pub fn new() -> Self { Self { active: ReviewTab::Prs } }
    pub fn set_active(&mut self, t: ReviewTab) { self.active = t; }
    pub fn draw_for_test(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        draw::draw_hub(f, area, theme, self);
    }
}

impl TabsModel for ReviewHub {
    fn tabs(&self) -> &[&'static str] { TABS }
    fn active(&self) -> usize { match self.active {
        ReviewTab::Prs => 0, ReviewTab::Ci => 1, ReviewTab::Releases => 2 } }
    fn select(&mut self, idx: usize) {
        self.active = match idx { 0 => ReviewTab::Prs, 1 => ReviewTab::Ci, _ => ReviewTab::Releases };
    }
}

impl KeymapProvider for ReviewHub {
    fn keymap_bindings(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Tab", "Next tab"), ("Enter", "Open"), ("r", "Refresh")]
    }
}

impl Screen for ReviewHub {
    fn handle_input(&mut self, e: &Event, _m: InputMode) -> ScreenAction {
        if let Event::Key(k) = e {
            if let KeyCode::Char(c @ '1'..='3') = k.code {
                self.select((c as u8 - b'1') as usize);
            }
        }
        ScreenAction::None
    }
    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) { draw::draw_hub(f, area, theme, self); }
    fn tabs(&self) -> Option<&dyn TabsModel> { Some(self) }
    fn breadcrumb(&self) -> &'static str { "Review" }
    fn footer_hints(&self) -> Vec<(&'static str, &'static str)> { self.keymap_bindings() }
}
```

`src/tui/screens/review/draw.rs`:

```rust
use ratatui::{Frame, layout::{Constraint, Direction, Layout, Rect}, style::{Modifier, Style},
              text::{Line, Span}, widgets::Paragraph};
use crate::tui::screens::review::{ReviewHub, ReviewTab, TABS};
use crate::tui::screens::TabsModel;
use crate::tui::theme::Theme;

pub fn draw_hub(f: &mut Frame, area: Rect, theme: &Theme, hub: &ReviewHub) {
    let rows = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)]).split(area);
    let spans: Vec<Span> = TABS.iter().enumerate().flat_map(|(i, l)| {
        let s = if i == hub.active() { Style::default().add_modifier(Modifier::REVERSED) } else { Style::default() };
        vec![Span::styled(format!(" {} ", l), s), Span::raw(" ▎ ")]
    }).collect();
    f.render_widget(Paragraph::new(Line::from(spans)), rows[0]);

    let body = rows[1];
    match hub.active {
        ReviewTab::Prs => f.render_widget(Paragraph::new("[PRs — UX-4-02]"), body),
        ReviewTab::Ci => f.render_widget(Paragraph::new("[CI — UX-4-03]"), body),
        ReviewTab::Releases => f.render_widget(Paragraph::new("[Releases — UX-4-04]"), body),
    }
}
```

Export from `src/tui/screens/mod.rs`.

- [ ] **Step 4: Accept snapshots + commit**

```bash
cargo test --lib tui::snapshot_tests::review_hub
cargo insta accept
git add src/tui/screens/review/ src/tui/screens/mod.rs \
        src/tui/snapshot_tests/review_hub.rs src/tui/snapshot_tests/mod.rs \
        src/tui/snapshot_tests/snapshots/
git commit -m "feat(tui): ReviewHub scaffold + 3 tabs (UX-4-01)"
```

---

### Task 2 (UX-4-02): PrReview body → PRs tab

**Files:**
- Modify: `src/tui/screens/pr_review/`
- Create: `src/tui/screens/review/prs.rs`

- [ ] **Step 1: Failing snapshot for the PRs tab body.**

- [ ] **Step 2: Extract `draw_pr_review_body(f, area, theme, &PrReviewState)` from existing PR review screen.**

- [ ] **Step 3: Wire `ReviewTab::Prs` arm.**

- [ ] **Step 4: Accept + commit**

```bash
git add src/tui/screens/pr_review/ src/tui/screens/review/prs.rs src/tui/screens/review/draw.rs \
        src/tui/snapshot_tests/snapshots/
git commit -m "feat(tui): PRs tab (UX-4-02)"
```

---

### Task 3 (UX-4-03): CiErrorReview body → CI tab; GateOutputViewer drilldown

**Files:**
- Modify: `src/tui/screens/ci_error_review.rs`, `src/tui/screens/gate_output_viewer.rs`
- Create: `src/tui/screens/review/ci.rs`

- [ ] **Step 1: Failing snapshot for CI tab.**

- [ ] **Step 2: Extract `draw_ci_body(f, area, theme, &CiState)`.**

- [ ] **Step 3: On Enter in CI tab, push `GateOutputViewer` as a screen via `ScreenAction::Push(TuiMode::GateOutputViewer(...))`.**

- [ ] **Step 4: Accept + commit**

```bash
git add src/tui/screens/ci_error_review.rs src/tui/screens/gate_output_viewer.rs \
        src/tui/screens/review/ci.rs src/tui/screens/review/draw.rs \
        src/tui/snapshot_tests/snapshots/
git commit -m "feat(tui): CI tab + GateOutputViewer drilldown (UX-4-03)"
```

---

### Task 4 (UX-4-04): ReleaseNotes body → Releases tab

**Files:**
- Modify: `src/tui/screens/release_notes/`
- Create: `src/tui/screens/review/releases.rs`

- [ ] **Step 1: Failing snapshot for Releases tab.**

- [ ] **Step 2: Extract `draw_release_notes_body(f, area, theme, &ReleaseNotesState)`.**

- [ ] **Step 3: Wire `ReviewTab::Releases`.**

- [ ] **Step 4: Accept + commit**

```bash
git add src/tui/screens/release_notes/ src/tui/screens/review/releases.rs src/tui/screens/review/draw.rs \
        src/tui/snapshot_tests/snapshots/
git commit -m "feat(tui): Releases tab (UX-4-04)"
```

---

## Milestone Dependency Graph

```
Level 0:
• UX-4-01 scaffold (depends on v0.32.0 UX-0-11)

Level 1 (parallel):
• UX-4-02 PRs
• UX-4-03 CI
• UX-4-04 Releases

Sequence: UX-4-01 → (UX-4-02 ∥ UX-4-03 ∥ UX-4-04)
```
