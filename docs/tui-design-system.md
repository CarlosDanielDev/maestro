# TUI Design System

Reference for anyone (human or AI agent) changing the maestro TUI.
**Rule zero: reuse before you build.** Every section below lists what already
exists. If a need is covered here, use it. If not, check `crates/maestro-tui/src/tui/` once more,
then build the smallest new piece and add it to this doc in the same PR.

This doc is linked to the code. `design_system_doc_names_exist_in_code` in
`tests/tui_design_system.rs` fails CI when a name or path cited here in
backticks no longer exists in `src/`, `crates/` or `tests/`. Rename or delete something? Update this
doc too.

Stack: ratatui + crossterm + tokio. All paths are relative to the repo root.

Visual reference: [design-system/index.html](design-system/index.html) is the
full design system as one HTML page (screen catalog, flow map, color, keymap,
poka-yoke, rollout). Open it in a browser. This doc stays the source of truth
for code names; the page shows how the screens look. How the target design,
this doc and the decisions log fit together: [design-system/README.md](design-system/README.md).

---

## 1. Theme — `crates/maestro-tui/src/tui/theme.rs`

Every color comes from `&Theme`. Screens receive it in `draw(f, area, theme)`.
Never write `Color::Rgb(..)` / `Color::Red` in a screen — the theme handles
presets (`dark`, `light`, `retro`, `high-contrast`, `no-color`), user overrides
from `maestro.toml` (`ThemeConfig { preset, overrides }`), and terminal downgrade
(`ColorCapability::{Basic, Extended, TrueColor}` via `apply_capability`).

Build the startup theme with `Theme::detect(&config.tui.theme)`: `NO_COLOR`
(non-empty) or `TERM=dumb` forces `Theme::no_color()` (every colour `Reset`,
overrides skipped); otherwise preset + overrides + `apply_capability`. Under
`no-color`, meaning must come from modifiers, border type and glyphs — never
from colour alone.

### Color tokens (fields on `Theme`)

| Group | Fields | Use for |
|---|---|---|
| Branding | `branding_fg`, `branding_bg` | logo / header brand only |
| Text | `text_primary`, `text_secondary`, `text_muted` | body, labels, hints/disabled |
| Borders | `border_active`, `border_inactive` | use via `styled_block`, not directly. `border_active` is the only focus colour |
| Session status | `status_running`, `status_completed`, `status_errored`, `status_paused`, `status_killed`, `status_queued`, `status_spawning`, `status_stalled`, `status_retrying`, `status_gates_running`, `status_needs_review`, `status_ci_fix` | use via `status_color(status)` |
| Semantic | `accent_success`, `accent_warning`, `accent_error`, `accent_info`, `accent_identifier` | ok / warn / fail / info / issue & PR numbers |
| Gauges | `gauge_low`, `gauge_medium`, `gauge_high`, `gauge_background` | use via gauge helpers |
| Notifications | `notification_critical`, `notification_blocker`, `notification_default` | banners, toasts |
| Keybinds | `keybind_key`, `keybind_label_bg`, `keybind_label_fg` | help / hint bars |
| Selection | `selection_bg`, `selection_fg` | focused list row |
| Titles | `title_accent` | block titles (set by `styled_block`) |
| F-key badges | `fkey_badge_bg`, `fkey_badge_fg` | F-key bar |
| Surfaces (#1247) | `surface_bar`, `surface_overlay` | status bar background; dialog / palette background |
| PR states (#1247) | `status_needs_pr`, `status_conflict` | PR-ready and conflict-fix states (not yet wired into `status_color`) |
| Agent badges (#1346) | `agent_claude`, `agent_codex`, `agent_qwen`, `agent_opencode`, `agent_ollama`, `agent_minimax`, `agent_unknown` | use via `agent_color(agent_id)`; all `Reset` under no-color; not in `ThemeOverrides` |

`border_focused` is retired: `ThemeOverrides` still parses it so old
`maestro.toml` files load, but `Theme` has no such field and nothing draws it.

Dark values for the four v1 tokens (§03): `surface_bar` = `Indexed(236)`,
`surface_overlay` = `Indexed(235)`, `status_needs_pr` = `Blue`,
`status_conflict` = `LightMagenta`.

NO_COLOR snapshots: `snapshot_tests::no_color::no_color_snapshot(|f, theme| ..)`
draws any screen with `Theme::no_color()` and returns the buffer with styles,
so a screen adds its NO_COLOR snapshot in one line. It panics if any cell's
fg, bg or underline colour is not `Reset` (#1346).

Adding a token means touching: `Theme`, `ThemeOverrides`, the five presets,
`from_config`, `apply_capability`, and the `tui.theme.overrides` schema. Prefer an existing token first.

### Helper methods

| Method | Returns | Notes |
|---|---|---|
| `theme.styled_block(title, focused)` | `Block` | **Default panel.** Delegates to `ds::panel::panel`: focused = Thick border in `border_active` + bold, unfocused = Plain in `border_inactive`; centered `[ Title ]` in `title_accent` |
| `theme.styled_block_plain(focused)` | `Block` | Same, no title (`ds::panel::panel_plain`) |
| `theme.stats_block()` | `Block` | Top + bottom border only (stats rows) |
| `theme.status_color(SessionStatus)` | `Color` | Session state color |
| `theme.gauge_color(pct)` | `Color` | Context / usage gauge (high = bad) |
| `theme.compact_gauge_color(pct)` | `Color` | 4-tier variant: green <50, yellow 50–70, orange 70–85, red >85 |
| `theme.milestone_gauge_color(pct)` | `Color` | Inverted: high completion = green |
| `theme.budget_color(u8)` | `Color` | `>= 90` error, else warning |
| `widgets::focused_selection_style(theme)` | `Style` | Focused row: selection colors + bold + reversed (crate-private) |

### Panels, breakpoints, SizeGuard — `crates/maestro-tui/src/tui/ds/` (#1253)

Border type carries the meaning (design `§03 Borders & focus`), so focus
survives NO_COLOR. Use `styled_block` for the two everyday states; call
`ds::panel` directly for overlays and danger.

| Item | For |
|---|---|
| `ds::panel::panel(title, Focus, theme)` / `panel_plain(Focus, theme)` | `Focus::Inactive` Plain · `border_inactive`; `Active` Thick · `border_active` + bold; `Overlay` Rounded · `accent_info`; `Danger` Double · `accent_error`. Double means danger only |
| `ds::breakpoint::Breakpoint::from_area(rect)` | `TooSmall` (< `MIN_SIZE` 80x24) · `Compact` 80–99 · `Regular` 100–149 · `Wide` >= 150 columns |
| `ds::breakpoint::MIN_SIZE` | `(80, 24)` |
| `ds::size_guard::draw(f, area, theme)` | "Terminal too small" screen: the minimum and the current size. `ui::draw` draws it for every mode when `from_area` is `TooSmall`, so no screen needs its own size floor. The `TuiMode` confirms `ConfirmExit`, `ConfirmKill` and `BudgetPreSpawn` are drawn on top of the guard. While the guard is shown, a key goes only to one of those confirms; every other key (global chords included) is ignored, except `Ctrl+C`. Resize events always go through. Confirms that live inside a screen (Settings discard, Fleet board / Adopt picker live confirm) stay hidden, and their keys are ignored |
| `ds::size_guard::is_shown(area)` | The one size test: `ui::draw` uses it to draw the guard, and the run loop uses it (`input_handler::handle_key_under_size_guard`) to swallow keys |

Enforced by `screens_use_theme_colors_only` in `tests/tui_design_system.rs`:
any `Color::` (except `Color::Reset`) in a non-test screen file fails CI.
Known debt is allowlisted there (`pr_review/concerns.rs`,
`issue_wizard/draw_diff.rs`). Don't copy it. The allowlist can only shrink:
fix a file, then remove it from the list.

---

## 2. Widgets — `crates/maestro-tui/src/tui/widgets/`

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
| `wizard_fields::TextAreaField` + `WizardFields` | `crates/maestro-tui/src/tui/screens/wizard_fields.rs` | Used by wizards (focus cycle, `insert_sanitized`) |
| Raw `ratatui_textarea::TextArea` + `screens::wrap` | pattern in `crates/maestro-tui/src/tui/screens/prompt_input/draw.rs` | Rich prompt / interaction input |

**Never** hand-roll a `String` + cursor. It caused three bugs in a row in #733
(cursor drift on wrap, no scroll, no arrow keys). For wrapping and scroll use
`screens::wrap::{wrap_lines, scroll_offset_for_cursor}`, then place the cursor
with `f.set_cursor_position`.

Enforced by `no_new_hand_rolled_text_input`: a file outside `widgets/input/`
that matches `KeyCode::Char(c)` and pushes or inserts `c` into a string fails
CI. The 12 existing sites are in `HAND_ROLLED_INPUT_ALLOWLIST`. Migrating one
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
| `bypass_indicator::render_line` | The `▲ BYPASS` chip (`BYPASS_CHIP`, reverse warn + bold). Only `ds::status_bar` draws it; no screen draws its own bypass banner |

### Armed-Enter — the only way a screen starts a GitHub write (#1250)

Every GitHub write a screen starts (create, comment, review, merge, label,
re-run) goes through `ArmedEnter` and is drawn with `ArmedBar`. No screen
calls a GitHub write on a single key. Screens adopt it in their own issues.

| Item | Where | For |
|---|---|---|
| `ArmedEnter` | `crates/maestro-tui/src/tui/ds/armed.rs` | Pure state machine, clock passed in: `on_enter(now) -> EnterOutcome` (`Armed` / `Commit` / `Ignored`), `tick(now)`, `disarm()`, `block(reason)`, `reset()`, `finish(Result<(), String>)`, `remaining(now)` |
| `ArmedState` | same | `Blocked(reason)` → `Ready` → `Armed(at)` → `Working` → `Done` / `Failed(fix)` |
| `ARM_WINDOW` | same | The 3 s window, in one place. Second Enter at `<= ARM_WINDOW` commits; later re-arms |
| `ArmedBar::new(&armed, description, now).render(f, area, theme)` | `widgets/armed_bar` | Glyph + word + the exact write (`creates 1 issue in owner/repo`); second line = reason, countdown, or fix. Height `ARMED_BAR_HEIGHT` |

Rules: Blocked is inert (Enter returns `Ignored`), so show the fix in the
reason. On `EnterOutcome::Commit` start the write, then call `finish`.
Any other key or Esc calls `disarm()`; the app tick calls `tick(now)`.
The description is plain text (no GitHub types) and is sanitized by the bar.

### Typed confirm — `crates/maestro-tui/src/tui/ds/confirm.rs` (#1261)

Implements §04 `confirm` and §05 "Killing the wrong agent" of
`docs/design-system/index.html`. Destructive actions ask you to type a
token; `Esc` cancels and is the default.

| Item | For |
|---|---|
| `TypedConfirm::new(title, expected, action, consequences)` | `consequences` = `(label, text)` rows (`("kept", "…")`); an empty label draws the text alone. `action` names `⏎` (`kill`, `enable`). Field = `Input::single_line` (max 16) |
| `handle_key(key) -> ConfirmOutcome` | `Pending` / `Confirmed` / `Cancelled`. `Esc` cancels; `⏎` is inert until `matches()` (typed text == `expected`, case-sensitive); every other key edits the field |
| `render(f, area, theme)` | Centred in `area`, `ds::panel` `Focus::Danger` (Double). Consequences, `Type <token> to confirm:`, the field with `n of m`, then `Esc cancel (default)` and `⏎ <action> (locked)` until it matches |

Token rule (the #1261 ADR): the issue number for a per-session action
(kill), a word for a global one (bypass: `CONFIRM`). Users today: kill
(`App::open_kill_confirm`, state in `ScreenState::kill_confirm`) and the
bypass warning (`BypassWarningState::confirm`; `new()` is the `⌃B` session-only text, `with_text(title, consequences)` keeps `CONFIRM` with another text). `ui::draw_typed_confirm`
draws both last, over the whole frame. Both modes are text-input modes
(`is_text_input_mode`), so typed `q`, `M`, digits never reach a global key.

Bypass paths (#1411): the pool mode and the `▲ BYPASS` chip are set together.
`setup::setup_app_for_agent` reads the run's agent (`Config::effective_permission_mode_for`)
and calls `App::activate_bypass_from(source)` with `cli`, `config` or `agent <id>`;
Right before every spawn, `App::note_spawn_bypass(id)` lights the chip when
the session's own mode makes it run in bypass (log `mode <name>`, pool
default untouched), also after ⌃B turned the chip off while it was queued. In
Settings, choosing `bypassPermissions` for an agent or a mode is a pending
bypass request like the Sessions row (`settings/tabs/bypass_request.rs`):
`SettingsScreen::bypass_warning()` opens the Settings warning naming the key
it writes, `Esc` puts the last saved value back. Only the Sessions row and
the default agent put the pool in bypass on CONFIRM; a mode or another agent
is staged only (`SettingsScreen::bypass_target_effect` says where it applies:
sessions in that mode, or `maestro run --agent <id>`). A Ctrl+S save never turns
bypass on: `App::refuse_unconfirmed_bypass` keeps the pool out of bypass and
logs why; only a typed CONFIRM turns it on. A save turns bypass off only when
it moved the saved mode off bypass itself (a ⌃B or CLI bypass stays on).

### Hold to confirm — `crates/maestro-tui/src/tui/ds/confirm.rs` (#1260)

Implements §04 `confirm` and §05 "Quitting while agents run". No keyboard
enhancement flags are on, so terminals send no key release: a hold is a run
of key repeats, and a second press inside the window is the fallback.

| Item | For |
|---|---|
| `HoldToConfirm::new(code, mods)` | The hold key (`⌃Q` = `Char('q')` + `CONTROL`). `HOLD` = 0.5 s, `WINDOW` = 2 s |
| `handle_key(&key, now) -> ConfirmOutcome` | First press arms. A second press inside `WINDOW`, or repeats held for `HOLD`, confirm. `Esc` cancels; release events, `⏎` and every other key do nothing |
| `is_armed(now)`, `progress(now)`, `reset()` | Progress is `0` idle, `0.5` armed, up to `1` while held |
| `lines(key, action, now, theme)` | Two rows: the key chip, `hold to <action>`, a 16-cell bar, then `press <key> again` (armed) or `or press <key> twice within 2s` |

User today: Confirm Exit (`input_handler::QUIT_HOLD`, state in
`ScreenState::exit_hold`, drawn last by `ui::draw_typed_confirm`, sized from
its lines). Exit chords: ⌃Q and `q` quit at once when there is no live work.
Live work (`input_handler::has_live_work`) = pool active + queued sessions, a
queue executor that has not finished (also between items or awaiting a
decision), a completion / auto-PR job in flight, or a team run until its
result lands (`TeamWizardScreen::team_run_live`). The question names the most
specific work (`input_handler::live_work_question`). With live work, ⌃Q opens the dialog armed, `q` / ⌃C /
⌃X open it unarmed, and only the hold quits (`y` and `⏎` do nothing). In the
idle dialog `y` and ⌃Q quit. Exit chords ignore key release events. Once per
launch the run loop calls `announce_quit_key` (toast + activity log: ⌃Q quits,
TurboQuant is in Settings); it never blocks a quit. The first ⌃X shows
`⌃Q quits from anywhere`
(`ScreenState::quit_hints`). The hold reads `input_handler::quit_now`:
`ScreenState::quit_clock` when a test injects one, else `Instant::now`. The
TurboQuant on/off toggle is the Settings › TurboQuant › Enabled field
(`navigation::registry::TURBOQUANT_TOGGLE`); no chord toggles it.

### Other shared helpers (`crates/maestro-tui/src/tui/`)

| Module | Item | For |
|---|---|---|
| `keybinding_hints.rs` | `keybinding_hints_spans(fitted, copy_enabled, theme)` | Hint bar with dimmed inactive keys; pre-fit with `navigation::keymap::fit_hints_to_width` |
| `screens/mod.rs` | `sanitize_for_terminal(s)` | **Always** on text from GitHub / git / AI output before rendering |
| `icons.rs` | `icons::get(IconId::..)` | Nerd Font or ASCII icon (respects `tui.ascii_icons` / `MAESTRO_ASCII_ICONS=1`). Never paste raw glyphs |
| `spinner.rs` | `spinner_frame`, `tool_frame`, `spawning_frame`, `idle_pulse`, `animated_activity`, `thinking_activity` | Animation frames from the app tick |
| `marquee.rs` | `MarqueeState`, `visible_slice`, `needs_scroll` | Scroll text that overflows |
| `breadcrumb.rs` | `elide_breadcrumbs(labels, max_width, sep)` | Fit breadcrumb to width |
| `markdown.rs` | `render_markdown(input, theme, width)` | Markdown → `Text` |
| `clipboard_toast.rs` | `draw_copy_toast` | "Copied" confirmation (thin caller of `ds::toast`) |
| `budget_banner.rs` | `draw_budget_banner_if_alerting` | Budget alert banner |
| `gh_auth_warning.rs` | `draw(f, area, theme)`, `line(theme)`, `GH_AUTH_EXPIRED` | The one-line `⊘ gh auth expired · PR create paused · fix: gh auth login, then Shift+P` drawn on the log row while `App::gh_auth_ok` is false (#1241). Pure draw: the recheck is `app/gh_auth.rs` (one `gh auth status` on the blocking pool, before a PR action is refused), never the draw path |
| `agent_badge.rs` | `agent_label`, `agent_kind_for_id` | Per-agent label and kind; the colour is `theme.agent_color(agent_id)` |

### Shell rows: TabBar, CrumbBar, footer (`crates/maestro-tui/src/tui/ds/`, #1258)

Implements "Shell rules" and §04 `tab_bar` / `key_hints` from
`docs/design-system/index.html`. `ui::draw` lays out every mode as: status
bar, one shell row, main content, activity log, footer. Screens draw none of
these rows.

| Item | For |
|---|---|
| `ds::tab_bar::TABS` | The seven top-level screens: `1 Home` (`Landing`), `2 Dashboard`, `3 Issues`, `4 Milestones`, `5 Roadmap`, `6 Stats`, `7 PRD` |
| `ds::tab_bar::draw(f, area, active, theme)` | The shell row on a tab screen. Active tab is REVERSED + bold (reads under NO_COLOR); `? help` on the right when it fits |
| `ds::tab_bar::tab_for_digit(c)` / `tab_index(mode)` / `tab_label(mode)` | Digit → mode, and "is this mode a tab?" |
| `ds::crumb_bar::draw(f, area, &crumbs, state, theme)` | The shell row inside a flow (any mode that is not a tab): `Home › New issue`, elided with `elide_breadcrumbs`. `state` is the right-aligned slot (draft saved, fetch age, write state); `None` shows `Esc back  ? help` |
| `ds::crumb_bar::crumbs(stack, current)` / `crumb_label(mode)` | Crumb labels from the nav stack: tab name for a tab, else `breadcrumb_label` |
| `ds::key_hints::draw(f, area, &hints, pin_quit, theme)` | The footer: `key label` pairs, cut from the end at their drawn width, the quit hint `ds::key_hints::QUIT` pinned right (`pin_quit = false` only in the quit dialog). `QUIT` is `⌃Q quit` (#1260) |
| `ds::key_hints::from_pairs(&[(key, label)])` | A screen's `(key, label)` list as `InlineHint`s, in order |
| `ds::bar_row::draw_left_right(f, area, left, right)` | One row with a flush-right part that drops when it does not fit |
| `Screen::footer_hints(&self)` | A screen's footer hints for its current state, most important first. Empty (the default) means the footer shows the mode's `mode_keymap` hints |

Rules: never draw a hint row inside a screen. Return the hints from
`footer_hints`; `screens_draw_no_keybinds_bar` in `tests/tui_design_system.rs`
fails CI on a screen that draws its own. Digits `1`-`7` switch tabs only on a
tab screen with no text field focused (see §3 Keybindings).

### Markdown block and readiness checklist (`crates/maestro-tui/src/tui/ds/`, #1252)

Implements `mdBlock`, `checklistPanel` and `readyStrip` from
`docs/design-system/index.html` (§04 Kit v2, §02.2 Issue wizard).

| Item | For |
|---|---|
| `render_markdown_with(input, theme, width, MarkdownOptions)` | The one markdown renderer with options. `MarkdownOptions::default()` is exactly `render_markdown`. `HeadingStyle::Bare` drops the `#` and draws the heading bold `accent_warning`; `inline_marks` turns `[TODO: x]` into a reversed warn chip (plus a plain space) and `#123` into `accent_identifier` (via `extract_issue_refs`) |
| `ds::md_block::md_block(input, theme, width)` | Preset `MD_BLOCK` (bare headings + inline marks) for wizard previews, critiques and issue bodies. Use this, not a second renderer |
| `ds::checklist::checklist_panel(f, area, title, &[CheckItem], focused, theme)` | Live checklist panel (Wide): `✓ ✗ ! ○` rows (`CheckState::{Ok, Missing, Warn, Pending}`), `n/m ready` on the top-right border |
| `ds::checklist::ready_strip(label, &[CheckItem], compact, width, theme)` | Its one-line form (Regular, Compact): `Draft 3/5 ready   ✗ Acceptance   …`. Lists only `Missing` and `Warn`; `compact` hides notes; cut with `…` at `width` |

Rules: `Pending` (`○`) is for steps not reached yet and is never red.
`ready_count` counts only `Ok` as ready, and every row (also `Pending`) in the
total. Meaning rides on glyph + BOLD/REVERSED, so both read under NO_COLOR.

### Status chip, gauge level, money (`crates/maestro-tui/src/tui/ds/`, #1251)

| Item | Returns | For |
|---|---|---|
| `status_chip::status_chip(status, theme)` | `Span` | `glyph word` for any `SessionStatus`, coloured by `status_color`, except `NeedsPr` (`status_needs_pr`) and `ConflictFix` (`status_conflict`) (#1353); `NeedsReview` / `FailedGates` / `Errored` are bold. Reads the global icon mode |
| `status_chip::status_chip_for_mode(status, theme, nerd_font)` | `Span` | Same, icon mode passed in (use it in tests) |
| `status_chip::status_word(status)` | `&str` | The chip word alone (`needs you`, `PR ready`, `gates failed`, ...) |
| `gauge::gauge_level(ratio)` | `GaugeLevel` | Spend and quota gauges: `< 0.6` `Low`, `0.6` `Medium`, `0.8` `High`, `>= 1.0` `Critical`. NaN / negative = `Low` |
| `GaugeLevel::style(theme)` | `Style` | `gauge_low` / `gauge_medium` / `gauge_high`; `Critical` = `gauge_high` + BOLD |
| `money::money(Option<f64>)` | `String` | The only dollar formatter: `$12.40`, `$0.00`, `<$0.01` under one cent, `$?` for `None` (unpriced). NaN, negatives, `-0.0` print `$0.00` (via `sanitize_cost`) |

Chip glyphs are `IconId::Chip*` in `crates/maestro-core/src/icons.rs` (Unicode with an ASCII fallback such as `[E]`). Context gauges do **not** use `gauge_level`: they keep the 50% / 70% thresholds that match the overflow actions (`compact_gauge_color`). Existing gauges and `format!("${:.2}")` call sites move to these helpers in the screen issues, not here.

### Status bar — `crates/maestro-tui/src/tui/ds/status_bar.rs` (#1257)

The one state-only row at the top of every mode (design `§04 StatusBar`,
Shell rules). `ui::draw` draws it for every screen, so no screen draws
budget, bypass or update state of its own. No key hints and no actions here.

| Item | For |
|---|---|
| `status_bar::StatusModel` | Plain data the bar shows: version, `FleetCounts`, spent / cap, quota badges, TurboQuant, `untrusted`, `bypass`, `update`, `continuous` (`(done, attempted)` while a continuous run is active) |
| `FleetCounts::from_statuses(statuses)` | `● run` (spawning, running, gates, CI / conflict fix, retry) · `◐ wait` (needs review, PR ready, interactive) · `✗ fail` (gates failed, errored). Empty pool shows `○ no agents` |
| `status_bar::draw(f, area, &model, theme)` / `draw_for_mode(.., nerd_font)` | One row on `surface_bar`. Left: brand, fleet, budget (`money` + `gauge_level`), `CONTINUOUS: n/m done` (reverse info + bold, only while a run is active), quota, `TQ:ON`. Right: `▲ BYPASS`, `UNTRUSTED`, `↑ <version> u` |
| `status_bar::segments(&model, width, theme, nerd_font)` | The `(left, right)` spans after dropping. Whole segments drop, never a cut value: update first, then TurboQuant, quota, the continuous chip, budget, fleet. Brand, `▲ BYPASS` and `UNTRUSTED` never drop |
| `status_bar::pinned_width(&model, nerd_font)` | Columns of the safety chips; an overlay on the bar row (upgrade progress) stays left of them |

Rules: `UNTRUSTED` shows exactly when `App.workspace_trusted` is false. The
update notice has one place: the `↑ <version> u` chip (the activity log line
carries no version). Breadcrumbs and key hints are not part of this row.

### Sub-tabs, filter chips, KPI tile, tree prefix (`crates/maestro-tui/src/tui/ds/`, #1256)

Implements `subTabs`, `filterChips`, `kpiTile` and `treePrefix` from
`docs/design-system/index.html` (§04 Kit v2). Screens adopt them in their own issues.

| Item | Returns | For |
|---|---|---|
| `sub_tabs::sub_tabs(&[SubTab], active, theme)` | `Line` | Sub-view strip: `d Detail   f Output   v Gates ✗`. `SubTab { key, label, badge }`; the key is bold `keybind_key`, the badge `text_muted`; the active tab is REVERSED + bold. `active` out of range = none active |
| `filter_chips::filter_chips(&[FilterChip], width, theme)` | `Line` | Active filters ` label:tui ✕ `, REVERSED with a bold value. Chips that do not fit `width` collapse into a muted `+n` (`n` = hidden chips); the line never passes `width` |
| `kpi_tile::kpi_tile(f, area, &KpiTile, theme)` / `kpi_tile_for_mode(.., nerd_font)` | — | One KPI in a panel, height `KPI_TILE_HEIGHT` (4). `KpiState::Ready { value, delta, spark, context }`: bold value, `KpiDelta { text, trend }` with `Trend::{Up, Down, Flat}` glyph `↑ ↓ =` (ASCII `^ v =`), a ratatui `Sparkline` of the last 6 (tile < 26 wide) or 12 points, and one muted context line. `KpiState::Empty { reason }` draws the reason (`no runs yet`), never a fake `0` |
| `tree_prefix::tree_prefix(last_path, ascii)` | `String` | Tree row prefix, three columns per level: `├─ `, `└─ `, `│  ` (ASCII `|- `, `` `- ``, `|  `). `last_path[i]` = the ancestor at level `i` is a last child; the final entry is the row. Empty path = root, no prefix |

Rules: meaning rides on REVERSED / BOLD and glyphs, so every piece reads under
NO_COLOR. The delta colour is neutral: whether up is good depends on the metric,
so say it in the context line.

### Toast (`crates/maestro-tui/src/tui/ds/toast.rs`, #1254)

Implements `toast` from `docs/design-system/index.html` §04 (Components, Kit v2). The one toast: never draw a second banner.

| Item | For |
|---|---|
| `Toast::new(Severity, message)` / `Toast::undoable(message, undo)` | Build a toast. `Severity::{Success, Info, Warn, Error}`. An undoable toast is a success toast with a `^Z undo` countdown for `UNDO_WINDOW` (5 s) |
| `App::push_toast(toast, now)` | Show it. Success / info live 3 s, warn 6 s, error stays until `Esc`. One toast is visible: a newer one replaces the older, errors queue (at most 5; a sixth drops the oldest, #1382) |
| `ToastUndo` | `Box<dyn FnOnce(&mut App)>`: what `Ctrl+Z` runs. Which actions get one is each screen's call |
| `ds::toast::draw(f, area, &ToastView, theme)` / `draw_for_mode(.., nerd_font)` | Rounded `surface_overlay` box at the bottom right of `area`, above its last row; glyph + severity word (`ok`, `info`, `warn`, `error`) in the severity colour, bold. `ui::draw` draws `app.toasts` after every screen, so screens never call it |

Message text goes through `sanitize_for_terminal` inside `draw`. Glyphs and
the countdown bar come from `icons::get_for_mode` (`CheckCircle`, `DotFill`,
`Warning`, `XCircle`, `GaugeFilled`, `GaugeEmpty`), so ASCII mode works.

### Command palette (`crates/maestro-tui/src/tui/ds/palette.rs`, #1255)

Implements `palette` from `docs/design-system/index.html` §04 and the §06
`⌃K` row. `⌃K` opens it on any screen, except while a text field has focus
(the field keeps `⌃K`) or a modal is up.

| Item | For |
|---|---|
| `registry::actions_on(bindings, mode)` | The actions reachable on a screen: Global plus its screen block, one binding per action (the first chord), hidden aliases and `⌃K` left out. The palette lists these; Help (#1398) can read the same list |
| `registry::PALETTE_COMMANDS` / `PaletteCommand` | Commands with no key, declared next to the bindings (`network`) |
| `palette::entries(bindings, mode, disabled)` | One `Entry` per action, then the palette-only commands. `disabled` returns the reason an action cannot run; it shows with `⊘` and ⏎ does nothing |
| `palette::fuzzy_hits(query, label)` | Case-insensitive subsequence match; the hit indices are drawn `UNDERLINED` |
| `PaletteState` (`app.palette`) | The open palette: a canonical `Input` query (keys go through `feed`), the selection. `handle_key` returns `Outcome::{Open, Close, Run}`; `draw` dims the whole frame, then draws the Rounded box on `surface_overlay` with an `INSERT` chip |

⏎ closes the palette and runs the entry: a tab opens its screen, any other
binding goes through `handle_key` as its own key, so a destructive one
still asks first. A new binding in `registry()` shows up with no palette
edit. While open the palette counts as text input (`is_text_input_mode`)
and blocks the log drawer; a quit chord closes it.

### Activity log pane and log drawer (`crates/maestro-tui/src/tui/ds/log_drawer.rs`, #1263)

Implements the Activity log row of "Shell rules" in `docs/design-system/index.html`. The pane is drawn only on Home, Dashboard and the session Overview; other screens draw no log row. Decision: `docs/design-system/decisions.md` (2026-11-17).

| Item | For |
|---|---|
| `ActivityLog::take_alerts()` | WARN/ERR entries since the last call. `ui::draw` turns them into toasts (`App::push_toast`) when neither the pane nor the drawer is visible, and drops them when one is. Screens never call it |
| `ActivityLog::drawer_open` | The drawer state. `Ctrl+L` opens it (`handle_global_shortcuts`). While shown it is modal: `handle_key` swallows every key except `Ctrl+L` / `Esc` (close) and the quit chords before it (`Ctrl+C`, `Ctrl+X`, `ConfirmExit`); the footer shows only `Esc Close log  ⌃L close` |
| `ui::log_drawer_blocked(app)` / `ui::log_drawer_shown(app)` | Under the help overlay or a modal (`ConfirmExit`, `ConfirmKill`, `BudgetPreSpawn`, `QueueExecution`, `QueueConfirmation`, `CompletionSummary`, `ContinuousPause`, `HollowRetry`, `AdaptFollowUp`, `CiErrorReview`, `SessionSwitcher`, `BypassWarning`) or a confirm inside a screen (Settings discard, Interaction quit, Adopt picker / Fleet board live confirm) the drawer is hidden but stays open, and `Ctrl+L` / `Esc` go to the modal. It comes back when the modal closes |
| `ds::log_drawer::draw(f, area, rows, &log, theme, nerd_font)` | Rounded `Focus::Overlay` box on `surface_overlay`, the last `rows` rows of `area` (clamped), local time. `ui::draw` passes the main area and `tui.layout.activity_log_height` (same rows as the pane) |
| `ActivityLog::draw_block_in_zone(.., block, ..)` | The log lines inside a caller's block (the drawer's overlay panel) |

The Interaction composer clears its input with `Ctrl+G` (`Ctrl+U` stays ratatui-textarea's undo). Rules: log a WARN/ERR to tell the user something went wrong off the log screens; do not also push a toast for the same event. Interaction and session views keep their own transcript; they are not the Activity pane.

### Needs-you queue and terminal title (`crates/maestro-tui/src/tui/ds/attention.rs`, #1259)

Implements `AttentionQueue` (`ds::attention_queue`) from `docs/design-system/index.html` §04, the Needs-you panel of §02 Home, and the §05 "Missing an agent that waits on you" title count. Decision: `docs/design-system/decisions.md` (2026-11-17, Needs-you queue).

| Item | For |
|---|---|
| `attention::rank(&AttentionInput)` | Pure ranking over plain data (`SessionSignal`, `doctor_failures`, `gh_auth_lost`, `BudgetSignal`, `QuotaSignal`); no `App` needed. `AttentionKind::{Error, NeedsReview, NeedsPr, Resource}` in that order: gates failed / errored sessions, failed doctor checks and lost gh auth, then review, then PR ready, then budget and quota. Kind first, then the oldest `since`; undated items after dated ones; ties keep input order |
| `AttentionItem` | `kind`, `subject` (`Subject::Session { id, issue, status }`, `DoctorCheck`, `GhAuth`, `Budget`, `Quota`), one sanitized `reason`, the `action` word (always on `⏎`), `since` |
| `attention::draw(f, area, items, QueueView, theme, nerd_font)` | `Needs you (n)` panel (`ds::panel`, Thick when focused). Row: chip (`status_chip` for sessions; `✗ doctor`, `✗ gh auth`, `⚠ budget`, `⚠ quota`), subject, reason (cut with `…`), age, `⏎ action`. The selected row uses the focused selection style. Empty: `Nothing needs you. Start something: i f a` plus a key line when `fleet_busy` is false; `All clear. Agents are working.` when true |
| `attention::terminal_title(n)` | `maestro`, or `(n) maestro` when `n > 0` |
| `attention::exit_title()` | `maestro`: written by the quit restore in `tui::run`, so no stale count stays in the shell tab |
| `App::attention()` / `App::attention_input()` | The queue from App state: pool sessions, the Home doctor warnings, `gh_auth_ok`, the budget enforcer when it alerts, quota windows at 80% or more |
| `App::sync_terminal_title(out)` | Writes the title only when it changed. The run loop calls it after each draw |

Rules: one action per row, always `⏎` (no per-row letters; `p` already means PRD on Home). The panel and the `⏎` jump are live on Home since #1264: see "Home Mission Control and Fleet table" below.

### Home Mission Control and Fleet table (`screens/home/mission/`, `ds/fleet_table.rs`, #1264)

Implements `§02 Home: Mission Control` of `docs/design-system/index.html` (the mock's home layout and its Actions, Fleet, quota panel, quota strip and Milestone drawings). `TuiMode::Landing` draws it; the Release Console no longer draws on Home. Decision: `docs/design-system/decisions.md` (2026-11-17, Home Mission Control). The Needs-you panel and its `⏎` jump from the section above are live here.

| Item | For |
|---|---|
| `screens::home::mission::draw(f, area, &MissionView, &mut MissionControl, bp, theme, nerd_font)` | Wide: Actions + Milestone · Needs you / Fleet · Budget & quota (the rest of the right column is the Insight slot). Regular: right column drops, quota is a one-row strip on top. Compact: strip / Needs you / Fleet, Actions by key only. `bp` is the terminal's breakpoint (the body is shorter than 24 rows at 80x24). The Activity pane under it stays the shell's |
| `App::mission_view(now)` / `MissionView` | Plain data Home draws: ranked `attention`, `FleetRow`s for every pool session, `fleet_busy`, budget rows (`spend` vs the global cap, then each fresh quota window), `MilestoneCard`, `ActionGate`, `now` |
| `ACTION_GROUPS` / `actions()` | RUN `d f a` · PLAN `i m r p h` · INSIGHT `s t`. Labels and targets come from the landing `MENU_ITEMS`, so the letters stay the same. Quit is not an action (global `q`) |
| `disabled_reason(key, &ActionGate)` | `i m h` → `gh signed out` when `gh_auth_ok` is false; `h` → `no milestones yet` once stats loaded with no open milestone (unknown is not "no"). A disabled row draws its key and label muted + DIM with `⊘` on the right, and one `⊘ key reason` line under the groups. Its letter and `⏎` on it do nothing |
| `MissionControl` (`ScreenState.mission_control`) / `handle_key` | Needs you has focus on open. `Tab` / `BackTab` switch Needs you ⇄ Actions (not on Compact), `j k ↑ ↓` move, `⏎` runs the row, letters run actions from any panel, `n` release notes |
| `mission::dispatch::dispatch_input(app, event)` | Home keys, routed like the PRD screen. `⏎` on a session row opens the session Overview with that session selected and highlighted (`PanelView::reveal`; the next Overview draw moves the grid cursor, page included, with `GridState::select_index`, the inverse of `selected_index`); doctor / gh auth / budget / quota rows open Dashboard. Ctrl / Alt chords are ignored here |
| `ds::fleet_table::draw(f, area, &[FleetRow], compact, theme, nerd_font)` | `Fleet (n)` panel: `STATUS ISSUE TITLE MODEL CONTEXT COST TIME`, Compact `STATUS ISSUE TITLE CTX COST`. Chip via `status_chip_for_mode`, cost via `money` (`$?` unpriced), context with its 50% / 70% steps. Empty: `No agents yet. i new issue` |
| `ds::gauge::bar(ratio, width, nerd_font)` | `(filled, empty)` cells from `GaugeFilled` / `GaugeEmpty` (`▓░`, ASCII `#-`); ratio clamped to `0..=1`, NaN empty. Colour the filled part with `gauge_level(ratio).style(theme)` |

Rules: no key hints in Home panel titles (they live in the footer: `Enter Open  j/k Move  Tab Next panel`). Home draws no logo and no link chart (`home_no_decoration_after_first_run` guards it). Full-app Home snapshots leave session times unset so they do not depend on the clock.

---

## 3. Screens — `crates/maestro-tui/src/tui/screens/`

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
`crates/maestro-tui/src/tui/input_handler.rs`. A screen binding these never sees the key:

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
| `1`-`7` | Switch tab (`ds::tab_bar::TABS`, in `tab_key_target`) | only on a tab screen, not in text-input mode. Overview keeps `1`-`9` for session detail |
| `Ctrl+Z` | Run the visible toast's undo | only while an undo toast is visible; else the screen gets it |
| `Esc` | Dismiss the visible error toast | only while an error toast is visible; else the screen gets it |
| `Ctrl+L` | Open / close the log drawer (`ds::log_drawer`) | every screen, unless a modal hides the drawer (#1263) |
| `Esc` | Close the log drawer | only while the drawer is shown; else the screen gets it |

Text-input mode is one check: `is_text_input_mode` in `input_handler.rs`
(#1377). It is true when the active screen reports `InputMode::Insert`, in
`PromptInput`, `SessionSwitcher` and `Settings`, while the help overlay search
is active, while a PRD field is edited and while a Roadmap label or assignee
filter is edited. Every global bare-key handler asks it: the upgrade keys
`u` / `n` / `Esc` (bare only), the run loop's `y` restart
(`is_restart_key`, bare only), `q`, `?`, `A`, `e`, the Shift globals above and
the `1`-`7` tab switch. A new text field whose screen does not report `Insert`
must be added there; `global_bare_keys_never_fire_on_any_text_input_surface`
in `input_handler_text_input_tests.rs` walks every surface, so add it to
that list too.

Before adding a key: grep `handle_global_shortcuts`. In text-input screens
plain letters are typed, so use a free `Ctrl+letter` (e.g. `Ctrl+W`) and
catch it before forwarding to the editor. Declare every key in
`keybindings()` so it shows in help.

Conventions: `j/k` or arrows move, `Enter` open/confirm, `Esc` back,
`Tab` cycle focus, `Space` toggle.

### Keymap registry — `crates/maestro-tui/src/tui/navigation/registry.rs` (#1248)

Implements §06 Keymap and §07 "Keymap registry" of
`docs/design-system/index.html`. It declares keys; it does not dispatch them.
`handle_global_shortcuts` and the mode handlers still run the keys, and a
screen moves into the registry one at a time.

| Item | For |
|---|---|
| `Binding::new(chord, scope, action, danger, label)` | One declared key. `.hidden()` keeps an alias (`↑` next to `k`) out of the footer; `.exception(why)` documents a case-rule exception |
| `Chord::key(code)` / `Chord::ctrl(c)` / `Chord::label()` | The key, and the text a hint shows (`⌃K`, `Enter`, `G`) |
| `Scope::Global` / `Scope::Screen(mode)` / `Scope::Panel(mode, name)` | Where it applies. Global ⊃ Screen ⊃ Panel (`Scope::encloses`); a screen matches its mode with any payload |
| `Danger::Safe` / `Danger::Destructive` | Drives the case rules |
| `registry()` | Every binding: the §06 Global table, the other keys `handle_global_shortcuts` takes before screens (`⌃L ⌃B ⌃X ⌃C` and Shift `Q M C P`), then one block per registered screen (`LogViewer`, `SessionSummary`) |
| `check(bindings)` | The rules as `Violation`s: `Duplicate` (same chord twice in one scope), `Shadow` (an inner scope reuses an enclosing scope's chord), `LowercaseDestructive` (destructive on a bare lowercase letter), `UppercaseSafe` (safe on a bare uppercase letter with no `.exception`). Ctrl chords, digits and symbols skip the case rules |
| `footer_hints(mode)` | The footer hints of a registered screen, in registry order; `mode_keymap` uses them in place of its own list |

`keymap_is_poka_yoke_clean` runs `check(registry())` and must stay empty.
To register a screen: add its block to `registry()`, give its `mode_keymap`
arm an empty hint list, and keep the test green. Do not redeclare a Global
key (`Esc` back, `/` filter) in a screen: that is a `Shadow`. The §06 rows
`⌃K` opens the command palette (#1255, below) and `⌃Q` quits (#1260).

---

## 4. UX rules

- **Visible feedback for async work.** Show it on the screen, not only in the
  activity log: `⟳ Syncing…` while running, `✓ Synced Ns ago` (keep 5–10 s),
  `⚠ Failed: <reason>`. Over ~200 ms → spinner or `EmptyState::loading`.
- **Empty states say what to do**: key + outcome, never just `(empty)`.
- **First entry** to a feature screen: one-line "what is this" intro.
- **Destructive actions** need a confirm step (see `queue_confirmation.rs`,
  `bypass_warning.rs`). Killing or an action with a big blast radius uses
  `ds::confirm::TypedConfirm` (§2 "Typed confirm").
- **Small terminals**: test narrow sizes; fit hints with
  `fit_hints_to_width`, elide breadcrumbs, marquee long text.
- **ASCII mode** must work: icons only through `icons::get`.

---

## 5. Testing TUI changes

Snapshot tests with `insta` + `ratatui::backend::TestBackend` live in
`crates/maestro-tui/src/tui/snapshot_tests/` (helpers: `test_terminal()`, `make_session(..)`,
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
