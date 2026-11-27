Parent: #1066

maestro sees only its own pool, in its own repo. The machine runs ~10 active repos with a mix of maestro-managed and external sessions (the pain that started the adopt epic #1051). The discovery scan (#1052, merged as #1056) already enumerates Claude Code transcripts machine-wide; this epic turns that data plus the pool into one cross-repo board — maestro's answer to akitaonrails' `ghpending` (batched GraphQL digest of every tracked repo, stale-first sort, own-items de-emphasized — `src/graphql.rs:15`, `src/sort.rs` in that repo), but session-centric instead of issue-centric.

Children:

- #1067 — the board: every session, every repo, one screen.
- #1068 — the actions: adopt and jump from the board.

## Done when

- [ ] both children closed by PRs with complete ledgers — owner: reviewer

## Handoff

Parents are never dispatched — the leaves carry the handoffs. Closes when children close (`wave close-parents`).

```bash
git worktree add -b epic/1066-fleet-view ../maestro-1066 origin/main
```
