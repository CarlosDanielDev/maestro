Parent: #1063

maestro runs multiple providers side by side and tracks cost, but is blind to the thing that actually stops a work session: provider quota windows. Claude Code's own status shows five-hour and weekly windows (`{utilization %, resets_at}`); when a window dries up mid-wave, sessions stall. Pattern proven in the wild by akitaonrails' `ai-usagebar` (v1.38.1, 657 stars; endpoint `GET api.anthropic.com/api/oauth/usage` with Claude Code client headers, parsed into `five_hour`/`seven_day` windows — `src/anthropic/fetch.rs:21` in that repo). maestro's multi-provider pool turns this from a status widget into a routing decision: the next session goes to the provider with more headroom.

Children:

- #1064 — fetch + cache + backoff per provider.
- #1065 — the routing policy and UI.

## Done when

- [ ] both children closed by PRs with complete ledgers — owner: reviewer

## Handoff

Parents are never dispatched — the leaves carry the handoffs. Closes when children close (`wave close-parents`).

```bash
git worktree add -b epic/1063-quota-routing ../maestro-1063 origin/main
```
