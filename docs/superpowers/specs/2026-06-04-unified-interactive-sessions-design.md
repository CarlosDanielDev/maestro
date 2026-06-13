# Unified Interactive Sessions — Design

- **Date:** 2026-06-04
- **Status:** Approved (brainstorm) — implementation plan to follow
- **Milestone:** v0.30.0 — Interactive Iteration Sessions (#57)
- **Umbrella issue:** #935 (`unify interaction as a kept-alive extension of the session pipeline`)
- **Supersedes:** the phantom `docs/superpowers/specs/2026-05-14-interactive-iteration-sessions-design.md` referenced across the milestone but **never written**. This document is the real source of truth for interactive sessions.

## 1. Problem

The v0.30.0 milestone was decomposed bottom-up with no design doc present. The result is a chat **shell** that does not match the product vision:

- Picking an issue to "interact" spawns a **bare `claude --resume`** in the worktree with **zero context** — no issue body, no system prompt, no maestro guardrails/mode/knowledge. The agent does not know which issue it is on and asks the user basic questions ("which issue? gh needs approval?"). The provider decides what to do.
- The chat is a **separate machine** (`InteractionSession` + `InteractionState` + `interaction_turn.rs` bare spawn) parallel to the real session pipeline — duplicate prompt path, no shared telemetry/provider routing.
- The terminator (#739/#740/#741) **wipes the worktree and navigates away** the moment a `/pushup` PR is detected — which directly fights the "stay alive and keep chatting" intent.

The vision was captured late, in #934 (context injection) and #935 (unification), both sequenced at the very end of the graph. So everything built and tested so far is the empty shell.

## 2. Vision

**An interaction IS a regular `Session` that, instead of terminating after its one-shot run, stays alive for follow-up chat.**

Pick an issue → it runs the *real* maestro flow (the same thing a one-shot session runs: issue context + guardrails + `/implement`), narrated live in a chat transcript → when the flow settles (like a session ending) it transitions to a **kept-alive** state → you keep sending prompts, using the harness, discussing results with full context, on the same agent/resume.

One pipeline, one prompt builder, one provider routing, one telemetry path. "Interaction is an extension of a session, not a new thing" becomes literally true.

## 3. Locked decisions

1. **Auto-run flow, then chat.** Starting Interaction immediately runs the full flow (the normal session run), narrated live; on settle it stays open for follow-ups.
2. **PR keeps the session alive.** PR detection posts a `System` turn and marks the flow done, but does **not** wipe or navigate. Teardown happens **only on explicit quit** (Ctrl+Q).
3. **Collapse #934 + #935 into one effort; retire the separate machine.** No throwaway patch on the parallel `InteractionSession`. Go straight to the unified design, implemented in phases.

## 4. Architecture — "Session gains a kept-alive tail" (Approach 1)

An `Interactive`-mode `Session` reuses the entire one-shot pipeline (prompt, appendix, spawn/resume, provider routing, telemetry). When it would reach a terminal status, it instead transitions to a kept-alive `Interactive` status that accepts follow-up turns on the same resume. The Interaction screen becomes a **view over the live `Session`**. The parallel `InteractionSession` machine is retired.

`SessionMode::{OneShot, Interactive}` already exists (#734) — the mode concept is in place; what is missing is making Interactive sessions run the pipeline and stay alive.

### 4.1 State machine

- `SessionStatus` one-shot variants (`Queued`, `Running`, `Completed`, `GatesRunning`, `NeedsReview`, `FailedGates`, `Errored`, `NeedsPr`, …) stay **unchanged** — one-shot sessions are untouched (zero regression).
- **Add one variant:** `SessionStatus::Interactive` — the kept-alive state.
- **Add `Session.settled_from: Option<SessionStatus>`** — the one-shot outcome the session settled from (`Completed` / `FailedGates` / `NeedsPr` / `Errored`), shown in the banner so the user knows how the flow ended.
- **Add `Session.turn_state: TurnState { Idle, Streaming }`** — turn-level activity within the kept-alive state (drives the input lock). Not a top-level status.
- **Retire** `InteractionState` (Idle/Streaming/Terminated) and the `InteractionSession` struct. `Terminated` maps to the existing session-termination path; Idle/Streaming map to `turn_state`.

### 4.2 Flow execution (auto-run)

- Interactive launch builds the prompt **identically to a one-shot**: `build_issue_prompt_with_custom` (issue title + body + acceptance criteria) + `system_prompt_appendix` (mode + guardrails + knowledge appendix). The issue body is fetched/looked up at launch — the agent never sees an empty issue.
- The first turn **is the issue work** — the same spawn a one-shot uses. The agent runs the maestro flow (it has the `/implement` skill and the guardrails) and narrates each step into the transcript.
- No special "workflow-first turn" hack (#934's patch on the bare machine) — it is the normal session run, rendered in the chat view.

### 4.3 Kept-alive transition

- At the point a one-shot would reach a terminal status and the dispatcher would tear down, an Interactive session **intercepts**: `settled_from = <that status>`, `status = Interactive`, `turn_state = Idle`, **worktree preserved**.
- A **follow-up turn** = user prompt → `claude --resume <session_id>` through the **same** prompt builder / appendix / provider routing / telemetry as any turn (not the bare `interaction_turn` path). Each follow-up records call-log/cost telemetry like a normal turn.
- **Failure stays alive.** Settling from `FailedGates`/`Errored` also lands in `Interactive`, so the user can discuss and retry via a follow-up ("fix the clippy error and re-run"). Failure is not terminal for interactive sessions.

### 4.4 PR detection + teardown rework

- A `/pushup` PR marker matching the issue → post a `System` turn "PR #N created" + set `Session.pr_linked = true`. **No wipe, no navigation.** The session stays `Interactive`.
- `wipe_worktree` (#740, the safe-gated destructive primitive) moves to the **quit path only**: Ctrl+Q confirm → terminate the session → wipe worktree → navigate back.
- #741's auto-wipe-on-PR + auto-nav-on-PR are **removed**; its `System`-turn rendering and the `WorktreeTeardownPort` seam are **kept and re-pointed at the quit path**. #739's PR-monitor terminator becomes a **"pr_linked notifier"** (notify, do not terminate).

### 4.5 Screen as a view

- The Interaction screen renders the **live `Session`**'s turns + a status banner derived from `settled_from`. No separate `InteractionSession.history` — the `Session` owns the turns.
- Input pane is locked while `turn_state == Streaming` (existing behavior, now driven by `Session.turn_state`).
- Re-entry (#738) and the session switcher (#930) treat an Interactive session like any other live session — because it is one.

## 5. Non-goals (YAGNI)

- **Multi-provider follow-ups (#929).** Once follow-ups run on the unified pipeline (Phase 2), provider routing is inherited for free; #929 is not core to this spec and is gated behind Phase 2.
- **Native in-session diff viewer (#918)** and **multi-issue / free-form launch (#919)** — orthogonal polish, unblocked after unification, out of scope here.
- **Long-lived single agent process.** The locked transport is per-turn `claude --resume` (milestone Approach A). No persistent process.

## 6. Migration phases

Each phase is its own issue, its own TDD cycle, its own PR. Phases are sequential.

1. **Context.** ✅ **Implemented — #946 (PR pending).** Interactive launch builds the real issue prompt + `system_prompt_appendix`; the first turn carries issue title + body + acceptance criteria + appendix (mode + guardrails + knowledge), identical to a one-shot. Falls back to dialog text on cache miss (fetch-on-miss deferred to a follow-up issue). Resumed sessions inject nothing. New surface: `SessionPool::interaction_appendix()` in `pool.rs`; `App::lookup_issue()` + `App::build_interaction_launch_prompt()` in `data_handler.rs`; `open_interaction_session` in `screen_dispatch.rs` rewired to seed the first turn with the built prompt. *(Subsumes #934.)*
2. **Pipeline reuse.** Follow-up turns go through the normal resumed-turn path (shared prompt builder + provider routing + telemetry); retire the `interaction_turn.rs` bare-spawn loop.
3. **State unify.** Add `SessionStatus::Interactive` + `Session.settled_from` + `Session.turn_state`; transition one-shot → Interactive on settle; retire `InteractionState` and `InteractionSession`.
4. **Teardown rework.** Move `wipe_worktree` to the quit path; strip auto-wipe + auto-nav from #741; repoint #739 to a notifier.
5. **Screen-as-view + switcher.** ✅ **Implemented — #950.** `InteractionView { turns, turn_state, settled_from, pr_linked }` injected each frame by `ui.rs` via `InteractionScreen::set_view`; screen owns no transcript. `history` field + `InteractionState` enum removed; pipeline writes turns to `Session.turns`. New `Session::interactive_banner` / `session::interaction::settled_banner` / `draw_settled_banner` render the settled-from + PR status. Session switcher (#930): `switcher_target` in `input_handler.rs` routes a live Interactive session to its chat screen instead of Detail view.

## 7. Backlog impact

- **#934** → closed, folded into Phase 1 (implemented in #946).
- **#946** → Phase 1 implementation issue. Implemented; PR pending. Branch: `feat/issue-946-feat-session-phase-1-interactive-launch-`.
- **#935** → this spec (the umbrella). Re-scoped to the phase sequence above. Phase 1 complete; Phase 2 is next.
- **#739 / #740 / #741** → reworked in Phase 4. PR #940 still ships its chat-shell render + the #943 wrap fix; Phase 4 repurposes the terminator (PR-keeps-alive, teardown-on-quit). #740's `wipe_worktree` primitive is reused unchanged, just re-pointed.
- **#929 / #930** → gated behind Phase 2 / Phase 5.
- **#936** (deferred terminator firing) and **#938** (route removals through the wipe guard) → revisit under Phase 4; #936 may be obviated by the kept-alive model.
- **#941** (teardown off the UI thread) → still valid, applies to the quit-path wipe in Phase 4.

## 8. Risks / open questions

- **Settle-point interception.** The dispatcher's one-shot teardown path must branch cleanly on `SessionMode::Interactive` at every terminal status (`Completed`, `FailedGates`, `NeedsPr`, `Errored`). A missed branch would either tear down an interactive session early or leak a one-shot. Phase 3 must enumerate every terminal-status site.
- **Resume identity across the settle boundary.** Follow-up turns must resume the SAME `session_id` the one-shot used, so the agent retains the work it just did. Verify the `--resume` id is captured before settle.
- **Telemetry continuity.** Follow-up turns must emit the same call-log/cost records as one-shot turns (no separate accounting). Covered by Phase 2 reusing the normal turn path.
- **`--append-system-prompt` availability.** If the flag is unavailable, embed the appendix on the first turn. Verify during Phase 1 impl.

## 9. Testing strategy

- **One-shot regression** is the top guardrail: a `OneShot` session must behave exactly as today through every phase (it never enters the `Interactive` tail).
- **Settle → interactive transition:** assert an Interactive session, on reaching a terminal one-shot status, stays alive with `settled_from` set and accepts a follow-up turn on the same resume id.
- **Failure stays alive:** settling from `FailedGates`/`Errored` lands in `Interactive`, follow-up retry works.
- **PR keeps alive:** a PR marker posts the `System` turn + `pr_linked`, does not wipe or navigate.
- **Quit teardown:** Ctrl+Q confirm wipes the worktree (via the #740 safe-gated primitive) and navigates back.
- **Telemetry parity:** a follow-up turn emits the same records as a normal turn.
- Trait-based fakes for spawn/resume + teardown (as in #741); `insta` snapshots for the screen-as-view states.
