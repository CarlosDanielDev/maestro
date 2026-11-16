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

## 2026-11-16 — NoColor known gap: agent badge colours bypass the theme

- **What:** under `NO_COLOR` / `TERM=dumb` / `preset = "no-color"`, the agent badge still draws colour, because `agent_color` in `crates/maestro-tui/src/tui/agent_badge.rs` returns hard-coded colours (e.g. `Color::LightYellow` for Claude) instead of reading `&Theme`. The first NO_COLOR snapshot (`overview::panel_view_single_running_session_no_color_snapshot`) pins this as one `fg: LightYellow` cell.
- **Why:** routing agent colours through the theme is screen/widget scope, not part of the token issue. The snapshot keeps the leak visible until it is fixed.
- **Section:** `§03 Color tokens` (no-color preset: "All colours Reset").
- **Rejected:** fixing `agent_badge.rs` inside #1247 (scope); making `no_color_snapshot` assert no colour now (it would fail on this known gap and block the harness).
- **Follow-up:** a new issue to route `agent_color` through the theme (Reset under NoColor). The badge already carries the agent's name as text, so no meaning is lost.
- **Source:** #1247 (judge finding on PR #1345).
