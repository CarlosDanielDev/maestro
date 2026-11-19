# TUI design decisions and learnings

Dated decisions about the TUI design, newest first. Add an entry in the same
PR when the code must differ from [index.html](index.html), or when a session
learns something the design did not foresee. Session-end summaries still go
to the repo `MEMORY.md`; for TUI design, `MEMORY.md` only links here.

## Entry template

Copy this block to the top of the entries (below this template), newest
first. One entry per decision.

```markdown
## YYYY-MM-DD — <short title>

- **What:** the decision or learning, in one or two sentences.
- **Why:** the reason or the evidence.
- **Section:** the `index.html` section it touches (for example `§02.14 Usage`).
- **Rejected:** the alternative not chosen, and why.
- **Source:** issue / PR / session (for example `#1238`).
```

---

## 2026-11-17 — Agent badge colours are theme tokens, not user overrides

- **What:** seven flat `Theme` fields (`agent_claude` … `agent_unknown`), read through `Theme::agent_color(agent_id)`. The four coloured presets share today's colours; no-color sets them all to `Reset`. They are not in `ThemeOverrides`, as before.
- **Why:** NO_COLOR must draw no colour, and the theme is the one place colours live. Flat fields match every other token, so `apply_capability` and the Debug-based preset tests cover them unchanged.
- **Section:** `§03 Color tokens` (no-color preset: "All colours Reset").
- **Rejected:** a nested `AgentColors` struct (breaks the one-value-per-field preset tests); a `no_color` flag checked inside `agent_color` (a second source of truth); adding overrides now (config and schema scope, not asked).
- **Source:** #1346.

## 2026-11-17 — The footer pins `q quit` until #1260

- **What:** the footer's pinned quit hint is `q quit`, not the design's `⌃Q quit`. It lives in one const, `ds::key_hints::QUIT`.
- **Why:** today `⌃Q` toggles TurboQuant and `q` opens the quit dialog (outside text input). A hint must name the key that does the thing. #1260 makes ⌃Q quit and changes that one const.
- **Section:** `Shell rules`, `§04 Components` (`key_hints`).
- **Rejected:** printing `⌃Q quit` now (wrong key on main until #1260); rebinding ⌃Q in #1258 (#1260 owns it).
- **Source:** #1258 (orchestrator decision), #1260.

## 2026-11-16 — Shell rows: the footer replaces the F-key bar; digits only on tab screens

- **What:** (1) The bottom F-key bar is gone; that row is now the one `ds::key_hints` footer. F-keys still work and are listed in help (`?` / F1). (2) Digits `1`-`7` switch tabs only on the seven tab screens, never in a flow (CrumbBar) and never while a text field has focus; on Milestones this shadows the old `1` / `2` sub-tab keys (`Tab` still switches). (3) The TabBar and CrumbBar right side say `? help`, not `⌃K palette`, because the palette does not exist yet. (4) The footer pins `q quit`, not the `⌃Q quit` the design and #1258 name: until #1260 lands `⌃Q` toggles TurboQuant and `q` quits (outside text input). The pin is one const, `ds::key_hints::QUIT`; #1260 changes it. (5) Footer hints are cut at their drawn width; `fit_hints_to_width` is not used because it counts bytes and the `[k] label` form, and dropped `Esc Back` from Settings at 80 columns. (6) `›` and `⌃Q` are literal characters, like the existing `↑/↓` hints; they are text, not Nerd Font icons.
- **Why:** "one footer" in Shell rules; inside wizards digits are content (issue numbers); a key hint must not advertise a key that does nothing.
- **Section:** `Shell rules`, `§04 Components` (`tab_bar`, `key_hints`), `§02.2 Issue wizard` (CrumbBar).
- **Rejected:** printing `⌃Q quit` before #1260 (the hint would name a key that toggles TurboQuant); rebinding ⌃Q here (#1260 owns it); keeping the F-key bar under the footer (two hint rows); digits as global keys (they would eat issue numbers in wizards); changing `fit_hints_to_width` (the status bar, owned by #1257, uses it).
- **Source:** #1258.

## 2026-11-16 — toast: bottom right, ASCII-safe, one at a time

- **What:** `ds::toast` draws bottom right (the design), not top right (the #1254 text), and shows one toast at a time instead of two stacked: a newer toast replaces the older, errors queue. The glyph is followed by the severity word (`ok`, `info`, `warn`, `error`). Glyphs come from existing `IconId`s (`CheckCircle`, `DotFill`, `Warning`, `XCircle`) and the countdown bar from `GaugeFilled` / `GaugeEmpty`, not the kit's `✓ · ! ✗` and `▰▱`; the key is written `^Z`, not `⌃Z`. The kit's per-toast right-side action (`l view log`, `⏎ fix`) is not built yet. The clipboard toast moved from the status row to the same bottom-right box.
- **Why:** the issue asks for glyph + word so a toast reads under NO_COLOR; "one visible" is the issue's test hint. ASCII mode must work and raw glyphs are not allowed (same call as `ArmedBar`). Bottom right keeps the status row free for #1257.
- **Section:** `§04 Components` (`toast`, Kit v2 `toast`), `§05 Losing a reversible action`.
- **Rejected:** top right (covers the status bar and its banners); two stacked toasts (no caller needs it yet); new `IconId`s for `!` and `·` (more surface for no new meaning).
- **Source:** #1254.

## 2026-11-16 — armedBar keys and glyphs are ASCII-safe

- **What:** `ArmedBar` writes the key as the word `Enter`, not `⏎`, and takes its state glyphs from `icons::get` (`Warning`, `Play`, `Hourglass`, `Sync`, `CheckCircle`, `XCircle`) instead of the kit's `⊘ ⏎ ⣾ ✓ ✗`. It has no countdown bar: the seconds left are printed. The border colour follows the state; the border is the standard `styled_block` panel border, not the kit's per-state plain / thick / double. The state label is bold text, not the kit's reversed chip, and the working / done lines are fixed words (`writing to GitHub…`, `written to GitHub`) instead of a caller-set action word; the write itself is always in the caller's description.
- **Why:** the design system rule is "never paste raw glyphs" and ASCII mode must work; no `IconId` exists for `⏎` or `⊘`. The printed seconds carry the same meaning as the bar.
- **Section:** `§04 Components` (Kit v2 `armedBar`).
- **Rejected:** raw kit glyphs (break ASCII mode); a new `IconId` (out of scope, `icons.rs` is owned by #1251 this wave).
- **Source:** #1250. Follow-ups: `IconId::Enter` for `⏎`; a reversed chip and a caller-set action word when a screen adopts the bar and needs them.
- **Resolved (#1357):** the key part is done. `IconId::Enter` exists (`⏎` with Nerd Font, `Enter` in ASCII mode) and `ArmedBar` draws every key label through it, so ASCII mode still reads `Enter`. The reversed chip and the caller-set action word stay open.

## 2026-11-16 — md_block and checklist: where #1252 differs from the mocks

- **What:** (1) `ds::checklist` counts `○` (Pending) rows in the `n/m` total; the `checklistPanel` / `readyStrip` mocks leave "optional" rows out. (2) `#ref` in `ds::md_block` uses `accent_identifier`, not the info colour of the `md()` mock, and detection is `extract_issue_refs` (`#1` to `#999999999`, not after a letter), not the mock's `#\d{2,5}\b`, so `#1` is a ref here. (3) The `✓ ✗ ! ○` glyphs are plain characters, with no `IconId` ASCII fallback yet. (4) `checklist_panel` draws with `theme.styled_block`, which is `ds::panel` since #1253: plain border (thick when focused), as the mock, but a centred `[ Draft ]` title where the mock's `E.block` has a left-aligned ` Draft `. Bare headings (bold `accent_warning`) and the plain space after the TODO chip match the mock.
- **Why:** (1) a `○` row is a required item of a later step; leaving it out would show `3/3 ready` on a draft that is not ready. (2) the issue (#1252) names `accent_identifier`; one ref detector for prompt input and markdown keeps the two from disagreeing, and single-digit issues exist. (3) `crates/maestro-core/src/icons.rs` was owned by #1251 in this wave. (4) the title style belongs to `ds::panel`; changing it for one panel would fork the panel look.
- **Section:** `§04 Kit v2` (`mdBlock`, `readyStrip`), `§02.2 Issue wizard` (Draft checklist), `§03 Borders & focus`.
- **Rejected:** excluding `○` from the total (overstates readiness); a second `#\d{2,5}` regex for markdown only; adding `IconId` variants here (file owned by another agent this wave); a custom left-aligned title block for the checklist.
- **Source:** #1252 (PR #1349, judge nits).

## 2026-11-16 — Keys are swallowed while the SizeGuard is shown

- **What:** below 80x24, a key goes only to a `TuiMode` confirm drawn on top of the guard (`ConfirmExit`, `ConfirmKill`, `BudgetPreSpawn`). Every other key is ignored, global chords included (`Ctrl+Q`, `Ctrl+B`, `Ctrl+X`, `q`, F-keys, the upgrade-restart `y`), except `Ctrl+C`. Resize events are never swallowed. The check sits at the single input dispatch point (the run loop, before the upgrade check and `handle_key`) and uses `ds::size_guard::is_shown`, the same size test as `ui::draw`.
- **Why:** the guard hides the screen, so any key would act unseen: Settings discard (`Esc`, `y`), the Fleet board / Adopt picker live-session adopt (`y`), the QueueConfirmation launch (`Enter`), HollowRetry, BypassWarning. Swallowing keys covers these and any future screen at once.
- **Section:** `§04 SizeGuard`, `§05 Poka-yoke catalog` (Terminal too small).
- **Rejected:** drawing every screen-level confirm over the guard (it needs a per-screen hook and still leaves non-confirm keys acting unseen). Owner decision.
- **Source:** #1253 (judge finding on PR #1352).

## 2026-11-16 — The #1157 home check moves to 80x24; 60x15 now pins the SizeGuard

- **What:** `home_small_terminal` keeps a 60x15 snapshot, which now shows the SizeGuard (`home_screen_60x15_shows_the_size_guard`). The #1157 home layout checks (labels whole or ending in `…`, one activity-log row) move to 80x24, the smallest size still laid out (`home_screen_80x24_keeps_labels_and_a_log_row`).
- **Why:** §03 puts every terminal below 80x24 behind the SizeGuard, so the home layout no longer exists at 60x15. The #1157 guarantees still apply at the floor.
- **Section:** `§03 Layout grid & breakpoints`, `§04 SizeGuard`.
- **Rejected:** moving the test to 80x24 alone, because it would drop the pin on what a 60x15 terminal shows; exempting the home from the guard.
- **Source:** #1253 (review note from the wave orchestrator).

## 2026-11-16 — ds::panel, Breakpoint and SizeGuard differ from the spec in four small ways

- **What:** (1) `Breakpoint::from_area` returns a `TooSmall` variant, not `Option<Breakpoint>` with `None`. (2) `Focus` has four variants; `Disabled` is not added. (3) The SizeGuard shows `width 72/80` and `height 12/24` as text, with no gauge bars, and writes `80 x 24` with an ASCII `x`. (4) The SizeGuard drops the "or ⌃Q to quit safely" line.
- **Why:** (1) and (2) follow issue #1253 as written. (3) a fixed-width bar does not fit the small windows the guard exists for, and the shared gauge is #1251's; ASCII needs no icon fallback. (4) today `Ctrl+Q` toggles TurboQuant, so the hint would be false until the keymap issue moves quit there.
- **Section:** `§03 Borders & focus`, `§03 Layout grid & breakpoints`, `§04 SizeGuard`, `§07 Breakpoints and the Home layout`.
- **Rejected:** `Option<Breakpoint>` (the issue names the four-class enum); adding the quit hint now (it would name the wrong key).
- **Source:** #1253.

## 2026-11-16 — Status chips: FailedGates gets its own glyph; context gauges keep 50/70

- **What:** `ds::status_chip` draws `FailedGates` as `⊠ gates failed` (ASCII `[!]`), not `✗` like `Errored`. `ds::gauge::gauge_level` (0.6 / 0.8 / 1.0) is for spend and quota only; context gauges keep 50% / 70%.
- **Why:** every status needs its own glyph so a chip reads without colour and without the word being fully visible (#1251 asks for 16 distinct glyphs). Context thresholds match the commit nudge at 50% and the auto-fork at 70% in `context_overflow.rs`; moving them to 60 / 80 would warn after the action already happened.
- **Section:** `§04 Components` (StatusChip, Gauge), `§07 ratatui implementation`.
- **Rejected:** the shared `✗` for both error statuses (two statuses, one glyph); one threshold set for every gauge (context would disagree with its own overflow actions).
- **Source:** #1251.

## 2026-11-14 — Release Console landing and UX Spine sidebar are superseded

- **What:** the redesign wins over two logged decisions: the one-landing Release Console renderer (#1218) and the UX Spine sidebar chain (#826-#835). Both are superseded by `index.html`.
- **Why:** the redesign replaces the landing with Home: Mission Control and defines the navigation the UX Spine chain was building toward.
- **Section:** `§01 Audit of the v0.31.6 landing screen`, `§02 Home: Mission Control`, `§09 Code findings & open questions`.
- **Rejected:** keeping the Release Console as the landing, or finishing #826-#835 as specced.
- **Source:** #1238 (poka-yoke, owner-delegated).

## 2026-11-14 — D1: budget at 100% pauses, never kills

- **What:** when the total budget cap is reached, maestro blocks new spawns and pauses running agents. Paused agents resume with `maestro resume`. maestro never kills agents because of the cap.
- **Why:** killing a running agent loses its work; pausing keeps it resumable.
- **Section:** `§02.14 Usage: cost, tokens and TurboQuant`, `§05 Poka-yoke catalog`.
- **Rejected:** killing running agents at the cap.
- **Source:** #1238 (poka-yoke D1).

## 2026-11-14 — D2: a confirmed quit pauses, never kills

- **What:** a confirmed quit pauses running agents and saves them as resumable with `maestro resume`. No `kill_all` on a normal exit; kill stays an explicit, typed action.
- **Why:** quitting the TUI should not destroy agent work.
- **Section:** `§05 Poka-yoke catalog`, `§02.24 Guards, release notes and small modals`.
- **Rejected:** `kill_all` on normal exit.
- **Source:** #1238 (poka-yoke D2).

## 2026-11-14 — D3: team waves advance on finish, not on spawn

- **What:** the next wave starts only when every issue of the previous wave has finished. Today's spawn-gated advance is a bug.
- **Why:** a wave depends on the work of the wave before it; spawning is not finishing.
- **Section:** `§02.17 Teams: Launch`, `§02.22 Run loop`.
- **Rejected:** advancing the wave once its agents have spawned (current behavior).
- **Source:** #1238 (poka-yoke D3).

## 2026-11-14 — #833 `new_ux` gates only Home Mission Control

- **What:** the `new_ux` flag (#833) is narrowed to gate only the new Home Mission Control, as a safe rollback during phase 2. #833 stays open.
- **Why:** one flag over one screen keeps the rollback simple.
- **Section:** `§02 Home: Mission Control`, `§08 Rollout & verification`.
- **Rejected:** a flag that gates the whole redesign.
- **Source:** #1238.

## 2026-11-14 — Milestone "TUI Redesign" for new work

- **What:** every new epic and leaf of the redesign goes in a new milestone, "TUI Redesign". "v0.32.0 — UX Spine" and the milestones of existing issues are not changed.
- **Why:** keeps the redesign trackable without rewriting history.
- **Section:** `§08 Rollout & verification`.
- **Rejected:** moving existing issues into the new milestone.
- **Source:** #1238.

## 2026-11-14 — `github.auto_merge` is a danger setting

- **What:** `github.auto_merge` is marked ▲ danger in Settings. Turning it on needs a typed confirmation.
- **Why:** auto-merge removes the human check between "CI green" and "merged".
- **Section:** `§02.15 Settings`, `§02.12 Review PRs`.
- **Rejected:** a plain toggle.
- **Source:** #1238.

## 2026-11-14 — Radial agent graph is removed

- **What:** the radial agent graph is removed: `ConcentricLayout`, its tests, and the palette entry "Agent graph (radial)" are deleted. The files lens is the only graph.
- **Why:** one graph view instead of two.
- **Section:** `§02.20 Agent graph`.
- **Rejected:** keeping the radial view next to the files lens.
- **Source:** #1238.

## 2026-11-14 — Cache lives in `.maestro/cache/`

- **What:** the TUI cache lives in `.maestro/cache/`, listed in `.gitignore`.
- **Why:** cached data must never be committed.
- **Section:** `§02.6 Roadmap`, `§09 Code findings & open questions`.
- **Rejected:** not recorded.
- **Source:** #1238.

## 2026-11-14 — Single-pass and verdict teams allowed from the TUI

- **What:** single-pass and verdict teams can be launched from the TUI, behind a typed confirmation that states roles are not routed yet.
- **Why:** the user can run them today, but must know role routing is not live.
- **Section:** `§02.16 Teams: entry, Compose, Manage`, `§02.17 Teams: Launch`.
- **Rejected:** hiding these team types until role routing ships.
- **Source:** #1238.

## 2026-11-14 — TurboQuant toggle lives in Settings

- **What:** the TurboQuant toggle lives in Settings › TurboQuant, not in the palette. ⌃Q is quit only.
- **Why:** one key, one meaning.
- **Section:** `§02.15 Settings`, `§06 Keymap`.
- **Rejected:** a palette entry or a ⌃Q binding for TurboQuant.
- **Source:** #1238.

## 2026-11-14 — No per-edit session hook; CI design-drift check enforces

- **What:** no per-edit PreToolUse hook (the logged rejection in `MEMORY.md` stands). Enforcement is `scripts/check-design-drift.sh` in CI, which blocks the merge instead of reminding.
- **Why:** a PreToolUse hook is Claude-only and noisy per edit; the CI check is provider-neutral and runs where merges happen.
- **Section:** this directory (see [README.md](README.md)).
- **Rejected:** a Claude-only PreToolUse hook. Revisit a SessionStart pointer hook only if two consecutive PRs ship design drift the CI script missed.
- **Source:** #1238.

## 2026-11-14 — §09 remainder: undo, `maestro:ready`, `due_on`

- **What:** ⌃Z undoes only reversible local actions announced by a toast. Milestone-wizard issues are NOT labelled `maestro:ready` by default (the gatekeeper decides). `due_on` comes from GitHub REST `due_on` (Azure `finishDate`).
- **Why:** answers the open questions left in §09.
- **Section:** `§09 Code findings & open questions`, `§02.3 Create milestone (wizard)`.
- **Rejected:** not recorded.
- **Source:** #1238.

## 2026-11-16 — Theme tokens v1: `border_focused` kept parsing, preset values outside dark

- **What:** `border_focused` stays in `ThemeOverrides` (parsed, ignored) and is removed from the runtime `Theme`; focus uses `border_active`. Light, retro and high-contrast values for the four new tokens come from the `--t-bar`, `--t-pn`, `--t-bl` and `--t-mg` mock palette in `index.html` (light and dark-ANSI presets keep named colours for the status tokens). `status_needs_pr` / `status_conflict` exist but `status_color` still maps `NeedsPr` / `ConflictFix` to the old fields until a screen issue moves them.
- **Why:** deleting the key would break loading of existing `maestro.toml` files; §03 gives only dark values; rewiring `status_color` changes colours on every screen and is screen-issue scope.
- **Section:** `§03 Color tokens`.
- **Rejected:** deleting `border_focused` from config; inventing new hues for light/retro.
- **Source:** #1247.

## 2026-11-16 — NoColor known gap: agent badge colours bypass the theme (RESOLVED 2026-11-17, #1346)

- **What:** under `NO_COLOR` / `TERM=dumb` / `preset = "no-color"`, the agent badge still draws colour, because `agent_color` in `crates/maestro-tui/src/tui/agent_badge.rs` returns hard-coded colours (e.g. `Color::LightYellow` for Claude) instead of reading `&Theme`. The first NO_COLOR snapshot (`overview::panel_view_single_running_session_no_color_snapshot`) pins this as one `fg: LightYellow` cell.
- **Why:** routing agent colours through the theme is screen/widget scope, not part of the token issue. The snapshot keeps the leak visible until it is fixed.
- **Section:** `§03 Color tokens` (no-color preset: "All colours Reset").
- **Rejected:** fixing `agent_badge.rs` inside #1247 (scope); making `no_color_snapshot` assert no colour now (it would fail on this known gap and block the harness).
- **Follow-up:** a new issue to route `agent_color` through the theme (Reset under NoColor). The badge already carries the agent's name as text, so no meaning is lost.
- **Source:** #1247 (judge finding on PR #1345).
- **Resolved:** #1346 moved the colours into `Theme` as seven `agent_*` tokens read through `Theme::agent_color(agent_id)`; the no-color preset sets them all to `Reset`, and `agent_badge::agent_color` is gone. `no_color_snapshot` now panics on any non-`Reset` fg, bg or underline colour, and the overview NO_COLOR snapshot has no colour left.

## 2026-11-16 — StatusBar: one row, chips replace the banners

- **What:** `ds::status_bar` is one row with no border. The `▲ BYPASS` banner overlay is gone (the chip replaces it, no coexistence). An available update is only the `↑ <version> u` chip; the "New version X available" banner arm is removed and the activity-log line no longer names the version. Downloading / restart / failed upgrade states keep their banner, drawn left of the pinned safety chips. Drop order is update, TurboQuant, quota, continuous-run chip, budget, fleet; the issue's "update then quota" order wins over the mock, where the update chip never drops. Uptime, RAM and `MOD:` are not in the bar. `CONTINUOUS: n/m done` stays as a chip only while a continuous run is active (owner decision on PR #1360): it drops after quota and before budget, because run progress is live state you act on while quota is a slower signal, and money and fleet counts matter more. It is reverse `accent_info` + bold, so it reads under NO_COLOR; `TQ:ON` stays (pinned by the #1162 header test, and the mock shows TurboQuant). Fleet `run` also counts CI fix, conflict fix and retry; `wait` also counts interactive sessions (the chat section counts its turn as `◐ wait`).
- **Why:** the shell rule puts state in one place; two places for one fact is what the redesign removes. The upgrade progress states carry prompts (`[y]/[n]`) that are not state, so they stay until the upgrade issue.
- **Section:** `§04 StatusBar`, `Shell rules`, `§08` metric "Places the update notice appears".
- **Rejected:** keeping the bypass banner during a migration (two places for one fact); width breakpoints instead of greedy dropping (a long quota badge would still overflow); mode chip (`NORMAL` / `INSERT`) — no App state for it yet.
- **Known gap:** the budget alert banner (`draw_budget_banner_if_alerting`) still overlays the bar row and can hide the safety chips while it shows; it is outside #1257's ownership.
- **Source:** #1257.

## 2026-11-16 — Kit v2 pieces: `+n` overflow, three-column tree, no loading/error tile yet

- **What:** `ds::filter_chips` ends with `+n` (hidden count) instead of the mock's `…`. `ds::tree_prefix` uses three columns per level (`├─ `, `└─ `, `│  `, blank under a last ancestor) instead of the mock's two (`├ `, `│ `), and has no `▾ ▸` expander. `ds::kpi_tile` ships the ready and empty forms only; the delta is drawn in a neutral colour.
- **Why:** the issue asks for `+n` and for `├─ └─ │`; a count tells how much is hidden, `…` does not. Blank space under a last ancestor is what makes nested levels read right. The issue lists value, delta, sparkline, context and the empty reason; loading and error forms wait for the Stats screen issue that fetches the data. Up is good for some metrics and bad for others, so colour by direction would lie half the time.
- **Section:** `§04 Kit v2` (`subTabs`, `filterChips`, `kpiTile`, `treePrefix`), Stats KpiTile caption.
- **Rejected:** `…` overflow (hides the count); a `│ ` rail at every ancestor level (wrong under last children); green-up / red-down delta.
- **Source:** #1256.

## 2026-11-17 — Activity log: pane on Home and Dashboard, drawer elsewhere; `activity_log_height` sizes both

- **What:** `ui::draw` draws the Activity pane only on Home (`Landing`), Dashboard and the session `Overview` (merged into Dashboard by the design), and under a modal on top of one of them (`ConfirmExit`, `ConfirmKill`, `BudgetPreSpawn`). Every other screen gets the rows back. There, each new WARN/ERR entry raises a `ds::toast` (`[label] message`; warn 6 s, error sticky), and `Ctrl+L` toggles the log drawer (`ds::log_drawer`): a Rounded `Focus::Overlay` box on `surface_overlay` over the bottom of the main area. While shown, the drawer is modal: it swallows every key except `Ctrl+L` and `Esc` (both close it) and the quit chords that already bypass screens (`Ctrl+C`, `Ctrl+X`, the `ConfirmExit` routing), and the footer shows only `Esc Close log  ⌃L close`. `tui.layout.activity_log_height` is kept and sizes both the pane and the drawer (same percentage, same 10..=50 clamp). The Interaction composer's "clear input" moves from `Ctrl+L` to `Ctrl+G`.
- **Why:** answers the §02.15 open question "Is `activity_log_height` still needed with the log drawer?": removing the key would break existing configs silently, and one key for "how tall is the log" is enough. Toasts keep WARN/ERR interrupting once the pane is gone; Info stays in the log only. Entries shown in the pane or the open drawer raise no toast, so switching tabs does not replay old warnings.
- **Section:** `Shell rules` (Activity log row), `§02.15 Settings` open question, `§02.21 Interaction` composer key table (`⌃L` global log drawer, `⌃U` clear).
- **Rejected:** `Ctrl+U` for clear input, which §02.21 (Interaction) proposes: the composer is a `tui_textarea::TextArea`, and `Ctrl+U` is its built-in undo (tui-textarea 0.7 `input`), so catching it would silently drop undo. `Ctrl+G` is free in tui-textarea's default keys (it uses `Ctrl+A B C D E F H J K M N P R U V W X Y`) and in maestro's globals and Interaction keys (`Ctrl+Q B X C Z L`, `Ctrl+W P D`). Also rejected: removing `activity_log_height` (silent break); a drawer of fixed height (a second knob for the same thing); exempting the Interaction screen from `Ctrl+L` (the design makes `⌃L` global; two meanings for one chord).
- **Modals:** under the help overlay, the modal and confirm modes `ConfirmExit`, `ConfirmKill`, `BudgetPreSpawn`, `QueueExecution`, `QueueConfirmation`, `CompletionSummary`, `ContinuousPause`, `HollowRetry`, `AdaptFollowUp`, `CiErrorReview`, `SessionSwitcher`, `BypassWarning`, and the confirms inside a screen (Settings discard, Interaction quit, Adopt picker / Fleet board live confirm), the open drawer is hidden, not closed, and `Ctrl+L` / `Esc` go to the modal. Hiding keeps the user's choice without hooks on every mode change; closing would lose it. Drawing over them could hide part of a confirm that still takes keys (the BudgetPreSpawn spend confirm takes `y`/`n`). Owner decision on PR #1366: the drawer is modal while shown. Rejected: a per-popup block list only — it is never complete (every new popup or in-screen confirm would have to be added), so a key could still fire something hidden under the drawer.
- **Known gap:** the notification banner (`draw_notification_banner`) still draws in the pane's rows, so it is hidden on screens without the pane, as it already was when `d` hides the log. The gh-auth banner has the same limit, but the auth loss also logs an ERR, which now raises a sticky toast.
- **Source:** #1263.

## 2026-11-17 — Needs-you queue: kind first, age second; title live before the Home panel

- **What:** `ds::attention` ranks by kind (error > waiting on you > ready to ship > resource), then by age (oldest first); same-age items keep input order. Failed doctor checks and lost gh auth rank with errors; a failed `gh auth` doctor check is dropped when the lost-auth row is shown, so the fact appears once. The row has no title column of its own: the issue title leads the reason (`auth-refactor · completion gates failed`). The idle empty state keeps the issue's line `Nothing needs you. Start something: i f a` and adds the mock's key line under it. Quota notices start at 80% of the busiest window (the `High` gauge level). The terminal title `(n) maestro` is written from the run loop after each draw, only when it changes.
- **Why:** an old failure must still beat a new review request (ADR stub in #1259). Plain inputs keep `rank` testable without an `App`. One title write per change avoids sending an escape sequence every frame.
- **Section:** `§04 AttentionQueue`, `§02 Home: Mission Control` (Needs you), `§05` "Missing an agent that waits on you".
- **Rejected:** age across all kinds (a new failure could sit under an old PR); per-row action letters (`p` collides with PRD on Home).
- **Known gap:** the panel is not on Home yet and `⏎` does not jump (Home layout issue). `CiFix` / `ConflictFix` sessions are not rows (an agent is working on them). Claim conflicts are not rows: the claim-conflict module acts on the clash (warn / pause / kill) and keeps no list to read. On quit, the normal terminal restore in `tui::run` writes `exit_title()` (`maestro`, no count) through `write_exit_title`, so the shell tab keeps no stale `(n)`. Plain `maestro` was chosen over an empty title: what a terminal does with an empty title varies (some may restore the shell title, others show a blank tab), while `maestro` reads the same everywhere and many shell setups set their own title at the prompt anyway. The upgrade restart path (owned by #1377) does not; the new process sets the title again. A panic skips it (maestro has no panic hook that restores the terminal).
- **Source:** #1259.

## 2026-11-17 — Home Mission Control replaces the Release Console on Home

- **What:** `TuiMode::Landing` draws Mission Control (`screens/home/mission/`): Needs you (focused on open), the Fleet table (`ds::fleet_table`), Actions grouped RUN · PLAN · INSIGHT with `⊘` + reason for disabled ones, Budget & quota and the Milestone card, at Wide / Regular / Compact. The Release Console, logo and network chart no longer draw on Home; `LandingScreen` stays for its state and the help entry. The Activity region is the shell's Activity pane under Home (full width, `ui::draw`), not a pane inside the center column. `⏎` on a session row opens the session `Overview` with that session selected, because today's `Dashboard` (`HomeScreen`) has no session rows; doctor, gh auth, budget and quota rows open `Dashboard`. Wide keeps the right column under Budget & quota empty as the Insight slot. The Actions keep the old letters and labels (`r` Roadmap, `p` PRD, not the mock's `5` / `7`, which are tab digits). Disabled reasons today: `gh signed out` (`i m h`) and `no milestones yet` (`h`).
- **Why:** §01 counts ~17 decoration rows at 60 rows on the old Home; the ADR stub in #1264 recommends replacing the landing renderer rather than keeping the console as a Wide-only panel. Reusing the shell's Activity pane keeps one log renderer and the #1226 / #1263 height rules. Overview is where a session row is selectable until the Dashboard redesign (operate epic) lands.
- **Section:** `§01 Audit of the v0.31.6 landing screen`, `§02 Home: Mission Control`, `§03 Layout grid & breakpoints`.
- **Rejected:** keeping the Release Console as Home's Wide-only panel (keeps the decoration rows on the default screen); drawing a second Activity pane inside the center column (two log renderers); an `IconId` for `⊘` (the glyph is drawn as-is, like the `▌` marker); the `new_ux` flag of #833 (no such flag exists in the code yet; this PR does not add it).
- **Known gap:** the "which model routing picks" row of Budget & quota, Milestone next issues / blockers, and the Adopt `no outside sessions` reason are not drawn (no cheap data source on Home today). Hints are not in panel titles (`f full board`, `o details`, `h review` of the mock) because the shell rules keep hints in the footer. The Fleet panel takes the leftover height of its column.
- **Source:** #1264.

## 2026-11-17 — Home `⏎` on a session row opens the session Overview

- **What:** `⏎` on a Needs-you session row (gates failed, errored, needs review, PR ready) opens `TuiMode::Overview` with that session selected and highlighted: `PanelView::reveal(index)` sets `selected` and a one-shot `pending_reveal`; the next Overview draw turns it into a grid position (page, row, column) with the layout it draws with (`GridState::select_index`). Setting `selected` alone was not enough: Overview highlights from `grid_state`. The issue said "lands on Dashboard with that session selected"; the test keeps its name `home_enter_attention_opens_dashboard_row`.
- **Why:** today's `TuiMode::Dashboard` (`HomeScreen`) has no session rows, so nothing there can be selected. Overview is the session grid that #1263 already treats as part of the Dashboard. When the Dashboard redesign (operate epic) adds session rows, the jump moves there.
- **Section:** `§02 Home: Mission Control` (Needs you), `§02.1 Dashboard: operations cockpit`.
- **Rejected:** pushing `Dashboard` with no selection (the operator lands on a menu and must find the session again); `Detail(id)` (skips the fleet context and is not part of the Dashboard). Computing the grid position in the key handler from `crossterm::terminal::size()` (the arrow keys' layout): the Overview draws in the body area, not the whole terminal, so page and row could differ.
- **Source:** #1264.

## 2026-11-17 — Pre-spawn budget gate wired in production; Block offers only `n`

- **What:** `App::install_budget(&[budget])` builds the `BudgetEnforcer` and a `DefaultBudgetProjector(per_session_usd, DEFAULT_FLOOR_PCT = 0.10)` together. Startup (`setup.rs`) and config reload (`screen_dispatch.rs`) both call it, so the gate and the modal's projected total use the same numbers. On Block (`spent + projected ≥ total_usd`), `y` and `s` do nothing and the modal footer reads `[n]o over the cap, cannot spawn`; Warn keeps `[y]es [n]o [s]kip`.
- **Why:** the projector was only set by `with_budget_projector` in tests, so the gate never fired for users (§09 Budget row). A `y` that spawns past the cap would make Block the same as Warn; a footer that offers a key that does nothing is a lie.
- **Section:** `§02.24 Guards, release notes and small modals` (modes table row `BudgetPreSpawn`, audit row 5, Implementation "Budget gate": "`S` → spawn (rejected on Block)"), `§09 Code findings & open questions` (Budget row: "the budget projector is only set in tests").
- **Rejected:** wiring only in `setup.rs` (config reload would rebuild the enforcer and leave a projector from the old `per_session_usd`); a new `[budget]` key for the floor (no request for it; 0.10 is the value the projector tests use). The §02.24 queue-and-toast redesign (`Esc` keep, `d` drop, `b` settings) is not part of this change.
- **Known gap:** the status-bar hints for `BudgetPreSpawn` (`navigation/mode_hints.rs`) still list `y Proceed` / `s Skip-once` on Block; that file is owned by the keymap work (#1248) in this wave.
- **Source:** #1243.

## 2026-11-17 — Keymap registry declares keys; dispatch stays where it is

- **What:** `navigation::registry` declares bindings (`Chord`, `Scope`, `Action`, `Danger`, label) and `check` encodes the §06 rules (no duplicate in a scope, no inner key shadowing an outer one, no destructive bare lowercase letter, no safe bare uppercase letter without a documented exception). It holds the §06 Global table, the keys `handle_global_shortcuts` takes today (`⌃L ⌃B ⌃X ⌃C`, Shift `Q M C P`, the Shift ones as documented exceptions until the globals issue retires them) and two screens, Log viewer and Session summary. Their footers now come from the registry: `j Down  k Up  g Top  G Bottom` and `j Down  k Up  Enter Expand`. `Esc Back` left those two footers because `Esc` is a Global key and the crumb bar already shows `Esc back`; `j/k` is now two hints because the registry has one binding per chord.
- **Why:** the ADR stub in #1248: rewriting dispatch for every mode in one PR is the big-bang the rollout avoids. A declaration checked by a test catches collisions now; screens move one at a time.
- **Section:** `§06 Keymap`, `§07 ratatui implementation` ("Keymap registry: collisions fail the build"), `§04` keymap registry.
- **Rejected:** dispatching keys from the registry in this PR (touches every handler); the `Reversible` danger level of the §07 sketch (the issue asks for `Safe | Destructive`; add it when a rule needs it); a confirm-step check (`requires_confirm` needs dispatch to know about the registry); merging `j`/`k` into one `j/k` hint (would need leaked or owned strings for a `&'static` hint key).
- **Known gap:** `⌃K` and `⌃Q` are declared as §06 targets; today `⌃K` is unbound and `⌃Q` toggles TurboQuant (#1260). `Shift+P` is declared `Destructive` (a GitHub write) but has no confirm step. F2–F12 are not declared (they come from the mode F-key table).
- **Source:** #1248.

## 2026-11-17 — Teams Launch Executing step lists waves; a wave waits for the previous one to finish

- **What:** `run_team` starts wave n+1 only when every session of wave n is terminal (`TeamLauncher::wait_until_terminal`, `team_verdict`). Paused, stalled and retrying sessions keep the next wave waiting; a killed, errored, hollow or gates-failed session fails the wave and skips the waves after it (`skip_reason: wave n failed: #N …`). The Executing step (`team_wizard/launch_waves.rs`) draws one line per wave: `Wave n  #a #b  finished | running | waiting on wave n-1`, fed by `TuiDataEvent::TeamWaveStarted`. Without a plan it keeps the old `Dispatching team run…` state.
- **Why:** waves exist because later issues depend on earlier ones; starting them on spawn ran dependents on a base without their blocker's work (#1246). The flow map edge `wave spawned → next wave starts (bug)` becomes `wave finished → next wave`.
- **Section:** `§02.17 Teams: Launch` (result panel, "spawned ≠ finished"), flow map node `team.waves`.
- **Rejected:** having the TUI push terminal transitions into the runner per session (the runner already owns the level loop, so a completion wait per spawned session is simpler); drawing waves with `status_chip` (that chip is for a `SessionStatus`, a wave is not one).
- **Known gap:** the Launched step still reads `Team run dispatched.`; it now shows only after the last wave finished. The per-issue result panel of §02.17 is not drawn yet. If the fetched issue never becomes a pool session (for example a launch guard refuses it), the wave waits until the user cancels.
- **Waits with no end of their own:** a wave can hold forever on a Paused session (only a resume or a kill moves it), a Stalled session past its retries (only a kill), or a NeedsPr session whose PR retries ran out (it cannot even be killed today). `Esc` on Executing is the way out: it cancels the run (`run_team_until` + `CancellationToken`), starts no later wave, aborts the tasks still spawning or waiting, leaves running sessions alone (kill them from the Dashboard), and lands on Launch Failed with `cancelled by user while waiting on wave n`. The footer reads `Waves run in order   [Esc] cancel run`. A failed wave's banner now reads its `skip_reason` (`wave n failed: #N …; k downstream issue(s) skipped`), and the terminal-state banner wraps so long reasons are not cut.
- **Rejected (Esc):** Esc leaves the wizard while the run goes on (no screen would show the waves or offer a cancel); aborting only the outer task handle (the per-issue tasks would be orphaned and could still spawn).
- **Source:** #1246.

## 2026-11-18 — Interaction quit modal says what `y` removes

- **What:** the `Ctrl+W` modal on the Interaction screen now reads, one fact per line: `Quit interaction? Removes worktree`, the worktree path, `and local branch <branch>.`, `Unpushed commits are lost; any remote branch or PR stays.`, `Press y to quit, N to keep working.` With no trusted worktree root it reads `Quit interaction? No isolated worktree, so nothing is removed.` The copy comes from `InteractionScreen::quit_modal_lines`, which checks the same empty-`worktree_root` test as `begin_quit_teardown`. The RC4 "Unpushed work will be LOST" second-`y` line keeps its words. Lines wrap (`Wrap { trim: true }`) and the modal height follows the wrapped row count, so at the 80x24 floor a long path wraps instead of being cut, and so does the RC4 line. The width is measured in display columns.
- **Why:** the old text said the worktree was "kept for manual inspection", but `y` wiped the worktree and deleted the local branch (#1242). A confirm that promises the opposite of what happens is the worst kind of dialog.
- **Section:** `§09 Code findings & open questions` (Interaction audit row 1: "The dialog promises the opposite of what happens"), `§02.21` End chat dialog ("deleted / lost / kept" rows).
- **Rejected:** keeping the worktree on quit (teardown on quit is the designed behaviour, #941, and kept worktrees leak disk); building the §02.21 End chat TypedConfirm dialog now (it belongs to the redesigned Interaction screen, out of scope here).
- **Rejected (layout):** one long line per sentence (cut at 80 columns, and a real `<repo>/.maestro/worktrees/issue-N` path pushed the branch off the modal even at 120).
- **Known gap:** the modal is still `Ctrl+W` + `y` and does not read git for the real unpushed-commit count; the §02.21 dialog replaces it. With no trusted root, a `produce_pr` session with no PR still gets the RC4 "Unpushed work will be LOST" line on the first `y`, although nothing is wiped; that line is out of scope here.
- **Source:** #1242.

## 2026-11-18 — TypedConfirm for kill and the bypass warning

- **What:** `ds::confirm::TypedConfirm` (Double border, `Input::single_line`, `Esc` default, `⏎` inert until the typed text equals the token). Kill from the Dashboard asks for the session's issue number (a session with no issue: the first 8 characters of its id); `y`/`n` are now plain text. The bypass warning (`⌃B`) asks for `CONFIRM` then `⏎`, replacing the hand-rolled field that cancelled on the first wrong letter and confirmed on the last right one. The `ConfirmKill` footer comes from the registry (`Enter kill`); `Esc` is the Global back. Both dialogs are drawn last, over the whole frame (`ui::draw_typed_confirm`): inside the content row the activity log cut them at 80x24.
- **Why:** design §05 "Killing the wrong agent" / "Confirming by reflex". The number proves you kill the session you mean; a word only proves intent, so it is kept for global actions (the #1261 ADR stub). Making both modes text-input modes also fixed a live bug: the global `Shift+M` ate the `M` of `CONFIRM`, so the bypass warning could not be confirmed.
- **Section:** `§02.13 Status: session views` (Dashboard "Kill confirm `ds::confirm::TypedConfirm`", spec `dashboard.killConfirm`), `§02.15 Settings` ("Danger confirm"), `§04` `confirm`.
- **Rejected:** typing the word `KILL` for kill (proves intent only); a `y` that still kills (a reflex key); keeping the prefix-check field in `bypass_warning.rs` (hand-rolled input, no backspace).
- **Known gap:** the key is still `k`, not the target `X` (not in #1261). The kill dialog does not show status, elapsed time, cost, the worktree path or the edited-file count from the spec; it says what `pool.kill_session` keeps and loses in plain words. The mode chip does not switch to `INSERT` while typing. The bypass text keeps the run-only wording of `⌃B`; the §02.15 Settings variant (file write, `⌃B` safer) belongs to #1239.
- **Source:** #1261.

## 2026-11-18 — Settings bypass toggle opens the existing bypass warning

- **What:** turning the Settings › Sessions bypass row on no longer stages `sessions.permission_mode = "bypassPermissions"`. The row stays pending and `App::process_pending_settings_bypass` opens the existing `BypassWarning` screen with Settings text, every time (even after ⌃B was acknowledged this run, because Settings writes the committed `maestro.toml`). Typing `CONFIRM` then ⏎ stages the value and turns bypass on through `confirm_bypass_activation("settings (typed CONFIRM)")`; `Esc` or a wrong letter turns the row back off and stages nothing. `App::answer_settings_bypass` (one additive check in `bypass_dispatch.rs`) answers a pending Settings request; the ⌃B branches are unchanged, so there is one warning and one activation function.
- **Unanswered warning:** a pending Settings request belongs to the warning it opened. If that warning is left through a global chord, ⌃B turns the pending row back off before it opens its own warning (so a ⌃B CONFIRM is logged as `tui` and stages nothing), and coming back to Settings closes the stale warning and turns the row off. A row that was already staged on is not touched.
- **Why:** two paths reached the same dangerous state with two different protections (#1239, §09 Settings row, §06 `Ctrl+B` row "Settings must route through it too").
- **Section:** `§02.15 Settings` (row 2 of the audit, poka-yoke row "Turning on bypass for the whole repo by reflex", Implementation "Log the bypass source as settings (typed CONFIRM)").
- **Differs from the target:** §02.15 says confirming only stages the change. Here the confirm also turns bypass on for this run, like ⌃B, so the one activation function writes the audit line (owner to decide; see the PR). The dialog is #1261's `TypedConfirm` through `BypassWarningState::with_text`: title `DANGER: enable bypass for every future session?`, rows `writes sessions.permission_mode to maestro.toml (committed)`, `scope every future session after Ctrl+S; this run now`, `or Ctrl+B turns it on for this run only`. ⌃B keeps `new()` (`THIS session only`).
- **Rejected:** removing the toggle from Settings (it is the documented sole UI for `sessions.permission_mode`); a second warning inside Settings (two doors again); routing through `request_bypass_toggle` as is (it turns bypass off when it is on and skips the warning after the first ack).
- **Source:** #1239.

## 2026-11-18 — The queue reads each item's own session; hollow is a failure; dependent picks do not start

- **What:** every session that enters the pool for the running queue item's issue is recorded on that item (`QueueExecutor::set_session_id`, called from `add_session`, so an auto-retry moves the record to the new attempt). `App::settle_queue_item` reads the outcome from that session only: `Completed` and not hollow (or a satisfied consultation) advances the queue; anything else marks the item failed, and the Queue overlay shows `#N failed — choose action` with `[r] Retry  [s] Skip  [a] Abort`. No recorded session yet (the issue is still being fetched) is not an outcome; a failed fetch of the running item's issue fails the item, so the overlay offers the decision instead of waiting forever. The hollow-retry modal does not open for the session the queue is still deciding on (the running item's, or the failed item's awaiting r/s/a); when the queue takes that decision it sets `hollow_dismissed`, so a retry does not reopen the modal. Any other session, even a manual one on a queue issue, keeps the modal. LaunchQueue builds its work items from the fetch cache or the Issues browser list (`App::lookup_issue`), so `Blocked-by` reaches the dependency graph; when two selected issues depend on each other nothing starts, the user stays on Queue confirmation, and an error toast reads `Queue not started: Issue #B depends on #A, both are selected`.
- **Why:** §02.22 Run loop finding 1 and "Hollow ≠ done": the outcome came from `pool.all_sessions().last()` (any session added last), so a hollow run, or the previous item's result, advanced the queue and spent money on work that depends on it. With real bodies, the old "launch all at once" fallback would have run a dependent before its blocker.
- **Section:** `§02.22 Run loop` (audit row 1, Implementation "Hollow ≠ done"), `§09 Code findings & open questions` (Run loop row).
- **Rejected:** opening the hollow modal on top of the queue (its `[r]` spawns a retry the executor never sees, so the item stays stuck or runs twice); ordering dependent picks topologically (that is the run-plan rewrite, out of scope here); keeping the parallel fallback (it ignores `Blocked-by`).
- **Known gap:** the queue still waits for every pool session (`all_done`) and runs one item at a time; the slot-based loop is the run-plan issue. A session for the running item's issue started outside the queue (by hand) is also recorded on the item; telling them apart needs the launch to carry a queue tag.
- **Source:** #1244.

## 2026-11-18 — ⌃Q quits; every exit chord goes through Confirm Exit with a hold

- **What:** `ds::confirm::HoldToConfirm` (repeat-or-double-tap, `HOLD` 0.5 s, `WINDOW` 2 s; release events, `⏎` and other keys are inert). ⌃Q and `q` quit at once when there is no live work; with live work (pool active + queued sessions, an unfinished queue executor, also between items or awaiting a decision, a completion / auto-PR job in flight, or a team run until its result lands) they open Confirm Exit, and only holding ⌃Q (or pressing it again inside 2 s) quits — `y` and `⏎` do nothing, `Esc` and `n` stay. In the idle dialog `y` and ⌃Q quit. ⌃C and ⌃X always open Confirm Exit, also under the SizeGuard (⌃C used to quit there). Exit chords ignore key release events. The ⌃Q TurboQuant toggle is gone: once per launch, before the first frame, a toast and an activity-log line say that ⌃Q quits and that TurboQuant is in Settings › TurboQuant; the first ⌃Q is never swallowed (owner decision on the #1410 review). The first ⌃X shows `⌃Q quits from anywhere`. The footer pin is `⌃Q quit`. The popup is sized from its lines and drawn last over the whole frame, so the question and the key row show at 80x24 on Home and Dashboard (B1/B2 of #1394).
- **Why:** design §05 "Quitting through an alias" / "Quitting while agents run", §06 `⌃Q`, §09 "⌃X/⌃C quit with no confirm".
- **Section:** `§04` `confirm` (`ds::confirm::{HoldToConfirm, TypedConfirm}`), `§05` poka-yoke rows, `§06` Global keymap.
- **Differs from the target:** the hold key is ⌃Q, not `Q` (`Q` is the global TurboQuant dashboard key; reusing it inside the dialog would shadow it in the registry). The title stays `Confirm Exit` (the crumb names it) and the dialog says `N session(s) still running. Exit anyway?` instead of "Quitting pauses them": the pause-and-resume exit is #1266. The notice is kept per launch, not persisted, so it shows on every launch. With no sessions the dialog names the work: `A queue is waiting for your decision.`, `A team run is between waves.`, else `A queue or PR job is still running.` (then `Exit anyway?`). An issue fetch in flight for a manual launch (`spawn_issue_fetch`) is not counted: it sets no App state. The idle dialog keeps the old `[y]es [n]o` row.
- **Rejected:** true hold via key-release events (needs `KeyboardEnhancementFlags::REPORT_EVENT_TYPES`, which many terminals ignore); letting ⌃C twice quit (people mash ⌃C); persisting the notice flag (needs a state-file write outside this issue); a first ⌃Q that only shows the hint (it swallowed the quit; owner rejected it).
- **Source:** #1260.

## 2026-11-18 — Dashboard Quick Actions scroll to the selection

- **What:** the Dashboard Quick Actions panel (`screens/home/draw.rs`) draws only the rows that fit and scrolls so the selected row is always drawn: `offset = screens::wrap::scroll_offset_for_cursor(selected_action, inner height)`, the existing helper the prompt and issue-browser inputs use. Hidden rows show on the bottom border: `↑ N more` on the left, `↓ N more` on the right, in `text_muted`. At 80x24 the panel shows 6 of 13 rows; after `j`×12 it shows `c t Q S u q` with `q Quit` highlighted and `↑ 7 more`. Enter still runs `selected_action`, which is now always the drawn, highlighted row.
- **Why:** #1396 (smoke test B3/R5 at `de407c5e`): `j` moved the selection onto rows below the panel, nothing looked selected, and Enter ran a hidden row (Confirm Exit, Update, Settings). `S`, `Q`, `t`, `u` were never visible at 80 columns.
- **Section:** `§02.1 Dashboard` ("The Quick Actions list is deleted, not moved"). The target removes this list (#1280); until then it must not run hidden rows. The marker follows the target's overflow rule (`↓ n more` on the border, §02.1 Work list and the Issues list). `↑` goes on the bottom border too, because the top border holds the title and has no room at 80 columns.
- **Rejected:** `List` + `ListState` (the ADR stub's pick): the markers change the block before render, but `ListState` only knows its offset after render, and the panel already ellipsizes labels by hand. Marker rows inside the panel (they cost one or two of the 6 rows at 80x24). A stored, sticky offset on `HomeScreen` (the stateless pin-to-bottom offset is enough for 13 rows and needs no new state).
- **Source:** #1396.

## 2026-11-18 — gh auth comes back without a restart

- **What:** `App::gh_auth_ok` turns true again. Any gh call that succeeds (issue, milestone and PR fetches, a submitted PR review, a Shift+P retry that opens its PR) flips it back and logs `GitHub authentication restored.` once. A PR action that would be refused for lost auth first runs one `gh auth status` (`app/gh_auth.rs`, on the blocking pool, inside the completion job, at most 5 s; a slower answer counts as signed out and the `gh` child is killed) and goes ahead when gh is signed in again. While false, the log row draws `⊘ gh auth expired · PR create paused · fix: gh auth login, then Shift+P` (`tui/gh_auth_warning.rs`) instead of the old "GitHub CLI not authenticated" banner.
- **Why:** #1241, §09 Dashboard row 10: one auth error locked PR work for the life of the process.
- **Section:** `§05` "PR action while gh is signed out", `§09` Dashboard row 10, flow-map node "gh auth expired · PR rows ⊘".
- **Differs from the target:** there are no PR rows on the Dashboard yet (#1277), so only the header line shows the state. The target's `⏎` recheck key is not wired; the fix names `Shift+P`, the existing retry key. The line still sits on the activity-log row (moving it is the status bar issue).
- **Rejected:** a recheck on every gh call (a subprocess per call; the ADR stub's latency concern); a recheck on draw (blocks the render loop); a new `RepoProvider::auth_status` trait method (every fake provider would need it; a closure probe is enough).
- **Source:** #1241.

## 2026-11-18 — ratatui 0.30 port: ratatui-textarea, Shift+Tab, agent-graph pixels

- **What:** ratatui 0.29 → 0.30, crossterm 0.28 → 0.29, and `tui-textarea` 0.7 → `ratatui-textarea` 0.9 (same `TextArea` API; `cursor()` now returns `DataCursor`, read back as `(row, col)`). Three behaviour fixes keep the screens as they were: (1) `widgets::input::feed` is the one key mapping every editor uses to feed a `TextArea` (the canonical `Input` and the seven sites that still hold a raw `TextArea`); it drops Shift+Tab, so Shift+Tab never types and screens keep it for moving focus back; (2) `agent_graph::render::floor_shift` moves edges and node boxes half a grid pixel so they land on the same cell as in 0.29; (3) nothing else. One snapshot changes: the landing Internet chart midline moves one braille dot down.
- **Why:** ratatui-textarea maps crossterm `BackTab` to Shift+Tab, and its default keymap inserts a tab for it (tui-textarea 0.7 mapped it to `Key::Null`), so Shift+Tab typed spaces into the milestone goal field. ratatui 0.30's canvas `Painter::get_point` rounds to the nearest pixel (0.29 floored), but `ctx.print` labels still floor: without the shift, 24 agent-graph snapshots moved, file labels got a one-cell gap to their edge (the #569 regression) and flat edges stepped a row. The landing midline sits at y = 0, an exact .5 tie on that chart, so it rounds one dot down; the chart still reads the same.
- **Section:** `§02.20 Agent graph` (edges touch their file labels), `§02.21 Interaction` and `§02.3 Create milestone` (composer and wizard key handling). No visual target changes.
- **Rejected:** accepting the 24 agent-graph snapshot changes (brings back the #569 gap); snapping node positions to cell centres (tried: labels placed by `place_label` then drift, more snapshots changed); staying on ratatui 0.29 until tui-textarea ships a 0.30 build (blocks Dependabot, and tui-textarea moved to ratatui-textarea); keeping two ratatui copies with `deny.toml` skips (types do not match, it does not build).
- **Source:** #1413.

## 2026-11-18 — Help overlay scroll stops at the end; key column fits the longest key

- **What:** the Help overlay (`tui/help.rs`) measures its wrapped content with `Paragraph::line_count` at the inner width it draws with, clamps the scroll to `rows - inner height`, and writes the clamped value back into `HelpOverlayState`, so `j` / `PgDn` stop with the footer `[j/k] Scroll  [/] Search  [?/F1/Esc] Close` on the last row and one `k` moves up at once. The key column is per mode: the longest key + 2, capped at a third of the inner width; a key longer than the cap gets one space. On Landing the longest key is `^Q / q / Ctrl+c / ^X` (20), so the column is 22 (was a fixed 16) and `d/f/a/i/m/r/p/h/s/t` no longer runs into `Run an action`.
- **Why:** #1397 (smoke test B4/B5 at `de407c5e`): `j` scrolled the box to blank, and a 19-character key touched its description.
- **Section:** `§06 keymap` (flow-map node `start.help`, "Help (? / F1)"). No visual target changes; the Help content itself is #1398.
- **Unstable feature:** the row count comes from `Paragraph::line_count`, behind the ratatui feature `unstable-rendered-line-info` (no new crate, `Cargo.lock` unchanged). Real wrapping is needed: at 80 columns `Shift+P  Manually retry PR creation for selected session` is wider than the 70-column inner box and wraps to a second row, so a plain `lines.len()` would stop the scroll one row early per wrapped line. If a ratatui bump renames or changes it, `help_scroll_clamps_to_content_end` (80x24 and 80x27) goes red.
- **Rejected:** a fixed 16 column with a forced separator (the column stays ragged for long keys); drawing Help without `Wrap` and using the plain line count (long descriptions get cut off at the border); counting rows by `width / inner width` by hand (word wrap breaks earlier, so the clamp would drift from what is drawn).
- **Source:** #1397.
