# Interaction sessions

Chat with the agent that is working your issue — without leaving the TUI.

An interaction session keeps the conversation alive after each turn: you
send a prompt, watch the agent stream its answer into a transcript, and
follow up on the same context (same `claude` conversation, same worktree).

## When to use it (vs. one-shot)

| | One-shot session | Interaction session |
|---|---|---|
| You know exactly what you want | ✅ fire and forget | works, but overhead |
| You expect back-and-forth (review, iterate, ask why) | painful — relaunch per change | ✅ built for it |
| Output | status + activity log | live chat transcript |
| Ends | when the run terminates | when a linked PR lands, or you quit |

## Launching

Every launch dialog carries the same two checkboxes (defaults come from
`[behavior.launch]` in `maestro.toml` — see
[configuration.md](../configuration.md)):

- **Produce PR** — the session is expected to end in a PR; when a
  `/pushup` PR linked to the issue is detected, the session finishes and
  the worktree is wiped.
- **Interaction** — open the chat screen instead of the one-shot status
  view.

Surfaces (#733, #919): the single-issue launch dialog, the multi-issue
launch overlay (values apply to **all** launched sessions), and the
free-form prompt screen. Keymap is identical everywhere: `Tab`/`BackTab`
moves between the prompt, the checkboxes, and Launch; `Space` toggles the
focused checkbox; `Enter` launches from any stop.

The four combos:

| Produce PR | Interaction | Behaviour |
|---|---|---|
| on | off | today's one-shot + PR auto-detection (default) |
| on | on | chat session that finishes when the linked PR lands |
| off | on | open-ended chat; ends only when you quit |
| off | off | plain one-shot, no PR terminator |

## The Interaction screen

```
┌ claude · opus · #946 — Build interaction launch ─────────────┐
│  ╭ user · 09:01 ───────────────────────────────────────────╮ │
│  │ implement the launch prompt builder                     │ │
│  ╰──────────────────────────────────────────────────────────╯ │
│  ╭ agent · 09:01 ──────────────────────────────────────────╮ │
│  │ Reading the issue… building the prompt + appendix…      │ │
│  ╰──────────────────────────────────────────────────────────╯ │
│ [Enter] Send  [Shift+Enter] New line  [Ctrl+P] Pushup  …     │
│ ┌ Prompt ───────────────────────────────────────────────────┐ │
│ │ >                                                         │ │
│ └───────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────┘
```

A new session's **first turn is the issue work**: maestro builds the same
prompt a one-shot gets (issue title + body + acceptance criteria + the
mode/guardrails appendix). If the issue is not cached yet, maestro fetches
it first — the agent never starts blind. Re-entering a live session
injects nothing.

### Keymap

| Key | Action |
|---|---|
| `Enter` | send the prompt |
| `Shift+Enter` | newline in the composer |
| `Ctrl+L` | clear the composer |
| `Ctrl+P` | send the `/pushup` prompt (only when launched with Produce PR) |
| Mouse wheel / `PageUp` / `PageDown` | scroll the transcript |
| `End` | jump to the newest message and resume tail-follow |
| `Ctrl+W` | quit the session (confirm modal; worktree kept) |
| any key after termination | back to the Issues list |

While the agent streams, the input pane is locked and a wave indicator
shows progress. The activity log tracks the lifecycle with `[INTERACTION]`
and `[TEARDOWN]` lines (launched / turn stats / closing / teardown).

## Lifecycle

1. **Launch** — worktree + branch are created (same as a one-shot);
   `[INTERACTION] #N launched (mode: produce_pr=…, interaction=true,
   transport=…)` lands in the activity log.
2. **Turns** — each `Enter` runs one agent turn through the configured
   provider/transport (`claude --resume <session-id>` under the headless
   transport; the warm PTY child under `transport = "interactive"` — see
   the [transport guide](claude-transport.md)). Per-turn stats log as
   `#N turn K: M chunks streamed (T ms)`.
3. **PR created** — when the agent runs `/pushup` and the PR marker lands,
   the screen posts a `System` turn, wipes the worktree off-thread
   (the UI stays responsive; a "wiping worktree…" banner shows progress),
   and auto-navigates back to Issues after a short beat. If a turn is
   mid-stream, the terminator waits for it to settle.
4. **Quit** — `Ctrl+W` + confirm closes the session WITHOUT wiping the
   worktree ("kept for manual inspection" — the modal names the path).

## Troubleshooting

- **"could not bind session_id; subsequent turns will re-init context"** —
  degraded mode: the first turn never reported a conversation id, so the
  next turn starts a fresh context. Usually a provider hiccup; just
  continue, or quit and relaunch.
- **"worktree teardown failed: …"** — the worktree was kept. The `System`
  turn includes the manual cleanup command
  (`git worktree remove <path>`). Common cause: a process still holding
  the directory.
- **Pre-closed session, PR arrives later** — if you `Ctrl+W` before the PR
  marker lands, the marker is ignored for that session and teardown is
  skipped (logged as `#N session pre-closed by user; teardown skipped`).
- **Ctrl+Q vs Ctrl+W** — `Ctrl+Q` is the global quit-maestro chord;
  `Ctrl+W` closes just the interaction session.

## Related

- [Claude transport guide](claude-transport.md) — headless vs interactive
  PTY, and the 2026-06-15 subscription cutoff.
- `docs/configuration.md` — `[behavior.launch]` defaults.
- Design: `docs/superpowers/specs/2026-06-04-unified-interactive-sessions-design.md`
  (the unification track that will fold this screen into the regular
  session pipeline).
