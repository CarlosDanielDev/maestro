Parent: -

Measured on 2026-11-27 against nightly `30f3b1d8` in the isolated lab `CarlosDanielDev/maestro-qa-todo` (full evidence: `plans/session-followups/STUDY.md` §6–7): the single-issue loop works (3/3 PRs, gates green, $0.07), but the multi-issue engine is dead on arrival and the artifacts lie. A council review (shipping triad, unanimous) split the findings by the rule "the code promised X and does not-X → patch; never promised X → minor". This epic is the patch train: five promises the code already makes and breaks.

Measured failures this train repairs:

1. `team launch` succeeds 0/3 — dispatch spawns `claude --print --output-format text` (argv captured by a wrapper binary) while `drive_provider` awaits `StreamEvent::AssistantMessage`, which only the stream-json path emits (`crates/maestro-core/src/agent_provider/claude/headless.rs:70` vs `crates/maestro-core/src/orchestration/dispatch.rs:296`).
2. The same failed run left 76 uncommitted lines in the lab's main checkout while reporting "0 succeeded" — side effects without record.
3. PR #10 on the lab reports `Cost: $0.00`; the real cost was $0.03. `total_cost_usd` in state measured going DOWN between runs (0.0737 → 0.0245).
4. The TUI Dashboard showed "SESSIONS 0 / PR PIPELINE 0" with 4 open maestro PRs on the repo, one created by maestro itself 20 minutes earlier.
5. A session that finished while its issue already had an open PR left the new commit stranded locally (`fa0f791`), with no push, no comment, no summary line.

## Fatia

- {{p1}} — team dispatch provider transport mismatch (+ the real-argv contract test the council unanimously demanded)
- {{p2}} — failed subagents dirty the main checkout (data-loss class; blocked by {{p1}}: same files)
- {{p3}} — cost accounting: PR body $0.00, resetting total
- {{p4}} — Dashboard blind to persisted sessions and open PRs
- {{p5}} — PR-exists guard: push to the existing branch or say so, never strand silently

## Done when

- [ ] all five children closed by merged PRs, each patch release-note ready — owner: implementer

## ADR stub

Ship as 0.34.1 patches vs fold into the next minor. Council rule applied: every item here is a broken existing promise (patch-class), so they ship as soon as each is green instead of waiting for the v0.41.0 engine work. Rejected: bundling them into the minor — users keep losing work and trusting false numbers meanwhile.

## Pendências (bloqueiam o Ready)

- none — every child carries its own reproduction and evidence.

## Out of scope

- Rebuilding the team engine on the single-run rails (rich prompt, readiness, worktree policy) — that is v0.41.0 {{e6}} territory; p1/p2 only stop the bleeding.
- The judge, DOR gate, ledger and guarantees registry — minor-class by the council rule.

## Handoff

- `git worktree add -b fix/{{epic.n}}-engine-repair-0341 ../maestro-{{epic.n}} origin/main` — only if executed as a leaf; parents never receive PRs.
- Release: after all five merge, `0.34.1` per the repo's release flow.
