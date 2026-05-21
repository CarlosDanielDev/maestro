# System Hub + GA Flip (v0.35.0) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans.

**Goal:** Land the final hub (Settings + Teams), promote Teams from wizard-only to a first-class list screen with launch modal, then flip `behavior.new_ux` default to `true`. Publish user-facing docs.

**Architecture:** `SystemHub` wraps the already-tabbed `SettingsScreen` body inside one tab and adds a Teams tab built on top of the v0.32.5 wizard spine. Once all five hubs ship, the flag default flips and CHANGELOG/user-guide get published in the same release window.

**Tech Stack:** ratatui · insta · `gh` for milestone update. Depends on v0.32.0..v0.34.5, plus v0.31.0 #821 (ranking surface for Teams) and v0.32.5 UX-0a-05 (TeamWizard).

**Spec:** Section 3 (System bucket), Section 8 (Phase 5).

---

### Task 1 (UX-5-01): SystemHub scaffold + Settings/Teams tabs

**Files:**
- Create: `src/tui/screens/system/mod.rs`, `src/tui/screens/system/draw.rs`
- Modify: `src/tui/screens/mod.rs`
- Test: `src/tui/snapshot_tests/system_hub.rs`

- [ ] **Step 1: Failing snapshot per tab**

```rust
use insta::assert_snapshot;
use ratatui::{Terminal, backend::TestBackend};
use crate::tui::screens::system::{SystemHub, SystemTab};
use crate::tui::theme::Theme;

fn render(tab: SystemTab) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(120, 30)).unwrap();
    let theme = Theme::dark();
    let mut hub = SystemHub::new();
    hub.set_active(tab);
    t.draw(|f| hub.draw_for_test(f, f.area(), &theme)).unwrap();
    t
}

#[test] fn settings_tab() { let t = render(SystemTab::Settings); assert_snapshot!(t.backend()); }
#[test] fn teams_tab() { let t = render(SystemTab::Teams); assert_snapshot!(t.backend()); }
```

Add `pub mod system_hub;` in `src/tui/snapshot_tests/mod.rs`.

- [ ] **Step 2: Confirm fail**

- [ ] **Step 3: Implement skeleton**

`src/tui/screens/system/mod.rs`:

```rust
pub mod draw;

use crossterm::event::{Event, KeyCode};
use ratatui::{Frame, layout::Rect};
use crate::tui::navigation::InputMode;
use crate::tui::navigation::keymap::KeymapProvider;
use crate::tui::screens::{Screen, ScreenAction, TabsModel};
use crate::tui::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SystemTab { Settings, Teams }

const TABS: &[&str] = &["Settings", "Teams"];

pub struct SystemHub { pub active: SystemTab }

impl SystemHub {
    pub fn new() -> Self { Self { active: SystemTab::Settings } }
    pub fn set_active(&mut self, t: SystemTab) { self.active = t; }
    pub fn draw_for_test(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        draw::draw_hub(f, area, theme, self);
    }
}

impl TabsModel for SystemHub {
    fn tabs(&self) -> &[&'static str] { TABS }
    fn active(&self) -> usize { match self.active { SystemTab::Settings => 0, SystemTab::Teams => 1 } }
    fn select(&mut self, idx: usize) {
        self.active = match idx { 0 => SystemTab::Settings, _ => SystemTab::Teams };
    }
}

impl KeymapProvider for SystemHub {
    fn keymap_bindings(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Tab", "Next tab"), ("Enter", "Open"), ("n", "New team")]
    }
}

impl Screen for SystemHub {
    fn handle_input(&mut self, e: &Event, _m: InputMode) -> ScreenAction {
        if let Event::Key(k) = e {
            if let KeyCode::Char(c @ '1'..='2') = k.code {
                self.select((c as u8 - b'1') as usize);
            }
        }
        ScreenAction::None
    }
    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) { draw::draw_hub(f, area, theme, self); }
    fn tabs(&self) -> Option<&dyn TabsModel> { Some(self) }
    fn breadcrumb(&self) -> &'static str { "System" }
    fn footer_hints(&self) -> Vec<(&'static str, &'static str)> { self.keymap_bindings() }
}
```

`src/tui/screens/system/draw.rs`:

```rust
use ratatui::{Frame, layout::{Constraint, Direction, Layout, Rect}, style::{Modifier, Style},
              text::{Line, Span}, widgets::Paragraph};
use crate::tui::screens::system::{SystemHub, SystemTab, TABS};
use crate::tui::screens::TabsModel;
use crate::tui::theme::Theme;

pub fn draw_hub(f: &mut Frame, area: Rect, theme: &Theme, hub: &SystemHub) {
    let rows = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)]).split(area);
    let spans: Vec<Span> = TABS.iter().enumerate().flat_map(|(i, l)| {
        let s = if i == hub.active() { Style::default().add_modifier(Modifier::REVERSED) } else { Style::default() };
        vec![Span::styled(format!(" {} ", l), s), Span::raw(" ▎ ")]
    }).collect();
    f.render_widget(Paragraph::new(Line::from(spans)), rows[0]);

    let body = rows[1];
    match hub.active {
        SystemTab::Settings => f.render_widget(Paragraph::new("[Settings — UX-5-02]"), body),
        SystemTab::Teams => f.render_widget(Paragraph::new("[Teams — UX-5-03]"), body),
    }
}
```

Export from `src/tui/screens/mod.rs`.

- [ ] **Step 4: Accept snapshots + commit**

```bash
cargo test --lib tui::snapshot_tests::system_hub
cargo insta accept
git add src/tui/screens/system/ src/tui/screens/mod.rs \
        src/tui/snapshot_tests/system_hub.rs src/tui/snapshot_tests/mod.rs \
        src/tui/snapshot_tests/snapshots/
git commit -m "feat(tui): SystemHub scaffold + 2 tabs (UX-5-01)"
```

---

### Task 2 (UX-5-02): Settings → SystemHub::Settings (inner tabs preserved)

**Files:**
- Modify: `src/tui/screens/settings/` (extract `draw_settings_body(f, area, theme, &SettingsState)`)
- Create: `src/tui/screens/system/settings.rs`

- [ ] **Step 1: Failing snapshot for Settings tab.**

- [ ] **Step 2: Extract draw body from existing `SettingsScreen::draw`** keeping its internal tab strip — that strip becomes a sub-tab inside the System Settings tab.

- [ ] **Step 3: Wire `SystemTab::Settings` arm.**

- [ ] **Step 4: Verify ALL existing Settings snapshot tests still pass (memory `settings_layout_parity` etc.).**

```
cargo test --lib tui::snapshot_tests::settings
```
Expected: PASS.

- [ ] **Step 5: Accept new snapshot + commit**

```bash
git add src/tui/screens/settings/ src/tui/screens/system/settings.rs src/tui/screens/system/draw.rs \
        src/tui/snapshot_tests/snapshots/
git commit -m "feat(tui): Settings tab inside SystemHub (UX-5-02)"
```

---

### Task 3 (UX-5-03): Teams promoted from wizard-only to list screen + launch modal

**Files:**
- Modify: `src/tui/screens/team_wizard/` (split into `TeamsListScreen` + `TeamWizard` modal)
- Create: `src/tui/screens/system/teams.rs`

- [ ] **Step 1: Verify dependency**

Run: `gh issue view 821 --json state --jq .state`
Expected: `CLOSED`. If `OPEN`, halt — Teams tab depends on ranking surface from v0.31.0 #821.

- [ ] **Step 2: Failing snapshot for the Teams list view**

```rust
#[test]
fn teams_list_shows_existing_teams_with_ranking() {
    use ratatui::{Terminal, backend::TestBackend};
    use crate::tui::screens::system::teams::{TeamRow, draw_teams_list};
    use crate::tui::theme::Theme;
    let mut t = Terminal::new(TestBackend::new(120, 12)).unwrap();
    let theme = Theme::dark();
    let rows = vec![
        TeamRow { name: "default".into(), agents: 4, ranking_score: 0.92 },
        TeamRow { name: "experimental".into(), agents: 2, ranking_score: 0.55 },
    ];
    t.draw(|f| draw_teams_list(f, f.area(), &theme, &rows)).unwrap();
    insta::assert_snapshot!(t.backend());
}
```

- [ ] **Step 3: Implement list view**

`src/tui/screens/system/teams.rs`:

```rust
use ratatui::{Frame, layout::Rect, widgets::{Block, Borders, List, ListItem}};
use crate::tui::theme::Theme;

#[derive(Debug, Clone)]
pub struct TeamRow { pub name: String, pub agents: u32, pub ranking_score: f64 }

pub fn draw_teams_list(f: &mut Frame, area: Rect, _t: &Theme, rows: &[TeamRow]) {
    let items: Vec<ListItem> = rows.iter().map(|r| {
        ListItem::new(format!(" {:20}  agents:{:>3}  rank:{:.2}", r.name, r.agents, r.ranking_score))
    }).collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Teams"));
    f.render_widget(list, area);
}
```

Wire `SystemTab::Teams` arm in `draw.rs` to call `draw_teams_list(..., &hub.teams)`.

Add to `SystemHub`: `pub teams: Vec<teams::TeamRow>` (default empty).

- [ ] **Step 4: `n` key pushes TeamWizard modal**

```rust
if let Event::Key(k) = e {
    if matches!(k.code, KeyCode::Char('n')) && self.active == SystemTab::Teams {
        return ScreenAction::PushTeamWizardModal;
    }
}
```

- [ ] **Step 5: Accept + commit**

```bash
git add src/tui/screens/team_wizard/ src/tui/screens/system/teams.rs src/tui/screens/system/draw.rs \
        src/tui/screens/system/mod.rs src/tui/snapshot_tests/snapshots/
git commit -m "feat(tui): Teams list screen + ranking column + launch modal (UX-5-03)"
```

---

### Task 4 (UX-5-04): Flip behavior.new_ux default to true; CHANGELOG

**Files:**
- Modify: `src/config.rs` (default flips)
- Modify: `CHANGELOG.md`
- Modify: `src/tui/landing.rs` snapshot fixtures may need to refresh (per memory `project_changelog_snapshot_coupling`)

- [ ] **Step 1: Failing test for new default**

In `src/config.rs`:

```rust
#[test]
fn behavior_new_ux_now_defaults_true() {
    let cfg: Config = toml::from_str(r#"
[project]
name = "demo"
"#).unwrap();
    assert!(cfg.behavior.new_ux);
}
```

- [ ] **Step 2: Confirm fail**

Run: `cargo test --lib config::new_ux_flag_tests::behavior_new_ux_now_defaults_true`
Expected: fail — flag still defaults `false`.

- [ ] **Step 3: Flip default**

In `BehaviorConfig::default()`:

```rust
impl Default for BehaviorConfig {
    fn default() -> Self { Self { caveman_mode: false, new_ux: true } }
}
```

- [ ] **Step 4: CHANGELOG entry**

Add to `CHANGELOG.md` under `## [v0.35.0]`:

```markdown
### Changed
- TUI: new sidebar-driven UX is now the default. Set `[behavior] new_ux = false` to opt back into the legacy chrome (will be removed in v0.35.5).
```

- [ ] **Step 5: Refresh landing snapshots**

Per memory `project_changelog_snapshot_coupling`: `CHANGELOG.md` is `include_str!`'d into the TUI landing widget. Run:

```
cargo test --lib tui::snapshot_tests::landing
cargo insta accept
```

- [ ] **Step 6: Commit**

```bash
git add src/config.rs CHANGELOG.md src/tui/snapshot_tests/snapshots/
git commit -m "feat(config): flip behavior.new_ux default to true + CHANGELOG (UX-5-04)"
```

---

### Task 5 (UX-5-05): User guide + screenshots for new spine

**Files:**
- Create: `docs/user-guide/ux-spine.md`
- Create: `docs/screenshots/ux-spine/*.png` (captured from a running TUI)

- [ ] **Step 1: Write the user guide page**

`docs/user-guide/ux-spine.md`:

```markdown
# UX Spine (since v0.35.0)

Maestro reorganized around five buckets:

| Bucket | Hotkey | Tools |
|---|---|---|
| Run | Ctrl+R | Sessions · Interactions · Queue · Adapt · Prompt |
| Plan | Ctrl+N | Issues · Milestones · Roadmap · PRD |
| Review | Ctrl+V | PRs · CI Errors · Release Notes |
| Insights | Ctrl+I | Cost · Tokens · TurboQuant · Agent Graph · Project Stats |
| System | Ctrl+S | Settings · Teams |

Also:
- `Ctrl+H` → Home dashboard
- `Ctrl+P` → fuzzy command palette
- `Ctrl+B` → toggle sidebar between full and narrow rail
- `Ctrl+L` → toggle activity log strip
- `Tab` / `Shift+Tab` → cycle focus between Sidebar, Tabs strip, Content
- `?` → contextual help
- `q` → quit (with confirmation)

Wizards (Issue / Milestone / Team / Adapt) now share a single frame with consistent step counter, validation banner, and footer hints.

To opt back into the legacy chrome temporarily:

```toml
[behavior]
new_ux = false
```

This flag will be removed in v0.35.5.
```

- [ ] **Step 2: Capture screenshots** at three terminal sizes (80×24, 120×40, 200×60) for each bucket landing page. Save under `docs/screenshots/ux-spine/`.

- [ ] **Step 3: Commit**

```bash
git add docs/user-guide/ux-spine.md docs/screenshots/ux-spine/
git commit -m "docs(ux): user guide + screenshots for new sidebar UX (UX-5-05)"
```

---

## Milestone Dependency Graph

```
Level 0:
• UX-5-01 scaffold (depends on v0.32.0 UX-0-11)

Level 1 (parallel):
• UX-5-02 Settings
• UX-5-03 Teams (also depends on v0.31.0 #821 and v0.32.5 UX-0a-05)

Level 2:
• UX-5-04 flag flip (depends on UX-5-01..03 + v0.33.0..v0.34.5 closed)

Level 3:
• UX-5-05 docs (depends on UX-5-04)

Sequence: UX-5-01 → (UX-5-02 ∥ UX-5-03) → UX-5-04 → UX-5-05
```
