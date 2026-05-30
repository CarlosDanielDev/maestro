# Spike: in-session diff reviewer for interaction sessions

- **Issue:** #732 — `spike(tui): nvim diff viewer feasibility for interaction sessions`
- **Milestone:** v0.30.0 — Interactive Iteration Sessions (#57)
- **Spec driver:** `docs/superpowers/specs/2026-05-14-interactive-iteration-sessions-design.md` §8
- **Date:** 2026-05-29
- **Prototype harness:** `scripts/spikes/nvim-diff-prototype.sh` (throwaway, not wired into build)

---

## Executive Summary

**Verdict: 🟡 YELLOW — proceed. Build a native in-TUI diff reviewer (gitui-derived). nvim handoff is demoted to an optional power-user escape hatch.**

The spike opened by asking "can we suspend the TUI and shell into nvim?" The answer is yes —
maestro already ships that cycle (`OsShellLauncher`, #560). But once the **real scenario** is
fixed — a *live* interaction session that stays alive in-context, where the user reviews a
"finished" session diff (GitHub-PR style) and then keeps iterating — suspending out to nvim
is the **wrong default**. It yanks the user out of the live loop.

The right default is a **native, read-only diff reviewer rendered inside the TUI**, living as
an overlay on the Interaction screen (#736). Review the diff, press a key, you are back in the
chat exactly where you left it. The session never suspended.

This became affordable because of a single finding: **gitui is MIT-licensed and runs the same
stack as maestro (ratatui 0.29 + syntect 5.3).** Its diff component is a ready blueprint —
dual-scroll, line/hunk selection, diff-line theming. We adapt the pattern (with attribution),
we don't invent a lazygit clone.

YELLOW, not GREEN, because three real caveats shape the build (not the go decision):

1. **Vim-*like*, not vim.** A native read-only reviewer ships a curated motion set
   (`j/k`, `]`/`[` hunk, `g/G`, `Ctrl-d/u`, `/`, `n/N`). No text objects, macros, or `:`
   commands. For read-only review that covers ~95% of how anyone moves through a PR diff —
   but it is not nvim. The power user who wants real vim gets the `[o] open in $EDITOR` escape.
2. **syntect is a real dependency cost** (~MB binary + build time). Gate behind `fancy-regex`
   to avoid the C `onig` dep; consider shipping v1 with add/del line color only (already
   readable) and adding full syntax highlight as a fast-follow.
3. **Worktree teardown ends diff availability.** The reviewer reads the live worktree
   (#740 reclaims it after the PR terminator). Review must happen *before* teardown — which
   is exactly the natural flow (review → `Ctrl+P` open PR → terminator → teardown).

Read-only is confirmed scope: the agent makes changes, the user *reads* them. No
stage/unstage/revert from the viewer in v1 (that is gitui territory, not "review a session").

---

## How it flows with the v0.30.0 interactive sessions

This is the part the original spike missed. The reviewer must be a **companion to the live
session**, not a separate workflow. Read against #734 / #736 / #737 / #738:

**Integration model — a pushed overlay, not a new session state.**

- The Interaction screen (#736) binds to one `InteractionSession` with state
  `Idle / Streaming / Terminated` (#734, #738).
- The diff reviewer is a **modal overlay pushed on top of that screen**. It adds **no** new
  `InteractionState`. The session underneath stays `Idle` (or keeps `Streaming` in the
  background). Closing the overlay returns to the chat with history intact.
- This mirrors how #738 already treats the `Ctrl+Q` confirm modal — a transient overlay over
  a living session. The diff reviewer is the same shape, just read-only and scrollable.

**Entry keymap — no collision with #738's claimed chords.**

#738 already owns `Enter`, `Shift+Enter`, `Ctrl+P` (open PR), `Ctrl+L` (clear), `Esc` (back),
`Ctrl+Q` (quit). Free and mnemonic: **`Ctrl+D` = open diff reviewer.** Available from `Idle`
and `Streaming` (read-only — reviewing while the agent works is fine; snapshot the diff at
open time for v1, don't live-update). The #736 hint bar gains `[Ctrl+D] review diff`.

Inside the overlay the keyboard is **local** (the modal captures all input), so the vim
motion set has zero collision with the outer screen — same way nvim would own the keyboard,
except we never left the TUI.

**Diff base — the GitHub-PR diff falls out for free.**

The "finished session" diff the user wants is exactly the branch's diff against its fork
point:

```
base   = git merge-base main <InteractionSession.branch>
diff   = git diff <base>          # working tree of InteractionSession.worktree_path
```

`merge-base(main, branch)..worktree` **is** what GitHub shows as the PR diff. This needs
**zero new tracking** — #734's `InteractionSession` already carries `worktree_path` and
`branch`. No `session_start_commit` field, no per-turn snapshots required for v1.

**Lifecycle fit with the terminator (#739 / #740).**

```
Idle ──Ctrl+D──▶ [Diff Reviewer overlay] ──Esc/q──▶ Idle   (session untouched)
   review the branch diff (PR-equivalent), vim motions, read-only
   satisfied ──Ctrl+P──▶ Streaming ("/pushup") ──PR detected──▶ Terminated ──teardown (#740)
```

The reviewer is the natural "do I trust this before I open the PR?" gate, sitting right
before `Ctrl+P`. After teardown the worktree is gone, so the reviewer is a live-session tool
by definition — document that it is unavailable post-teardown rather than trying to preserve
a snapshot.

---

## Q1 — Embedded vs. external (and the bigger native-vs-nvim call)

**Answer: Native in-TUI widget for the default. Embedded nvim handoff kept as an escape hatch.**

The suspend/resume mechanism is proven (`src/tui/shell_launcher.rs:29-51`, #560):

```rust
let _ = crossterm::terminal::disable_raw_mode();
let _ = crate::tui::leave_tui_mode(&mut stdout);
let status_result = Command::new(&shell).current_dir(worktree_path).status(); // BLOCKS
let _ = crate::tui::enter_tui_mode(&mut stdout);
let _ = crossterm::terminal::enable_raw_mode();
```

So nvim *can* be embedded. But for a live, stay-resident interaction session it is the wrong
default — it suspends the very session the user is iterating in. We reuse this exact code for
the **`[o] open in $EDITOR`** escape hatch only: swap `$SHELL` for `nvim`/`$EDITOR`, same
trait shape (`trait DiffLauncher` modelled on `trait ShellLauncher`, fake modelled on
`CapturingShellLauncher`).

**Why native is the right default here** (it was not, before the "session stays alive"
constraint was known):

| | Native widget (gitui-derived) | nvim handoff |
|---|---|---|
| Stays in the live session | ✅ overlay, no suspend | ❌ suspends the TUI |
| GitHub-PR layout (file list + diff) | ✅ gitui already is this | ❌ `ft=diff` is a flat dump; needs diffview.nvim (can't assume) |
| Works for non-technical users | ✅ no nvim install/config needed | ❌ depends on user's nvim + config |
| Vim motions | ✅-ish curated read-only set | ✅ real vim |
| Build cost | Medium — adapt gitui (MIT, same stack) | Low — shell out |
| In-house UX control (theme, hint bar) | ✅ | ❌ foreign island |

---

## Q2 — Diff granularity

**Answer: v1 = full branch diff (PR-equivalent), base = `merge-base(main, branch)`. Last-turn diff = future toggle.**

The scenario is "understand a finished session like the GitHub PR view" → the full branch
diff is the default and, for v1, the only base. Keep it simple (CLAUDE.md: simplest first).

**Last-turn diff** ("what changed since my last message") is genuinely useful in an iteration
loop but needs per-turn HEAD snapshots wired into #737's turn loop (capture `git rev-parse
HEAD` on each `Idle` settle). Defer to a fast-follow toggle; do not block v1 on it.

---

## Q3 — Alternatives: native widget vs. nvim handoff

**Answer: Native gitui-derived widget. This is the flip from the original spike.**

What changed the math: **gitui is MIT** (gitui-org), **ratatui 0.29** (identical to maestro),
**syntect 5.3** for highlighting. Reusable, with attribution:

- **Dual-scroll architecture** — vertical = line-based *selection* (`selection: Selection`,
  `VerticalScroll` viewport; renders only `min = top .. top+height`); horizontal = viewport
  offset (`HorizontalScroll`). This is the scroll engine we would otherwise hand-roll.
- **`Selection` enum** — `Single(usize)` / `Multiple(start,end)` with `get_start/get_end/contains`.
- **`selected_hunk: Option<usize>`** + `find_selected_hunk(diff, line)` — hunk tracking from
  the selected line (powers `]`/`[`).
- **`DiffLineType`** (`Add` / `Delete` / `Header` / `None`) + `theme.diff_line(type, selected)`
  — maps directly onto maestro's `src/tui/theme.rs`.
- **syntect** for syntax color — MIT/Apache (passes the dep license allow-list). Cost flagged
  above; `fancy-regex` feature avoids the C dep.

The native widget's win over nvim — never leave the live session — is exactly what this
scenario needs. gitui being MIT + same stack is what makes it affordable rather than a
from-scratch lazygit clone.

---

## Q4 — Sidebar: file-tree vs. single-buffer scroll

**Answer: File-list pane — required, maestro diffs are too big for a flat scroll.**

Measured real diff sizes (recent merged PRs, file counts): 11, 30, 21, 18, 18, 16, 15 →
median ~16–18, tail to 30. A flat scroll at 17k+ lines (harness: 163 files / 17,674 lines for
a 20-commit range) is painful. gitui's file-list component is the blueprint; `Tab` /
file-list jump moves between files (the thing that felt "arrows-only" in raw gitui is just an
arrow-default keymap + missing hint bar — we fix both by shipping vim keys + a hint line by
default).

---

## Q5 — Keymap

**Answer: `Ctrl+D` to open (collision-free vs #738). Vim motions inside the overlay.**

Entry (outer Interaction screen — must avoid #738's `Enter`/`Shift+Enter`/`Ctrl+P`/`Ctrl+L`/
`Esc`/`Ctrl+Q`):

| Key | Action |
|---|---|
| `Ctrl+D` | Open the diff reviewer overlay (from `Idle` or `Streaming`). |

Inside the overlay (local keyboard — no outer collision), shipped **by default** + shown in a
persistent hint bar:

| Action | Key |
|---|---|
| line up/down | `j` / `k` (+ arrows) |
| half-page | `Ctrl+d` / `Ctrl+u` |
| next/prev hunk | `]` / `[` |
| top/bottom | `g` / `G` |
| search / next / prev | `/` · `n` · `N` |
| jump file | `Tab` / file-list focus |
| open in real editor | `o` (escape hatch → nvim/`$EDITOR`) |
| close → back to chat | `Esc` or `q` |

This is the discoverability fix for the gitui pain: hunk-jump and file-jump are first-class
default keys, not config-gated, with the hint bar always visible.

---

## Q6 — Cross-platform + fallback

**Answer: Native widget removes the hard nvim dependency from the primary path.**

- Native reviewer = pure Rust (ratatui + syntect) → identical on macOS / Linux / Windows /
  WSL. No nvim install required for the default experience. This is a strict cross-platform
  improvement over the nvim-default the spike opened with.
- The `[o] open in $EDITOR` escape hatch is the only nvim-dependent path, and it degrades
  cleanly: if `$EDITOR` / nvim is absent, grey the `o` key and keep the native reviewer.
- nvim availability for the escape hatch: Homebrew (macOS, confirmed `v0.12.2` locally),
  apt/dnf/pacman (Linux), winget/choco/scoop/MSI (Windows). WSL behaves like Linux.

---

## Q7 — Worktree-state contract (diff base)

**Answer: base = `git merge-base main <branch>`; target = the worktree. No new state needed.**

| Diff view | Base | Target |
|---|---|---|
| Full session diff (v1 default) | `merge-base(main, InteractionSession.branch)` | worktree (`InteractionSession.worktree_path`) |
| Last-turn diff (future toggle) | `head_before_turn` | `head_after_turn` (or worktree if no commit) |

This equals the GitHub PR diff and reuses #734's existing `branch` + `worktree_path` fields —
**no `session_start_commit`, no per-turn snapshots for v1.** maestro already shells
`git` in worktrees through the `Git` trait (`src/git.rs`, `src/git_mock.rs`); reuse it for
`merge-base` + `diff`, do not reinvent. Availability ends at worktree teardown (#740) — a
live-session tool by design.

---

## Prototype evidence (run log)

`bash scripts/spikes/nvim-diff-prototype.sh main~20`:

```
nvim FOUND: /opt/homebrew/bin/nvim — NVIM v0.12.2
worktree diff vs main~20: 163 files, 17674 diff lines
nvim --headless +qa startup/teardown: 83 ms
git diff | nvim -R ft=diff open+quit:   42 ms
```

These numbers validated the *nvim escape-hatch* path (spawn cost well under 300 ms). The
**native widget** has no spawn cost at all — it renders in the existing ratatui frame. The
prototype's value now is confirming the escape hatch is cheap, and that `git diff` against a
worktree is trivially pipeable.

**Not provable headless (manual-QA matrix for the implementation issue):**
1. Native widget readability + scroll feel on a real 16-file diff across terminals.
2. `[o]` escape-hatch suspend/resume integrity (kitty, iTerm2, Windows Terminal, tmux) — the
   shell launcher already exercises this in production, strong but not nvim-specific evidence.
3. syntect highlight performance on a large (17k-line) diff.

---

## Follow-up issue — filed as #918 (Level 7, folded into v0.30.0)

> **Resolved:** filed as **#918** and folded into milestone v0.30.0 as **Level 7**
> (`Blocked By` #736 + #738) — not a post-milestone fast-follow. Milestone working
> principle #5 was amended to record the override. The DOR spec below is what #918 carries.

> **Title:** `feat(tui): native in-session diff reviewer (gitui-derived) for interaction sessions (follow-up to #732)`
>
> **Overview:** Add a read-only, in-TUI diff reviewer as an overlay on the Interaction screen
> so users review a finished session's changes (GitHub-PR style) with vim motions, without
> leaving the live session. Adapts gitui's diff component (MIT, ratatui 0.29 + syntect).
>
> **Expected Behavior:**
> - `Ctrl+D` on the Interaction screen opens a modal diff-reviewer overlay; the underlying
>   `InteractionSession` state is unchanged (no new `InteractionState`).
> - Diff = `git diff $(git merge-base main <branch>)` over `worktree_path` (PR-equivalent).
> - Layout: file-list pane + unified diff pane + persistent hint bar. Add/Delete/Header
>   colored via `theme.rs`; syntect syntax highlight behind `fancy-regex` (may ship v1
>   without full syntax color).
> - Read-only vim motions: `j/k`, `Ctrl+d/u`, `]`/`[` (hunk), `g/G`, `/` `n` `N`, `Tab`
>   (file jump), `o` (open in `$EDITOR` escape hatch), `Esc`/`q` (close → back to chat).
> - No staging/reverting/editing. Strictly review.
> - Unavailable after worktree teardown (#740); grey the entry key when no worktree.
>
> **Acceptance Criteria:**
> - [ ] `Ctrl+D` opens the overlay from `Idle` and `Streaming`; session state untouched.
> - [ ] Diff base is `merge-base(main, branch)`; matches the eventual PR diff file set.
> - [ ] File-list jump + hunk-jump + search work via the default keymap; hint bar visible.
> - [ ] `Esc`/`q` returns to chat with history intact.
> - [ ] `o` opens `$EDITOR`/nvim via the `DiffLauncher` trait; greyed when absent.
> - [ ] Trait + capturing fake unit-tested (path reached, base ref correct) without forking real nvim.
> - [ ] Snapshot tests: empty diff, single-file, 16-file diff with file-list, scrolled state.
> - [ ] `cargo test --quiet` green; `cargo clippy -- -D warnings -A dead_code` clean.
>
> **Files to Modify:** `src/tui/screens/interaction/diff_review.rs` (NEW overlay + widget),
> `src/tui/screens/interaction/mod.rs` (dispatch `Ctrl+D`, extends #738), `src/tui/theme.rs`
> (diff line styles if missing), `src/git.rs` (add `merge_base` + `diff_text` to the `Git`
> trait), `Cargo.toml` (syntect, `fancy-regex` feature). Carry the gitui MIT copyright notice
> for any adapted code.
>
> **Test Hints:** Reuse the `Git` trait + mock (`src/git_mock.rs`) for `merge_base`/`diff`.
> Reuse the `ShellLauncher`/`CapturingShellLauncher` pattern for the `DiffLauncher` escape
> hatch. ratatui `insta` snapshots for the overlay states.
>
> **## Manual Test (Human):**
> 1. Start an interaction session in a worktree with ≥10 changed files.
> 2. `Ctrl+D` → reviewer opens, file list shows all changed files, diff pane shows the first.
> 3. `j/k` scroll, `]`/`[` jump hunks, `Tab` jump files, `/` search, `g/G` top/bottom.
> 4. `Esc` → back in chat, input + history exactly as left.
> 5. `o` → opens `$EDITOR` on the diff; quit returns to the TUI with keys intact.
> 6. Confirm the file set matches `gh pr diff` after the PR is opened.
> 7. Run on macOS, Linux/WSL, native Windows Terminal.
>
> **## Blocked By:** #736 (Interaction screen hosts the overlay), #738 (keymap dispatch).
> Folded into v0.30.0 as Level 7 (#918).
>
> **Definition of Done:** overlay + widget + `DiffLauncher` escape hatch landed; manual matrix
> executed on 3 platforms; read-only verified; keymap collision-free with #738; milestone
> dependency graph updated.

---

## Verdict recap

🟡 **YELLOW — proceed. Build native, in-TUI, read-only, gitui-derived.** It keeps the user
inside the live interaction session, matches the GitHub-PR mental model, needs no diff-base
tracking beyond what #734 already holds, and collides with nothing in #738. nvim becomes a
one-key `[o]` escape hatch for power users via the existing `OsShellLauncher` pattern. The
caveats — vim-subset, syntect weight, teardown timing — are scoping notes, not blockers.
