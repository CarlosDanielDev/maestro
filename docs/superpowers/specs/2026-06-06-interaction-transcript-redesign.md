# Interaction Transcript Redesign — opencode-style chat

**Status:** Approved (brainstorm 2026-06-06)
**Milestone:** v0.30.x (standalone; lands on the current `InteractionScreen`)
**Spec owner:** Carlos

## 1. Problem

The Interaction screen (#736 → #946) renders the chat as one flat wall of
wrapped text. Two concrete failures, both seen in manual QA of PR #954:

1. **Message scope is invisible.** `history.rs::build_lines` emits a tiny
   `you ▸` / `agent ▸` prefix on the first line of each turn, then flat
   wrapped text. A long agent reply and the issue prompt blur into one
   scroll — the user cannot tell where one message ends and the next begins.
2. **Scroll does not work as expected.** Mouse wheel routes to
   `app.panel_view.scroll_up()` (`src/tui/mod.rs:209-213`) — a different
   panel — never the interaction history. Keyboard `Up`/`Down` reach the
   screen but move one line at a time; there is no `PageUp`/`PageDown`, no
   jump-to-latest, and a streaming reply re-pins the view to the tail.

Reference target: opencode (https://github.com/anomalyco/opencode) — bordered
message blocks with role + time, markdown rendering, syntax-highlighted code.

## 2. Goal

Render the transcript as opencode-style **bordered cards**: each turn in a
rounded, role-colored box titled `role · HH:MM`, body rendered as markdown
with syntect-highlighted fenced code. Make scroll work: mouse wheel, PageUp/
PageDown, jump-to-latest, and stop the streaming yank.

## 3. Non-goals

- No change to the session/turn plumbing (`interaction.rs`,
  `interaction_turn.rs`, the spawn loop). This is view-layer only.
- No new dependency. `pulldown-cmark` 0.12 and `syntect` 5 are already in
  `Cargo.toml`; `src/tui/markdown.rs` already renders both.
- No change to #946 (PR #954) — that ships as-is.
- Not folded into the v0.32.0 UX Spine; this is a focused v0.30.x polish.

## 4. Key existing pieces to reuse

- `src/tui/markdown.rs` → `pub fn render_markdown(input: &str, theme: &Theme, width: u16) -> Text<'static>`.
  Full markdown + syntect code highlighting. The card body is produced by this
  function; the redesign does NOT reimplement markdown.
- `src/tui/screens/interaction/history.rs` — current flat renderer
  (`build_lines`, `draw_history`, `visual_total`). Rewritten to emit cards.
- `src/tui/screens/interaction/mod.rs` — scroll state (`scroll_offset`,
  `auto_scroll`, `last_max_offset`, `effective_offset`, `scroll_up/down`).
  Scroll model is kept; inputs are extended.
- `src/tui/screens/interaction/keymap.rs` — `classify()` intent map. Extended
  with PageUp/PageDown/End.
- `src/tui/mod.rs:209-213` — mouse scroll routing. Fixed to reach the active
  screen when it is the Interaction screen.

## 5. Architecture — cards as text lines

Keep the existing `Paragraph::new(lines).wrap(...).scroll((offset,0))` model so
the scroll math (`visual_total`, `effective_offset`, `last_max_offset`) barely
changes. Only `build_lines` changes: instead of flat prefixed lines, each turn
emits a card drawn with box-drawing characters inline.

Per turn:

```
╭─ agent · 14:43 ──────────────────────────────╮
│ <body line 1 from render_markdown>           │
│ <body line 2 — styled spans preserved>       │
╰──────────────────────────────────────────────╯
```

- **Header line:** `╭─ {role} · {HH:MM} ` + `─`-fill + `╮`, colored by role
  (`role_color`: user=`accent_info`, agent=`accent_success`, system=
  `text_secondary`). Role label uses the existing `role_prefix` words.
- **Body:** `render_markdown(turn.content, theme, inner_width)` where
  `inner_width = card_width - 4` (two border cols + one space pad each side).
  Each returned `Line`'s spans are kept (styling/highlight intact) and wrapped
  in `│ ` … padding … ` │`. Lines longer than `inner_width` are truncated to
  `inner_width` (code lines do not overflow the card — R2).
- **Footer line:** `╰─` + fill + `╯`. A **streaming** turn
  (`finished_at.is_none()`) omits the footer and shows a `…` (or spinner glyph)
  after the header role, so an in-flight card reads as "still typing".
- Cards are separated by one blank line.

`card_width` = the history pane `area.width`. `build_lines` therefore needs the
render width; `draw_history` already has `area`, and `visual_total` already
takes `width`. Thread the width into `build_lines` (it currently ignores it).

Because the output is still a flat `Vec<Line>`, `visual_total` (wrapped-row
count) and the scroll offset model are reused unchanged in shape. Wrapping note:
since body lines are pre-truncated to `inner_width` and boxed to exact width,
the `Paragraph` `Wrap` no longer needs to soft-wrap card bodies — but the
header/footer/border lines are already full-width. Confirm `Wrap{trim:false}`
still aligns; if exact-width boxing makes wrap a no-op, that is fine.

## 6. Scroll rework

State stays (`scroll_offset`, `auto_scroll`, `last_max_offset`). Inputs added:

- **Mouse wheel:** in `src/tui/mod.rs`, when `app.tui_mode == TuiMode::Interaction`
  (and the interaction screen is present), route `MouseEventKind::ScrollUp/Down`
  to the interaction screen's `scroll_up(n)/scroll_down(n)` (n≈3 lines) instead
  of `app.panel_view`. All other modes keep the current `panel_view` routing
  (R3 — gate on mode; do not regress other screens).
- **PageUp/PageDown:** new `InteractionIntent::PageUp/PageDown` → scroll by the
  last viewport height (`last_max_offset` context / a stored viewport rows
  value). Clamp like `scroll_up/down`.
- **Jump-to-latest:** `End` → re-pin (`auto_scroll = true`, offset = max).
  (`Home` → top is optional; include if cheap.)
- **Streaming yank fix:** auto-follow the tail ONLY when `auto_scroll` is true
  (already the contract). Verify the streaming chunk-append path
  (`turn.rs` / the event that grows the streaming turn) does NOT force
  `auto_scroll = true` or reset `scroll_offset`. If it does, that is the bug —
  appending a chunk while the user is scrolled up must leave `auto_scroll`
  false (the `Append a turn` comment already promises this; streaming chunks
  must honor it too).

Keymap discipline (per /auto hard rule #6): `PageUp`, `PageDown`, `End`, `Home`
do not collide with the outer Settings/Dashboard chord set. Safe.

## 7. Decomposition — two chained issues

- **Issue A — card framing** (no dependency):
  `history.rs` rewrite: role·time header, rounded role-colored border, body via
  `render_markdown`, streaming = no footer + `…`. Fixes "scope unclear."
  Files: `src/tui/screens/interaction/history.rs` (+ `build_lines` width thread),
  minor `draw_history` call-site in `mod.rs`. Snapshots updated.

- **Issue B — scroll rework** (Blocked By A):
  Mouse routing fix (`src/tui/mod.rs`), `PageUp/PageDown/End` intents
  (`keymap.rs` + `mod.rs` handlers), streaming-yank verification.
  Files: `src/tui/mod.rs`, `src/tui/screens/interaction/keymap.rs`,
  `src/tui/screens/interaction/mod.rs`.

Sequence: A → B.

## 8. Testing

**Issue A (`history.rs` unit + snapshots):**
- `build_lines` emits a header line containing the role word and `·` time.
- Border characters (`╭ ╮ ╰ ╯ │`) present for a settled turn.
- Streaming turn: header has `…`, no `╰` footer line.
- Body markdown styled (reuse a fenced-code fixture; assert spans differ from
  plain — or assert via snapshot).
- Long code line truncated to inner width (no line wider than `card_width`).
- Snapshot tests for the interaction screen at 80x24 / 120x40 / 200x60.

**Issue B (scroll unit):**
- `classify` maps PageUp/PageDown/End to the new intents in Idle and Streaming.
- `scroll_up/down` page math clamps at 0 and `last_max_offset`.
- `End` sets `auto_scroll = true` and offset = max.
- Mouse routing: a unit/integration assertion that a ScrollUp event in
  `TuiMode::Interaction` moves the interaction offset, not `panel_view`
  (test at the routing seam; may need a small testable shim).
- Streaming-yank: append a chunk while scrolled up → `auto_scroll` stays false,
  `scroll_offset` unchanged.

**Manual-QA matrix:** each issue body carries a `## Manual Test (Human)`
section (per the `/auto` Step 4 gate — both touch `src/tui/**`).

## 9. Risks

- **R1 — body width vs card inner width.** `render_markdown` must be called
  with `inner_width = card_width - 4`; each body line boxed to exact width or
  borders misalign. Covered by snapshot tests.
- **R2 — wide highlighted code overflow.** Truncate body lines to inner width;
  do not let a long `fn ...` push the right border off-screen.
- **R3 — mouse routing regression.** Gate the new routing on
  `TuiMode::Interaction`; all other modes keep `panel_view` scroll. A test
  confirms a non-interaction mode still scrolls `panel_view`.
- **R4 — performance.** `render_markdown` runs syntect per render. The
  transcript can be long. If per-frame highlighting is too slow, cache rendered
  `Text` per settled turn (settled turns never change). Start without the cache;
  add it only if a frame-time problem shows up (YAGNI). Note as a possible
  Issue-A follow-up, not a blocker.

## 10. Working principles

1. View-layer only — no session/turn plumbing changes.
2. Reuse `render_markdown`; never reimplement markdown or re-add syntect.
3. No new dependency.
4. Card renderer survives the Phase 2-5 unification (#947-#950) and is reused
   by #950 (interaction screen as a view over the live Session).
