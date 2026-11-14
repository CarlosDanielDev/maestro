# TUI Design System

Reference for anyone (human or AI agent) changing the maestro TUI.
**Rule zero: reuse before you build.** Every section below lists what already
exists. If a need is covered here, use it. If not, check `src/tui/` once more,
then build the smallest new piece and add it to this doc in the same PR.

This doc is linked to the code. `design_system_doc_names_exist_in_code` in
`tests/tui_design_system.rs` fails CI when a name or path cited here in
backticks no longer exists in `src/`. Rename or delete something? Update this
doc too.

Stack: ratatui + crossterm + tokio. All paths are relative to the repo root.

---

## 1. Theme — `src/tui/theme.rs`

Every color comes from `&Theme`. Screens receive it in `draw(f, area, theme)`.
Never write `Color::Rgb(..)` / `Color::Red` in a screen — the theme handles
presets (`dark`, `light`, `retro`), user overrides from `maestro.toml`
(`ThemeConfig { preset, overrides }`), and terminal downgrade
(`ColorCapability::{Basic, Extended, TrueColor}` via `apply_capability`).

### Color tokens (fields on `Theme`)

| Group | Fields | Use for |
|---|---|---|
| Branding | `branding_fg`, `branding_bg` | logo / header brand only |
| Text | `text_primary`, `text_secondary`, `text_muted` | body, labels, hints/disabled |
| Borders | `border_active`, `border_inactive`, `border_focused` | use via `styled_block`, not directly |
| Session status | `status_running`, `status_completed`, `status_errored`, `status_paused`, `status_killed`, `status_queued`, `status_spawning`, `status_stalled`, `status_retrying`, `status_gates_running`, `status_needs_review`, `status_ci_fix` | use via `status_color(status)` |
| Semantic | `accent_success`, `accent_warning`, `accent_error`, `accent_info`, `accent_identifier` | ok / warn / fail / info / issue & PR numbers |
| Gauges | `gauge_low`, `gauge_medium`, `gauge_high`, `gauge_background` | use via gauge helpers |
| Notifications | `notification_critical`, `notification_blocker`, `notification_default` | banners, toasts |
| Keybinds | `keybind_key`, `keybind_label_bg`, `keybind_label_fg` | help / hint bars |
| Selection | `selection_bg`, `selection_fg` | focused list row |
| Titles | `title_accent` | block titles (set by `styled_block`) |
| F-key badges | `fkey_badge_bg`, `fkey_badge_fg` | F-key bar |

Adding a token means touching: `Theme`, `ThemeOverrides`, the three presets,
`from_config`, and `apply_capability`. Prefer an existing token first.

### Helper methods

| Method | Returns | Notes |
|---|---|---|
| `theme.styled_block(title, focused)` | `Block` | **Default panel.** Double border, centered `[ Title ]` in `title_accent`, border color by focus |
| `theme.styled_block_plain(focused)` | `Block` | Same, no title |
| `theme.stats_block()` | `Block` | Top + bottom border only (stats rows) |
| `theme.status_color(SessionStatus)` | `Color` | Session state color |
| `theme.gauge_color(pct)` | `Color` | Context / usage gauge (high = bad) |
| `theme.compact_gauge_color(pct)` | `Color` | 4-tier variant: green <50, yellow 50–70, orange 70–85, red >85 |
| `theme.milestone_gauge_color(pct)` | `Color` | Inverted: high completion = green |
| `theme.budget_color(u8)` | `Color` | `>= 90` error, else warning |
| `widgets::focused_selection_style(theme)` | `Style` | Focused row: selection colors + bold + reversed (crate-private) |

Enforced by `screens_use_theme_colors_only` in `tests/tui_design_system.rs`:
any `Color::` (except `Color::Reset`) in a non-test screen file fails CI.
Known debt is allowlisted there (`pr_review/concerns.rs`,
`issue_wizard/draw_diff.rs`). Don't copy it. The allowlist can only shrink:
fix a file, then remove it from the list.

---

## 2. Widgets — `src/tui/widgets/`

Enforced by `every_widget_is_documented`: every module declared in
`widgets/mod.rs` must be cited here in backticks (its type, `module::item`,
or `widgets/module`; a plain word in prose does not count). A new widget
fails CI until it is added to this section, so check first that no widget
below already does the job.

### Form widgets (settings-style forms)

All share `handle_input(key) -> WidgetAction` and
`draw(f, area, theme, focused[, validation])`. Wrap them in `WidgetKind` to
get enum dispatch, `edit_hint()` for the keybind bar, and
`needs_insert_mode()`.

| Widget | Constructor | For |
|---|---|---|
| `Toggle` | `Toggle::new(label, bool)` | on/off (Space) |
| `TextInput` | `TextInput::new(label, value)`, `.with_read_only()` | short single-line setting (Enter to edit) |
| `NumberStepper` | `NumberStepper::new(label, value, min, max)`, `.with_step(n)`, `.with_display_divisor(Some(d))` | bounded number (←/→) |
| `Dropdown` | `Dropdown::new(label, options, selected)`, `.selected_value()` | pick one of N (←/→) |
| `ListEditor` | `ListEditor::new(label, items)` | edit a `Vec<String>` |

`WidgetAction`: `None`, `Changed`, `RequestInsertMode`, `RequestNormalMode`.

### Text input — read this before adding any text field

| Option | Where | Status |
|---|---|---|
| **`Input`** (canonical) | `widgets/input/` — `Input::single_line(cfg)`, `Input::multi_line(cfg)`, `InputConfig::new().placeholder(..).max_len(n).initial(..)`; render with `widgets::input::render` | Foundation landed (#963). Not yet adopted by any screen; migrations #964–#974 pending. **Use this for new fields.** |
| `wizard_fields::TextAreaField` + `WizardFields` | `src/tui/screens/wizard_fields.rs` | Used by wizards (focus cycle, `insert_sanitized`) |
| Raw `tui_textarea::TextArea` + `screens::wrap` | pattern in `src/tui/screens/prompt_input/draw.rs` | Rich prompt / interaction input |

**Never** hand-roll a `String` + cursor. It caused three bugs in a row in #733
(cursor drift on wrap, no scroll, no arrow keys). For wrapping and scroll use
`screens::wrap::{wrap_lines, scroll_offset_for_cursor}`, then place the cursor
with `f.set_cursor_position`.

Enforced by `no_new_hand_rolled_text_input`: a file outside `widgets/input/`
that matches `KeyCode::Char(c)` and pushes or inserts `c` into a string fails
CI. The 13 existing sites are in `HAND_ROLLED_INPUT_ALLOWLIST`. Migrating one
to `Input` means removing it from that list (stale entries fail too). Known
gap: input sent through a helper method (`push_char` in `help.rs`) is not
caught, so review still applies.

Reuse with text input: `clipboard.rs` (copy/paste, image attach),
`issue_refs.rs` (`#NNN` coloring).

### Display widgets

| Widget / fn | For |
|---|---|
| `EmptyState::idle(title, message, hint).render(f, area, theme)` | Empty panel. `hint` must say what to do: `"Press [n] to add one"` |
| `EmptyState::loading(title, message, tick)` | Panel while data loads (braille spinner) |
| `WizardFrame::draw(f, area, theme, WizardFrameHeader{..}, WizardFrameFooter{..}, \|f, body\| ..)` | Multi-step wizard: `Step i/n: label` header, body, footer with validation error + hints. Default hints: `wizard_frame::FOOTER_HINTS` |
| `BrailleSpinner` | Inline spinner |
| `StatsBar::new(StatsBarData, theme)` | Top stats row with marquee overflow |
| `CiMonitorWidget::new(checks, theme).pr_number(n).max_visible_rows(n)` | CI check list |
| `unified_pr_toggle::{draw_unified_pr_toggle, draw_checkbox, focus_bar}` | Checkbox row + focus bar |
| `bypass_indicator::{draw, render_line}` | Header badge when bypass mode is on |

### Other shared helpers (`src/tui/`)

| Module | Item | For |
|---|---|---|
| `screens/mod.rs` | `draw_keybinds_bar(f, area, &[(key, label)], theme)` | Simple `[key] label` bottom bar |
| `keybinding_hints.rs` | `keybinding_hints_spans(fitted, copy_enabled, theme)` | Hint bar with dimmed inactive keys; pre-fit with `navigation::keymap::fit_hints_to_width` |
| `screens/mod.rs` | `sanitize_for_terminal(s)` | **Always** on text from GitHub / git / AI output before rendering |
| `icons.rs` | `icons::get(IconId::..)` | Nerd Font or ASCII icon (respects `tui.ascii_icons` / `MAESTRO_ASCII_ICONS=1`). Never paste raw glyphs |
| `spinner.rs` | `spinner_frame`, `tool_frame`, `spawning_frame`, `idle_pulse`, `animated_activity`, `thinking_activity` | Animation frames from the app tick |
| `marquee.rs` | `MarqueeState`, `visible_slice`, `needs_scroll` | Scroll text that overflows |
| `breadcrumb.rs` | `elide_breadcrumbs(labels, max_width, sep)` | Fit breadcrumb to width |
| `markdown.rs` | `render_markdown(input, theme, width)` | Markdown → `Text` |
| `clipboard_toast.rs` | `draw_copy_toast` | "Copied" confirmation |
| `budget_banner.rs` | `draw_budget_banner_if_alerting` | Budget alert banner |
| `agent_badge.rs` | `agent_color`, `agent_label` | Per-agent color/label |

---

## 3. Screens — `src/tui/screens/`

### Contract

```rust
pub trait Screen: KeymapProvider {
    fn handle_input(&mut self, event: &Event, mode: InputMode) -> ScreenAction;
    fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme);
    fn desired_input_mode(&self) -> Option<InputMode> { None }
}
pub trait KeymapProvider { fn keybindings(&self) -> Vec<KeyBindingGroup>; }
```

Screens never mutate the app. They return a `ScreenAction`
(`None`, `Push(TuiMode)`, `Pop`, `LaunchSession(..)`, `UpdateConfig(..)`, …
see `screens/mod.rs`). Add a new variant only when no existing one fits.

Big screens are folders: `mod.rs` (state + `impl Screen`), `draw.rs`,
`input.rs`, tests. Keep files under 500 lines (see `docs/RUST-GUARDRAILS.md`).

### Adding a new screen — wiring checklist

The compiler catches most of these, one file at a time. **Two do not.**

1. `screens/<name>/mod.rs` — struct, `impl Screen`, `impl KeymapProvider`.
2. `screens/mod.rs` — `pub mod <name>;` + `pub use`.
3. `app/types.rs` — `TuiMode::<Variant>`, `breadcrumb_label` arm, `ScreenState.<name>_screen: Option<..>`, add variant to `all_variants_return_non_empty_breadcrumb_label` test.
4. `screen_dispatch.rs` — input arm, `ScreenAction::Push` construct arm, `Pop` teardown arm.
5. `ui.rs` — the per-variant **draw arm** in `draw()`, and the `&dyn Screen` match lower down. ⚠️ `draw()` has a `_` fallback: missing arm = screen renders **blank**, build still passes, snapshot tests still pass.
6. `navigation/mode_hints.rs` — `mode_keymap` arm `(label, FKeyVis, &[InlineHint])`.
7. `snapshot_tests/mod.rs` — `mod <name>;`. ⚠️ An unregistered test file compiles and runs **zero** tests.

### Keybindings

Handled **before** any screen by `handle_global_shortcuts` in
`src/tui/input_handler.rs`. A screen binding these never sees the key:

| Key | Action | Note |
|---|---|---|
| `Ctrl+Q` | Toggle TurboQuant | always |
| `Ctrl+B` | Toggle bypass mode | always |
| `Ctrl+X` | Exit | always |
| `F1`, `?` | Help overlay | `?` skipped in text-input mode |
| `F2`…`F12` | Mode F-key table (`mode_keymap`) | consumed even if inactive |
| `Shift+Q` | TurboQuant A/B dashboard | skipped in text-input mode |
| `Shift+M` | Session-complete modal | skipped in text-input mode |
| `Shift+C` | Auto-review concerns panel | when pending |
| `Shift+P` | Retry PR creation | skipped in text-input mode |

Before adding a key: grep `handle_global_shortcuts`. In text-input screens
plain letters are typed, so use a free `Ctrl+letter` (e.g. `Ctrl+W`) and
catch it before forwarding to the editor. Declare every key in
`keybindings()` so it shows in help.

Conventions: `j/k` or arrows move, `Enter` open/confirm, `Esc` back,
`Tab` cycle focus, `Space` toggle.

---

## 4. UX rules

- **Visible feedback for async work.** Show it on the screen, not only in the
  activity log: `⟳ Syncing…` while running, `✓ Synced Ns ago` (keep 5–10 s),
  `⚠ Failed: <reason>`. Over ~200 ms → spinner or `EmptyState::loading`.
- **Empty states say what to do**: key + outcome, never just `(empty)`.
- **First entry** to a feature screen: one-line "what is this" intro.
- **Destructive actions** need a confirm step (see `queue_confirmation.rs`,
  `bypass_warning.rs`).
- **Small terminals**: test narrow sizes; fit hints with
  `fit_hints_to_width`, elide breadcrumbs, marquee long text.
- **ASCII mode** must work: icons only through `icons::get`.

---

## 5. Testing TUI changes

Snapshot tests with `insta` + `ratatui::backend::TestBackend` live in
`src/tui/snapshot_tests/` (helpers: `test_terminal()`, `make_session(..)`,
`make_gh_issue(..)` in `snapshot_tests/mod.rs`).

```rust
let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
let theme = Theme::dark();
terminal.draw(|f| screen.draw(f, f.area(), &theme)).unwrap();
insta::assert_snapshot!(terminal.backend());
```

- New snapshots: `INSTA_UPDATE=always cargo test <name>`, review, commit the `.snap`.
- Verify gate: `INSTA_UPDATE=no cargo test`. Delete any `*.snap.new`.
- Snapshots call `screen.draw()` directly — they do **not** prove the
  `ui.rs` draw arm exists. Run the app once for a new screen.
- Editing the current version's `### Added` bullets in `CHANGELOG.md` breaks
  the `landing_welcome_*` snapshots (the landing screen `include_str!`s it).

---

## 6. Before you finish a TUI change

- [ ] Colors from `Theme` only; panels from `styled_block`.
- [ ] Reused an existing widget/helper from §2 (or added the new one to this doc).
- [ ] Text input built on `Input` / `TextArea`, not a `String`.
- [ ] External text through `sanitize_for_terminal`.
- [ ] No clash with global keys; new keys in `keybindings()`.
- [ ] Empty, loading, and error states are visible and actionable.
- [ ] New screen: all 7 wiring steps, incl. `ui.rs` draw arm and snapshot `mod`.
- [ ] `cargo fmt --check && cargo clippy -- -D warnings -A dead_code && cargo test` green.
