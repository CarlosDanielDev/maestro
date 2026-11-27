Parent: #1051

maestro can continue a conversation only for sessions it spawned: `agent_session_id` binds once from a stream warning event (`src/session/manager.rs:490`, first-bind-wins at `src/session/types.rs:591`), follow-up turns resume via `build_followup_request` (`src/session/manager.rs:173`, `request.resume_session_id`), and the interactive transport re-attaches with `claude --resume <id>` (`src/agent_provider/claude/interactive.rs:260`). There is no path that starts a Session already bound to an external id, and `create_interaction_session` (`src/session/pool.rs:483`) always creates a worktree `maestro/issue-N` — wrong for adoption: `--resume` locates the transcript by cwd (`src/agent_provider/claude/interactive_types.rs:90`), so an adopted session **must** run in its original cwd with no worktree. Council verdict (2026-10-14, unanimous): Interactive only; warn-but-allow when the transcript was modified recently (mtime), never block.

## Reprodução

1. Start a `claude` session manually in any repo, exchange one turn, exit.
2. Try to continue it from maestro: no feature exists — the conversation is unreachable.

## Done when

- [ ] an adopt constructor exists (e.g. `SessionPool::create_adopted_session(id, cwd, title)`) producing an Interactive `Session` with `agent_session_id` pre-bound, `worktree_path = None`, session cwd = the transcript's cwd — unit test asserts the state (id set, no worktree, cwd preserved) — owner: implementer
- [ ] the first turn of an adopted session resumes the external conversation: integration test in `src/integration_tests/` (in-process, mock transport per charter) asserts the first provider request carries `resume_session_id = <external id>` and `cwd = <original cwd>` — owner: implementer
- [ ] the pre-bound id survives the first turn: the `SESSION_BOUND` warning bind path (`src/session/manager.rs:490`) does not clobber or error on an already-bound identical id (test: bind called twice with the same id is a no-op; with a different id is the existing error) — owner: implementer
- [ ] live-session guard: adopting a transcript whose mtime is newer than a threshold (default 10 minutes) surfaces a warning to the UI layer (returned flag on the adopt result); the adopt still proceeds — `adopt_live_guard_warns_on_recent_mtime` covers fresh (warn) and stale (no warn) fixtures — owner: implementer
- [ ] maestro's own state records the adoption: the session persists to `maestro-state.json` with its `agent_session_id` and cwd, so `maestro logs` and the session switcher see it — integration test asserts the persisted record — owner: implementer

## ADR stub

First-turn resume path: (a) reuse the follow-up request builder for turn 1 when `agent_session_id` is pre-bound (small conditional in the spawn path); (b) spawn a fresh conversation and only resume from turn 2 (rejected — the first turn would fork a new conversation, losing the thread). Recommendation (to argue with): (a). Worktree: none, ever, for adopted sessions — teardown of an adopted session must not delete anything outside maestro's own state/logs (the #741 teardown targets `maestro/issue-N` only; adopted sessions have no worktree to remove). Note: the session record lives in the maestro repo's `maestro-state.json` even though the conversation's cwd is another repo — accepted, maestro state is per-maestro-instance.

## Out of scope

- The picker UI (#1054).
- OneShot adoption (council Q1a: deferred; kill criterion — real headless demand reopens it).
- Forbidding adoption of live sessions (council Q3b: warn only).
- Provider sessions other than Claude Code (council Q5a).

## Handoff

```bash
git worktree add -b story/1053-adopt-pipeline ../maestro-1053 origin/main
```

`codegraph explore "create_interaction_session build_followup_request bind_agent_session_id"` and read `src/session/pool.rs`, `src/session/manager.rs`, `src/tui/screen_dispatch.rs` interaction branch. TDD: the integration test of the second Done-when item is the RED for the resume path. Gate: `cargo fmt --check && cargo clippy && cargo test && bash scripts/check-file-size.sh`. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes 1053`.
