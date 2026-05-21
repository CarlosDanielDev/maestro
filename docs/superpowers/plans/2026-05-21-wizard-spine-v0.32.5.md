# Wizard Spine (v0.32.5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace four bespoke wizards (Issue / Milestone / Team / Adapt) with a single `Wizard` + `WizardStep` trait pair plus a shared `WizardFrame` widget that renders step counter, title, body slot, validation banner, and nav footer.

**Architecture:** A single `wizard::WizardFrame` widget draws every wizard in a modal overlay (via Phase-0 modal stack). The `Wizard` trait holds step list + cursor + advance/rewind/cancel/submit. Each `WizardStep` impl owns its body rendering, input handling, validation, and footer hints. The Settings schema-tab field renderer is extracted into a reusable `wizard::fields` module so wizards get the same typed inputs.

**Tech Stack:** Rust 2024 · ratatui · `insta` snapshots. Depends on v0.32.0 modal stack (UX-0-05) and Settings schema renderer.

**Spec:** `docs/superpowers/specs/2026-05-21-tui-ux-redesign-sidebar-ia-design.md` — Section 7.

---

### Task 1 (UX-0a-01): Wizard + WizardStep traits + WizardFrame

**Files:**
- Create: `src/tui/widgets/wizard/mod.rs`, `src/tui/widgets/wizard/frame.rs`
- Modify: `src/tui/widgets/mod.rs`
- Test: `src/tui/widgets/wizard/mod.rs` `#[cfg(test)]`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::Event;

    struct StepA { input: String }
    impl WizardStep for StepA {
        fn header(&self) -> String { "Step 1/2: Name".into() }
        fn render_body(&self, _f: &mut ratatui::Frame, _r: ratatui::layout::Rect, _t: &crate::tui::theme::Theme) {}
        fn handle_input(&mut self, _e: &Event) -> WizardStepAction { WizardStepAction::None }
        fn validate(&self) -> Vec<ValidationError> {
            if self.input.is_empty() { vec![ValidationError::new("name required")] } else { Vec::new() }
        }
        fn footer_hints(&self) -> Vec<(&'static str, &'static str)> { vec![("→", "Next")] }
    }
    struct StepB;
    impl WizardStep for StepB {
        fn header(&self) -> String { "Step 2/2: Confirm".into() }
        fn render_body(&self, _f: &mut ratatui::Frame, _r: ratatui::layout::Rect, _t: &crate::tui::theme::Theme) {}
        fn handle_input(&mut self, _e: &Event) -> WizardStepAction { WizardStepAction::None }
        fn validate(&self) -> Vec<ValidationError> { Vec::new() }
        fn footer_hints(&self) -> Vec<(&'static str, &'static str)> { vec![("Ctrl+Enter", "Submit")] }
    }

    struct Demo { steps: Vec<Box<dyn WizardStep>>, cursor: usize }
    impl Wizard for Demo {
        fn title(&self) -> &'static str { "Demo Wizard" }
        fn steps(&self) -> &[Box<dyn WizardStep>] { &self.steps }
        fn steps_mut(&mut self) -> &mut [Box<dyn WizardStep>] { &mut self.steps }
        fn current_step(&self) -> usize { self.cursor }
        fn set_cursor(&mut self, idx: usize) { self.cursor = idx; }
    }

    #[test]
    fn advance_blocked_by_validation_errors() {
        let steps: Vec<Box<dyn WizardStep>> = vec![
            Box::new(StepA { input: String::new() }),
            Box::new(StepB),
        ];
        let mut w = Demo { steps, cursor: 0 };
        let r = w.advance();
        assert!(matches!(r, WizardResult::BlockedByValidation));
        assert_eq!(w.current_step(), 0);
    }

    #[test]
    fn advance_passes_when_valid() {
        let steps: Vec<Box<dyn WizardStep>> = vec![
            Box::new(StepA { input: "ok".into() }),
            Box::new(StepB),
        ];
        let mut w = Demo { steps, cursor: 0 };
        let r = w.advance();
        assert!(matches!(r, WizardResult::Continue));
        assert_eq!(w.current_step(), 1);
    }

    #[test]
    fn submit_on_final_step() {
        let steps: Vec<Box<dyn WizardStep>> = vec![Box::new(StepB)];
        let mut w = Demo { steps, cursor: 0 };
        let r = w.advance();
        assert!(matches!(r, WizardResult::Done(_)));
    }
}
```

- [ ] **Step 2: Confirm fail**

Run: `cargo test --lib tui::widgets::wizard`
Expected: compile error — `Wizard`, `WizardStep`, `WizardResult`, `ValidationError` undefined.

- [ ] **Step 3: Implement traits**

`src/tui/widgets/wizard/mod.rs`:

```rust
pub mod frame;

use crossterm::event::Event;
use ratatui::{Frame, layout::Rect};
use crate::tui::theme::Theme;
use crate::tui::modal::ModalResult;

#[derive(Debug, Clone)]
pub struct ValidationError { pub message: String }
impl ValidationError { pub fn new(m: &str) -> Self { Self { message: m.into() } } }

pub enum WizardStepAction { None, Submit, Back, Cancel }
pub enum WizardResult { Continue, BlockedByValidation, Done(ModalResult), Cancelled }

pub trait WizardStep {
    fn header(&self) -> String;
    fn render_body(&self, f: &mut Frame, area: Rect, theme: &Theme);
    fn handle_input(&mut self, event: &Event) -> WizardStepAction;
    fn validate(&self) -> Vec<ValidationError>;
    fn footer_hints(&self) -> Vec<(&'static str, &'static str)>;
}

pub trait Wizard {
    fn title(&self) -> &'static str;
    fn steps(&self) -> &[Box<dyn WizardStep>];
    fn steps_mut(&mut self) -> &mut [Box<dyn WizardStep>];
    fn current_step(&self) -> usize;
    fn set_cursor(&mut self, idx: usize);

    fn advance(&mut self) -> WizardResult {
        let idx = self.current_step();
        let errors = self.steps()[idx].validate();
        if !errors.is_empty() { return WizardResult::BlockedByValidation; }
        if idx + 1 < self.steps().len() {
            self.set_cursor(idx + 1);
            WizardResult::Continue
        } else {
            WizardResult::Done(ModalResult::None)
        }
    }
    fn rewind(&mut self) -> WizardResult {
        let idx = self.current_step();
        if idx > 0 { self.set_cursor(idx - 1); WizardResult::Continue } else { WizardResult::Cancelled }
    }
    fn cancel(&mut self) -> WizardResult { WizardResult::Cancelled }
}
```

- [ ] **Step 4: Implement WizardFrame draw**

`src/tui/widgets/wizard/frame.rs`:

```rust
use ratatui::{Frame, layout::{Constraint, Direction, Layout, Rect}, style::{Color, Modifier, Style},
              text::Line, widgets::{Block, Borders, Paragraph}};
use crate::tui::theme::Theme;
use crate::tui::widgets::wizard::{Wizard, ValidationError};

pub fn draw_wizard(f: &mut Frame, area: Rect, theme: &Theme, w: &dyn Wizard) {
    let block = Block::default().borders(Borders::ALL).title(w.title());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let idx = w.current_step();
    let step = &w.steps()[idx];

    let rows = Layout::default().direction(Direction::Vertical).constraints([
        Constraint::Length(1),  // header
        Constraint::Length(1),  // separator
        Constraint::Min(3),     // body
        Constraint::Length(1),  // separator
        Constraint::Length(2),  // validation banner (up to 1 line + sep)
        Constraint::Length(1),  // footer
    ]).split(inner);

    f.render_widget(Paragraph::new(step.header()).style(Style::default().add_modifier(Modifier::BOLD)), rows[0]);
    f.render_widget(Paragraph::new("─".repeat(inner.width as usize)), rows[1]);
    step.render_body(f, rows[2], theme);
    f.render_widget(Paragraph::new("─".repeat(inner.width as usize)), rows[3]);

    let errors = step.validate();
    if !errors.is_empty() {
        let line = format!("⚠ {}", errors[0].message);
        f.render_widget(Paragraph::new(line).style(Style::default().fg(Color::Yellow)), rows[4]);
    }

    let hints = step.footer_hints();
    let footer_line: Vec<ratatui::text::Span> = hints.iter().flat_map(|(k, v)| vec![
        ratatui::text::Span::styled(format!("[{}]", k), Style::default().add_modifier(Modifier::BOLD)),
        ratatui::text::Span::raw(format!(" {} · ", v)),
    ]).collect();
    f.render_widget(Paragraph::new(Line::from(footer_line)), rows[5]);
}
```

Export `pub mod wizard;` from `src/tui/widgets/mod.rs`.

- [ ] **Step 5: Run + commit**

```
cargo test --lib tui::widgets::wizard
git add src/tui/widgets/wizard/ src/tui/widgets/mod.rs
git commit -m "feat(tui): Wizard + WizardStep traits + WizardFrame (UX-0a-01)"
```

---

### Task 2 (UX-0a-02): Reuse Settings schema field renderer in wizard body

**Files:**
- Create: `src/tui/widgets/wizard/fields.rs`
- Modify: `src/tui/widgets/wizard/mod.rs` (`pub mod fields;`)
- Test: `src/tui/widgets/wizard/fields.rs` `#[cfg(test)]`

- [ ] **Step 1: Identify reusable field types**

Read `src/tui/screens/settings/schema_tab/` to find `TextInputField`, `DropdownField`, `ToggleField`, `NumberStepperField`. List them in the file header comment.

- [ ] **Step 2: Write the failing test for SchemaField wrapper**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    use crate::tui::theme::Theme;

    #[test]
    fn schema_text_field_renders() {
        let mut t = Terminal::new(TestBackend::new(40, 3)).unwrap();
        let theme = Theme::dark();
        let mut field = SchemaField::text("Issue title", "fix(thing)");
        t.draw(|f| field.render(f, f.area(), &theme)).unwrap();
        let buf = format!("{:?}", t.backend().buffer());
        assert!(buf.contains("Issue title"));
        assert!(buf.contains("fix(thing)"));
    }
}
```

- [ ] **Step 3: Wrap existing schema field types behind `SchemaField`**

`src/tui/widgets/wizard/fields.rs`:

```rust
use ratatui::{Frame, layout::Rect, widgets::{Block, Borders, Paragraph}};
use crate::tui::theme::Theme;

pub enum SchemaField {
    Text { label: String, value: String },
    Number { label: String, value: i64 },
    Toggle { label: String, value: bool },
    Dropdown { label: String, value: String, options: Vec<String> },
}

impl SchemaField {
    pub fn text(label: &str, initial: &str) -> Self { Self::Text { label: label.into(), value: initial.into() } }
    pub fn number(label: &str, initial: i64) -> Self { Self::Number { label: label.into(), value: initial } }
    pub fn toggle(label: &str, initial: bool) -> Self { Self::Toggle { label: label.into(), value: initial } }
    pub fn dropdown(label: &str, initial: &str, options: Vec<String>) -> Self {
        Self::Dropdown { label: label.into(), value: initial.into(), options }
    }

    pub fn render(&self, f: &mut Frame, area: Rect, _theme: &Theme) {
        match self {
            Self::Text { label, value } => {
                let body = format!(" {}", value);
                let p = Paragraph::new(body).block(Block::default().borders(Borders::ALL).title(label.as_str()));
                f.render_widget(p, area);
            }
            Self::Number { label, value } => {
                let body = format!(" {}", value);
                let p = Paragraph::new(body).block(Block::default().borders(Borders::ALL).title(label.as_str()));
                f.render_widget(p, area);
            }
            Self::Toggle { label, value } => {
                let body = format!(" [{}]", if *value { "x" } else { " " });
                let p = Paragraph::new(body).block(Block::default().borders(Borders::ALL).title(label.as_str()));
                f.render_widget(p, area);
            }
            Self::Dropdown { label, value, options } => {
                let body = format!(" {} (one of {})", value, options.len());
                let p = Paragraph::new(body).block(Block::default().borders(Borders::ALL).title(label.as_str()));
                f.render_widget(p, area);
            }
        }
    }
}
```

- [ ] **Step 4: Run + commit**

```
cargo test --lib tui::widgets::wizard::fields
git add src/tui/widgets/wizard/fields.rs src/tui/widgets/wizard/mod.rs
git commit -m "feat(tui): SchemaField renderer reused in wizards (UX-0a-02)"
```

---

### Task 3 (UX-0a-03): Migrate IssueWizard onto Wizard trait

**Files:**
- Modify: `src/tui/screens/issue_wizard/` (multiple files)
- Test: `src/tui/snapshot_tests/issue_wizard.rs` (replace or add `wizard_frame_*` snapshots)

- [ ] **Step 1: Add failing snapshot test for new wizard frame**

```rust
use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};
use crate::tui::theme::Theme;
use crate::tui::screens::issue_wizard::IssueWizardScreen;
use crate::tui::widgets::wizard::frame::draw_wizard;

#[test]
fn issue_wizard_step1_in_new_frame() {
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let theme = Theme::dark();
    let w = IssueWizardScreen::new();
    t.draw(|f| draw_wizard(f, f.area(), &theme, &w)).unwrap();
    assert_snapshot!(t.backend());
}
```

- [ ] **Step 2: Confirm fail**

Run: `cargo test --lib tui::snapshot_tests::issue_wizard::issue_wizard_step1_in_new_frame`
Expected: compile error — `IssueWizardScreen` doesn't implement `Wizard`.

- [ ] **Step 3: Implement Wizard for IssueWizardScreen**

In `src/tui/screens/issue_wizard/mod.rs`:

```rust
use crate::tui::widgets::wizard::{Wizard, WizardStep};

impl Wizard for IssueWizardScreen {
    fn title(&self) -> &'static str { "Issue Wizard" }
    fn steps(&self) -> &[Box<dyn WizardStep>] { &self.steps }
    fn steps_mut(&mut self) -> &mut [Box<dyn WizardStep>] { &mut self.steps }
    fn current_step(&self) -> usize { self.current_step_idx }
    fn set_cursor(&mut self, idx: usize) { self.current_step_idx = idx; }
}
```

Wrap each existing step page into a struct implementing `WizardStep`. For each existing step in `issue_wizard/steps/`, write an adapter:

```rust
pub struct ClassifyStep { pub kind: String }
impl WizardStep for ClassifyStep {
    fn header(&self) -> String { format!("Step 1/10: Classification") }
    fn render_body(&self, f: &mut ratatui::Frame, area: ratatui::layout::Rect, _t: &crate::tui::theme::Theme) {
        // existing render
    }
    fn handle_input(&mut self, _e: &crossterm::event::Event) -> crate::tui::widgets::wizard::WizardStepAction {
        crate::tui::widgets::wizard::WizardStepAction::None
    }
    fn validate(&self) -> Vec<crate::tui::widgets::wizard::ValidationError> {
        if self.kind.is_empty() { vec![crate::tui::widgets::wizard::ValidationError::new("pick a classification")] }
        else { Vec::new() }
    }
    fn footer_hints(&self) -> Vec<(&'static str, &'static str)> { vec![("→", "Next"), ("Esc", "Cancel")] }
}
```

Replace `IssueWizardScreen::draw` with delegation to `draw_wizard(f, area, theme, self)`.

- [ ] **Step 4: Accept new snapshot + verify legacy snapshots**

```
cargo test --lib tui::snapshot_tests::issue_wizard
cargo insta accept
cargo test --lib tui::snapshot_tests::issue_wizard
```
Expected: new `issue_wizard_step1_in_new_frame` PASS. Old `milestone_wizard`/`team_wizard` snapshots unchanged.

- [ ] **Step 5: Commit**

```bash
git add src/tui/screens/issue_wizard/ src/tui/snapshot_tests/issue_wizard.rs \
        src/tui/snapshot_tests/snapshots/
git commit -m "refactor(tui): migrate IssueWizard onto Wizard trait + WizardFrame (UX-0a-03)"
```

---

### Task 4 (UX-0a-04): Migrate MilestoneWizard onto Wizard trait

**Files:**
- Modify: `src/tui/screens/milestone_wizard/`
- Test: `src/tui/snapshot_tests/milestone_wizard.rs`

Repeat the Task 3 pattern. Specifically:

- [ ] **Step 1: Snapshot test for first step in new frame**

```rust
#[test]
fn milestone_wizard_step1_in_new_frame() {
    let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    let theme = crate::tui::theme::Theme::dark();
    let w = crate::tui::screens::milestone_wizard::MilestoneWizardScreen::new();
    t.draw(|f| crate::tui::widgets::wizard::frame::draw_wizard(f, f.area(), &theme, &w)).unwrap();
    insta::assert_snapshot!(t.backend());
}
```

- [ ] **Step 2: Confirm fail, then implement `Wizard` for `MilestoneWizardScreen`**

Same shape as Task 3 Step 3.

- [ ] **Step 3: Wrap each existing milestone step as `WizardStep`**

For each of the wizard's existing steps, write the adapter struct (header, render_body, handle_input, validate, footer_hints). Use exact existing fields.

- [ ] **Step 4: Accept snapshot**

```
cargo test --lib tui::snapshot_tests::milestone_wizard
cargo insta accept
cargo test --lib tui::snapshot_tests::milestone_wizard
```

- [ ] **Step 5: Commit**

```bash
git add src/tui/screens/milestone_wizard/ src/tui/snapshot_tests/milestone_wizard.rs \
        src/tui/snapshot_tests/snapshots/
git commit -m "refactor(tui): migrate MilestoneWizard onto Wizard trait (UX-0a-04)"
```

---

### Task 5 (UX-0a-05): Migrate TeamWizard onto Wizard trait

**Files:**
- Modify: `src/tui/screens/team_wizard/`
- Test: `src/tui/snapshot_tests/team_wizard.rs`

Same shape as Task 4. Watch for the in-flight #805/#806 issue picker bug — keep current logic, just wrap into a `WizardStep`.

- [ ] **Step 1: Snapshot test in new frame** (same pattern)
- [ ] **Step 2: Implement `Wizard` for `TeamWizardScreen`**
- [ ] **Step 3: Wrap each existing step**
- [ ] **Step 4: Accept**

```
cargo test --lib tui::snapshot_tests::team_wizard
cargo insta accept
cargo test --lib tui::snapshot_tests::team_wizard
```

- [ ] **Step 5: Commit**

```bash
git add src/tui/screens/team_wizard/ src/tui/snapshot_tests/team_wizard.rs \
        src/tui/snapshot_tests/snapshots/
git commit -m "refactor(tui): migrate TeamWizard onto Wizard trait (UX-0a-05)"
```

---

### Task 6 (UX-0a-06): Migrate AdaptWizard onto Wizard trait

**Files:**
- Modify: `src/tui/screens/adapt/`
- Test: new snapshot

- [ ] **Step 1: Snapshot test for first adapt step**

```rust
#[test]
fn adapt_wizard_step1_in_new_frame() {
    let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    let theme = crate::tui::theme::Theme::dark();
    let w = crate::tui::screens::adapt::AdaptScreen::new();
    t.draw(|f| crate::tui::widgets::wizard::frame::draw_wizard(f, f.area(), &theme, &w)).unwrap();
    insta::assert_snapshot!(t.backend());
}
```

Add to `src/tui/snapshot_tests/adapt_wizard.rs` (create file) and `pub mod adapt_wizard;` in `src/tui/snapshot_tests/mod.rs`.

- [ ] **Step 2: Implement Wizard for AdaptScreen** (same shape as previous)
- [ ] **Step 3: Wrap each adapt step** (Source URL, Adapter type, Confirm)
- [ ] **Step 4: Accept**
- [ ] **Step 5: Commit**

```bash
git add src/tui/screens/adapt/ src/tui/snapshot_tests/adapt_wizard.rs \
        src/tui/snapshot_tests/mod.rs src/tui/snapshot_tests/snapshots/
git commit -m "refactor(tui): migrate AdaptWizard onto Wizard trait (UX-0a-06)"
```

---

### Task 7 (UX-0a-07): Retire bespoke wizard scaffolding

**Files:**
- Modify: `src/tui/screens/wizard_fields.rs` — keep only schema-reusable parts (text input, dropdown). Move them under `widgets/wizard/fields.rs`.
- Delete: `src/tui/screens/wizard_fields_tests.rs` once the suites have been re-pointed.

- [ ] **Step 1: Verify no caller references retained**

Run: `rg "use crate::tui::screens::wizard_fields" src/`
Expected: empty (after Tasks 3-6 stopped using the legacy adapters).

- [ ] **Step 2: Delete legacy file**

Remove `src/tui/screens/wizard_fields.rs` and `src/tui/screens/wizard_fields_tests.rs` if all symbols are unused.

- [ ] **Step 3: Remove module declarations**

In `src/tui/screens/mod.rs` drop `pub mod wizard_fields;` and `pub mod wizard_fields_tests;`.

- [ ] **Step 4: Build + test**

Run: `cargo build && cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -u src/tui/screens/
git commit -m "chore(tui): retire wizard_fields legacy scaffolding (UX-0a-07)"
```

---

### Task 8 (UX-0a-08): Snapshot tests for one representative step per wizard

**Files:**
- Modify: existing wizard snapshot files to include one extra "middle step" snapshot per wizard.

- [ ] **Step 1: Add snapshots**

For each wizard, write a test that pre-fills 2-3 steps' state then asserts the rendering of a non-first step. Example:

```rust
#[test]
fn issue_wizard_step5_files_to_modify_in_new_frame() {
    let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    let theme = crate::tui::theme::Theme::dark();
    let mut w = crate::tui::screens::issue_wizard::IssueWizardScreen::new();
    w.set_cursor(4); // Step 5/10 — Files to Modify
    t.draw(|f| crate::tui::widgets::wizard::frame::draw_wizard(f, f.area(), &theme, &w)).unwrap();
    insta::assert_snapshot!(t.backend());
}
```

Repeat for milestone wizard, team wizard, adapt wizard.

- [ ] **Step 2: Accept snapshots**

```
cargo test --lib tui::snapshot_tests::issue_wizard tui::snapshot_tests::milestone_wizard tui::snapshot_tests::team_wizard tui::snapshot_tests::adapt_wizard
cargo insta accept
```

- [ ] **Step 3: Commit**

```bash
git add src/tui/snapshot_tests/ 
git commit -m "test(tui): mid-step snapshots for each wizard (UX-0a-08)"
```

---

## Milestone Dependency Graph

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
