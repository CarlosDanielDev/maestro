Parent: -

Measured case for this epic (full evidence: `plans/session-followups/STUDY.md` §5–7, benchmark 2026-11-27, nightly `30f3b1d8` vs the setwave engine on the same machine, same tasks, same model): maestro's single-issue loop produces good code cheaply (3/3 PRs, $0.07), but everything AROUND the code is missing or dead — no DOR gate (a vague issue was dispatched, spent money, duplicated existing work), no issue lifecycle (state emptied at teardown, `team_runs: 0`), no ledger (external verify: LEDGER-MISSING 3/3 on maestro PRs), no double-dispatch protection (a commit stranded silently), no judge, and a multi-issue engine that fails 100% of dispatches while its tests stay green on canned fakes. The maestro-side mechanism is documented line by line in the slices (e.g. the one-line team prompt at `crates/maestro-core/src/orchestration/primitives/pipeline.rs:44`). The external setwave engine, same day: refused the vague issue at $0.00 in 3s naming every gap, refused double-dispatch naming the open PR, 122 tested guarantees, 7,570 logged runs. Council verdict (shipping triad, unanimous): rebuild the CONCEPT natively, in slices, in the next free minor — never a big-bang port (setwave stays a separate project).

This epic is that minor: six invariants, each one a slice, plus the two cross-cutting proofs (telemetry, judge). The 0.34.1 patch train (`plans/engine-repair-0.34.1/`) stops the bleeding first; nothing here depends on it except where a `blocked_by` says so.

## Fatia

- {{e1}} — DOR gate at dispatch: run and team refuse issues that are not READY (setwave measured: refusal costs $0.00 and 3s; maestro's ungated dispatch cost $0.02 and duplicate work)
- {{e2}} — the issue becomes the state machine: lifecycle persisted outside the process, multi-epic by type
- {{e3}} — Done-when ledger on every PR body, ticks applied at merge
- {{e4}} — double-dispatch refusal: premise checks (open PR, worktree, blocker) before any spawn
- {{e5}} — guarantees registry: every engine promise pinned to its enforcing test
- {{e6}} — team dispatch rebuilt on the single-run rails (rich prompt, worktree, readiness, blocked-by)
- {{e7}} — merge-blocking judge seat, opt-in, cost-gated (last — only after telemetry proves its price)
- {{e8}} — per-seat/per-wave cost telemetry in the Usage views (the token-proof surface)

## Done when

- [ ] every slice closed by merged PRs, and the lab benchmark of 2026-11-27 re-run end-to-end with the engine refusing what setwave refuses — owner: implementer

## ADR stub

Native rebuild vs shelling out to wave.py. Decided upstream (MEMORY.md 2026-11-10 pivot + owner 2026-11-27): native rebuild, setwave stays separate — this epic executes that decision; the fork is closed, the slices cite setwave behavior as the specification, never its code.

## Pendências (bloqueiam o Ready)

- Milestone numbering: the council said "next free minor"; v0.35–v0.40 are all taken (Input, Workflow, Session Rings, UX Spine, Roles, insight), so the next free number is v0.41.0 — owner confirms the title at plan time: Carlos.

## Out of scope

- Porting wave.py (4.9k tested lines) — the pivot decision stands.
- Multi-repo registry/fleet semantics beyond what the Fleet board already does.
- Changing the single-issue run path's prompt (measured good).

## Handoff

- `git worktree add -b feat/{{epic.n}}-setwave-grade-engine ../maestro-{{epic.n}} origin/main` — only if executed as a leaf; parents never receive PRs.
- Spec-by-benchmark: each slice names the setwave behavior it must match and the maestro failure it retires.
