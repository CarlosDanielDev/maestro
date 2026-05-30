# Spike: nvim diff viewer feasibility for interaction sessions

- **Issue:** #732 — `spike(tui): nvim diff viewer feasibility for interaction sessions`
- **Milestone:** v0.30.0 — Interactive Iteration Sessions (#57)
- **Spec driver:** `docs/superpowers/specs/2026-05-14-interactive-iteration-sessions-design.md` §8
- **Date:** 2026-05-29
- **Prototype harness:** `scripts/spikes/nvim-diff-prototype.sh` (throwaway, not wired into build)

---

## Executive Summary

**Verdict: 🟡 YELLOW — proceed to an implementation issue, with the caveats below.**

The hard technical risk (can the ratatui loop be suspended for an external full-screen
program and resumed cleanly?) is **already retired in-tree**: `OsShellLauncher`
(`src/tui/shell_launcher.rs`, issue #560) ships the exact suspend → run external program →
resume cycle today for `[s] Shell into worktree`. An nvim launcher is the same shape with
`Command::new("nvim")` swapped for `$SHELL`. nvim cold spawn measured at **83 ms**, far
under the 300 ms budget.

It is YELLOW, not GREEN, because the *UX shape* has real constraints that must be locked
before building, not because the mechanism is in doubt:

1. `nvim -d a b` is **two-file only**. maestro's real diffs are median **~16 files**
   (measured, see Q4) — so the default path must be a whole-worktree handoff, not `-d`.
2. The richest experience (file-tree sidebar + cycling) needs **diffview.nvim**, a plugin
   we **cannot assume** the user has installed. The universal path is a single read-only
   `ft=diff` buffer.
3. **First-paint latency on the user's machine is unknown** — our 42 ms measurement is
   `nvim -es` script mode, which exits before drawing. A heavy user `init.lua` (plugins,
   LSP autostart) can push interactive first paint to 100–500 ms+. The 300 ms budget
   applies to *our* suspend/resume edges (met with margin); it does **not** bound the
   user's own nvim startup.

Recommendation: build the **embedded single-buffer `ft=diff` handoff** as the baseline
(universal, zero-plugin), with an **opportunistic diffview.nvim upgrade** when detected,
and a **native fallback message** when nvim is absent. Follow-up issue spec is in the last
section — to be filed **after v0.30.0 closes** per the milestone's working principle #5.

---

## Q1 — Embedded vs. external (suspend/resume the ratatui loop)

**Answer: Embedded. Proven by existing code.**

maestro already suspends and resumes the TUI for an external full-screen program. The
helpers and the working precedent:

- `src/tui/mod.rs:88-104` — `enter_tui_mode` / `leave_tui_mode` toggle
  `EnterAlternateScreen` / `LeaveAlternateScreen`, mouse capture, and bracketed paste.
- `src/tui/mod.rs:109,127` — `enable_raw_mode` / `disable_raw_mode` bracket the run loop.
- `src/tui/shell_launcher.rs:29-51` — `OsShellLauncher::open_shell_at`:

  ```rust
  let _ = crossterm::terminal::disable_raw_mode();
  let _ = crate::tui::leave_tui_mode(&mut stdout);
  let status_result = Command::new(&shell).current_dir(worktree_path).status(); // BLOCKS
  let _ = crate::tui::enter_tui_mode(&mut stdout);
  let _ = crossterm::terminal::enable_raw_mode();
  ```

  `Command::status()` blocks the calling thread until the child exits — exactly what an
  interactive `nvim` session needs. The restore runs **unconditionally** (no `?` before it)
  so a child crash cannot leave the terminal stuck in raw mode.

- Second precedent: the upgrade-restart path (`src/tui/mod.rs:181-191`) does the same
  leave/enter dance around `restart_with_same_args()`.

An nvim launcher mirrors `OsShellLauncher` exactly:

```rust
Command::new("nvim").args(["-d", file_a, file_b]).status()      // two-file
// or, whole-worktree (see Q2/Q3):
// pipe `git diff <base>` to `nvim -R -c 'set ft=diff' -`
```

**External (separate terminal / tmux pane) is NOT needed** and is rejected: it fragments
the single-window TUI promise, can't guarantee a tmux server exists, and complicates focus
return. The trait abstraction (`trait ShellLauncher`) is the model — define a
`DiffViewer` trait so tests assert the path was reached without forking real nvim
(`CapturingShellLauncher` is the template for the fake).

**Prototype-backed:** harness step 4 spawned real nvim headless: cold `--headless +qa` =
**83 ms**. The crossterm mode toggles are sub-millisecond. The suspend and resume edges are
comfortably inside 300 ms.

---

## Q2 — Diff granularity (full worktree vs. last-turn vs. both)

**Answer: Ship BOTH. Default = full worktree diff (`git diff <base>`). Toggle = last-turn diff.**

- **Full worktree diff** is the obvious default — "show me everything the agent did this
  session." Base ref is the session-start commit (see Q7), not always `main`.
- **Last-turn diff** is high-value in an *iteration* session (the whole point of v0.30.0):
  "what changed since my last message?" Capture `git rev-parse HEAD` before and after each
  turn settles to `Idle`; the per-turn diff is `git diff <head_before_turn> <head_after_turn>`
  (or `<head_before>` vs working tree if the turn didn't commit).

The interaction session already has a natural turn boundary (the spec's per-turn
`claude --resume` loop), so capturing pre/post HEAD is cheap and fits the existing state
machine. Default to full; toggle to last-turn with a key (Q5).

**Caveat:** if a turn makes no commit (uncommitted working-tree edits only), "last-turn"
must fall back to `git diff` of the working tree against the pre-turn HEAD. Document this in
the implementation issue.

---

## Q3 — Alternatives: native ratatui diff widget vs. nvim handoff

**Answer: nvim handoff for v1. Native ratatui widget is a larger, lower-fidelity build — defer.**

| Dimension | nvim handoff | Native ratatui widget (lazygit-style) |
|---|---|---|
| Build cost | Low — mirror `OsShellLauncher`, ~1 trait + 1 launcher | High — diff parser, syntax highlight, scroll, fold, side-by-side layout |
| Fidelity | High — real editor: search, fold, syntax, word-diff | Medium — we reimplement a fraction of nvim |
| Dependency | Requires `nvim` on PATH (fallback needed) | None |
| In-TUI feel | Breaks single-window (suspends to nvim) | Stays in the TUI |
| Maintenance | Low — nvim is the diff engine | Ongoing — our code owns rendering bugs |

The native widget's only real win is "never leave the TUI." That's a genuine UX value but
not worth the build cost for a spike-driven v1. Recommend nvim handoff now; revisit a
native widget only if telemetry shows users dislike the suspend, **or** as the no-nvim
fallback (a *minimal* read-only scroll of `git diff` output is far cheaper than a full
lazygit clone).

**Three concrete handoff paths** (prototype harness steps 2–3b):

- **Path A — `nvim -d a b`:** two files, true side-by-side. Use only for a single-file
  focus view. **Wrong as the default** (median diff is ~16 files).
- **Path B — `git diff <base> | nvim -R -c 'set ft=diff' -`:** whole diff in one read-only,
  syntax-highlighted buffer. **Universal, zero-plugin. This is the baseline default.**
- **Path B+ — `nvim -c 'DiffviewOpen <base>'`:** diffview.nvim — file-tree sidebar, `<tab>`
  cycling, real side-by-side. **Best UX but requires the plugin.** Detect and prefer when
  present; otherwise Path B.

---

## Q4 — Sidebar: file-tree vs. single-buffer scroll

**Answer: File navigation matters — maestro diffs are too big for a single flat scroll.**

Measured real diff sizes (recent merged PRs, `git diff` file counts):

| Merge | Files |
|---|---|
| #916 | 11 |
| #913 | 30 |
| (4 more) | 21, 18, 18, 16, 15 |

Median ~16–18 files; tail up to 30. A single flat `ft=diff` buffer at 17k+ lines (harness
measured 163 files / 17,674 lines for a 20-commit range) is navigable in nvim via search
(`/`) and folding, but a **file list is clearly better** at this size.

- If diffview.nvim is present → its file-tree sidebar is the answer for free (Path B+).
- If not → Path B single buffer, but set `foldmethod=syntax` / `ft=diff` so each file folds;
  document `]]` / `[[` (next/prev file hunk) and `/` search in the in-app hint line.

So: **prefer the file-tree (via diffview) when available, degrade to a folded single buffer.**
Do not build our own sidebar in v1.

---

## Q5 — Keymap: entry from the Interaction screen

**Answer: `Ctrl+D` for full diff, `Shift+D` (`D`) for last-turn toggle. Document conflicts.**

Constraints from the `/auto` keymap hard-rules (outer screens own `Tab`, `BackTab`, arrows,
`Enter`, `Esc`, `Ctrl+s`):

| Candidate | Verdict |
|---|---|
| `Ctrl+D` | **Recommended for "open full diff".** Mnemonic. Note: in many line editors `Ctrl+D` = EOF/delete-forward — confirm the multi-line input widget doesn't already consume it; if it does, fall to `F3`. |
| `D` (when input empty) | **Recommended for "last-turn toggle"** once a diff context is relevant. Single-letter chord, safe only when the input buffer is empty (matches the "single-letter chord" child-widget rule). |
| `F3` | Solid no-conflict fallback for the full diff if `Ctrl+D` collides with the input widget. |

**Conflict to resolve in implementation:** verify `Ctrl+D` against the multi-line input
handler in the Interaction screen (#736). If the input widget binds `Ctrl+D`, use `F3` for
full-diff and keep `D`-when-empty for last-turn. Do **not** use arrow/`Tab`/`Enter`/`Esc`/
`Ctrl+s` — owned by outer screens.

---

## Q6 — Cross-platform availability + fallback

**Answer: nvim is installable on all three targets; detect-and-degrade when missing.**

- **macOS:** confirmed locally — `nvim v0.12.2` via Homebrew (`/opt/homebrew/bin/nvim`).
- **Linux:** apt / dnf / pacman / AppImage. Standard.
- **Windows:** `winget install Neovim.Neovim`, `choco install neovim`, scoop, or MSI/zip
  from the releases page. **Caveat:** native Windows console (not WSL) has historically
  rougher full-screen-program handoff than a Unix PTY; crossterm 0.28 supports Windows but
  the suspend/resume edge should be **smoke-tested on native Windows + WSL separately**
  before claiming parity. WSL behaves like Linux.

**Fallback when nvim is absent** (harness step 1):
1. Detect with `which`/`where nvim` (or `Command::new("nvim").arg("--version")` probe) once,
   cache the result.
2. If missing → show an in-TUI message: *"Install nvim to view diffs:
   https://neovim.io/doc/install/"* and/or fall back to a minimal read-only scroll of
   `git diff` output rendered in a ratatui paragraph (cheap; not the full native widget).
3. Never hard-fail the session because nvim is missing.

---

## Q7 — Worktree-state contract (diff base)

**Answer: Base = the session-start commit, captured when the interaction session is created.**

Of the three candidate bases (`main`, session-start commit, per-turn snapshot):

- **`main` is wrong as the base** — the worktree may branch from a non-main point, and the
  user wants "what changed *this session*", not "everything since main".
- **Session-start commit is the right default base.** Capture `git rev-parse HEAD` in the
  worktree at the moment the InteractionSession is constructed (#734 scaffold is the natural
  home). Store it on the session struct, e.g. `session_start_commit: String`.
- **Per-turn snapshot** is the base for the *last-turn* toggle (Q2): capture HEAD before and
  after each turn settles to `Idle`.

**Contract to document in the implementation issue:**

| Diff view | Base | Target |
|---|---|---|
| Full session diff (default) | `session_start_commit` | working tree |
| Last-turn diff (toggle) | `head_before_turn` | `head_after_turn` (or working tree if no commit) |

**Snapshot capture points:** session-start commit at `InteractionSession::new`; per-turn HEAD
in the turn loop (#737), recorded on `Idle` settle — consistent with the spec's working
principle #6 (terminator/snapshot events fire only after the turn settles, never mid-stream).

maestro already shells `git rev-parse` / `git diff` in worktrees (`src/git.rs`,
`src/git_mock.rs` — `has_commits_ahead`, `commit_and_push`), so the trait + mock pattern for
capturing these refs already exists and should be reused, not reinvented.

---

## Prototype evidence (run log)

`bash scripts/spikes/nvim-diff-prototype.sh main~20`:

```
nvim FOUND: /opt/homebrew/bin/nvim — NVIM v0.12.2
worktree diff vs main~20: 163 files, 17674 diff lines
nvim --headless +qa startup/teardown: 83 ms
git diff | nvim -R ft=diff open+quit:   42 ms
```

**What is proven:** nvim spawn/exit cost (83 ms) << 300 ms budget; whole-worktree pipe into
a single `ft=diff` buffer works; suspend/resume mechanism ships today (`OsShellLauncher`).

**What is NOT proven headless (needs a human at a real terminal):**
1. Interactive first-paint latency with the *user's own* `init.lua` (our 42 ms is `-es`
   script mode, which exits before drawing). Heavy configs can be 100–500 ms+.
2. Keyboard raw-mode / alt-screen integrity on resume across terminals (kitty, iTerm2,
   Windows Terminal, tmux). The shell-launcher already exercises this path in production,
   which is strong but not nvim-specific evidence.
3. Native Windows console (non-WSL) handoff parity.

These three are the manual-QA matrix the follow-up implementation issue must carry.

---

## Recommended follow-up issue (file AFTER v0.30.0 closes — milestone principle #5)

> **Title:** `feat(tui): embedded nvim diff viewer for interaction sessions (follow-up to #732)`
>
> **Overview:** Add an in-TUI diff handoff to the Interaction screen. Suspend the ratatui
> loop, open the session diff in nvim, resume on exit — mirroring `OsShellLauncher` (#560).
>
> **Expected Behavior:**
> - `DiffViewer` trait + `NvimDiffViewer` production impl + capturing fake (model:
>   `src/tui/shell_launcher.rs`).
> - Default: full session diff, base = `session_start_commit`, via
>   `git diff <base> | nvim -R -c 'set ft=diff' -`.
> - Opportunistic: if diffview.nvim detected, `nvim -c 'DiffviewOpen <base>'`.
> - Toggle: last-turn diff (`head_before_turn`..`head_after_turn`).
> - Fallback: nvim absent → in-TUI "install nvim" message + read-only `git diff` scroll.
> - Keymap: `Ctrl+D` full diff (fall back to `F3` if input widget binds `Ctrl+D`),
>   `D`-when-empty last-turn toggle.
>
> **Acceptance Criteria:**
> - [ ] Suspend → nvim → resume leaves terminal raw-mode/alt-screen intact (manual matrix).
> - [ ] Full + last-turn diffs both reachable; correct base refs.
> - [ ] nvim-missing fallback never hard-fails the session.
> - [ ] Trait + fake unit-tested (path reached, base ref correct) without forking real nvim.
>
> **Files to Modify:** `src/tui/diff_viewer.rs` (new), Interaction screen handler (#736),
> InteractionSession struct (#734 — add `session_start_commit`), input_handler keymap.
>
> **Test Hints:** Reuse the `ShellLauncher`/`CapturingShellLauncher` trait+fake pattern.
> Mock git refs via the existing `Git` trait (`src/git.rs`).
>
> **## Manual Test (Human):**
> 1. Start an interaction session in a worktree with ≥5 changed files.
> 2. Press `Ctrl+D` → nvim opens the full diff. Scroll, search `/`, `:qa`.
> 3. On exit, confirm the TUI redraws correctly and keys still work (no stuck raw mode).
> 4. Time suspend→nvim-visible and nvim-exit→TUI-visible (<300 ms each, excluding the
>    user's own nvim startup).
> 5. Repeat with `D` (last-turn). Repeat with nvim uninstalled → fallback message shows.
> 6. Run on macOS, Linux/WSL, and native Windows Terminal.
>
> **## Blocked By:** #736 (Interaction screen), #734 (InteractionSession scaffold). File
> after v0.30.0 milestone closes.
>
> **Definition of Done:** trait+impl+fake landed, manual matrix executed on 3 platforms,
> fallback verified, keymap conflict resolved.

---

## Verdict recap

🟡 **YELLOW — proceed.** Mechanism is proven in-tree; spawn cost is well within budget.
Build the universal single-buffer `ft=diff` handoff with an opportunistic diffview upgrade
and a clean no-nvim fallback. The three unverified items (user-config first-paint, resume
integrity across terminals, native-Windows parity) become the implementation issue's
manual-QA matrix — they shape the build, they don't block the go decision.
