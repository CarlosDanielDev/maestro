Parent: -

Maestro sessions end today with rich in-memory data — `last_message`, `activity_log`, `files_touched`, cost, tokens, turns (`crates/maestro-core/src/session/types.rs:330`) — and none of it is read for follow-ups at settle time: `check_completions` (`crates/maestro-tui/src/tui/app/completion_pipeline.rs:13`) runs gates and PR creation, then teardown drops the record. Measured cost of the missing loop, from the 2026-11 orchestration history: 25 follow-ups found by judges/agents between 2026-11-14 and 2026-11-24 were never filed until a full session reconciled them by hand against main (21 became #1485–#1500; 1 already fixed; 1 obsolete; 1 duplicate; 3 unconfirmable). A "side agent" that reads each session's ending and drafts follow-up issues closes that gap inside the product.

Two verified bugs found by the same 2026-11-27 study ride as extra leaves of this epic ({{bug-logs}}, {{bug-pendingprs}}). They are not Fatia slices — they are session-lifecycle hygiene the study surfaced, parented here because every issue needs a parent and no other umbrella fits.

## Fatia

- {{s1-record}} — persist a machine-readable session-end record when a session settles
- {{s2-harvester}} — a cheap one-shot agent drafts follow-up candidates from the record (opt-in, never files anything)
- {{s3-review}} — the owner reviews drafts in the Run summary (accept/reject)
- {{s4-export}} — accepted drafts export to a lint-clean setwave plan dir; `wave plan` stays the only door to GitHub

## Done when

- [ ] every sub-issue (the four Fatia stories and the two study bugs) closed by merged PRs — owner: implementer

## ADR stub

Where the harvester lives. Context: the pivot decision (MEMORY.md 2026-11-10) keeps maestro as the session/TUI owner and setwave as the GitHub execution layer. Options: (a) harvester inside maestro, output = setwave plan dir consumed by the existing `wave plan` (recommended — no setwave changes, GitHub writes stay behind `wave plan --dry-run` + explicit OK); (b) harvester as a setwave skill reading maestro state (rejected: setwave does not own session lifecycle).

## Pendências (bloqueiam o Ready)

- none — home decided: v0.41.0, sibling of #1524, standalone parent (`Parent: -`, precedent #1183).

## Out of scope

- Auto-filing issues without owner review — violates the consent doctrine (consent gates live outside repo control).
- Harvesting sessions from before this feature ships — no backfill; the 2026-11-26 reconciliation already cleared the historical backlog.
- Changing what the judge does in the setwave flow — the judge's findings are an input source, not a target.

## Handoff

- `git worktree add -b feat/{{epic.n}}-followup-harvester ../maestro-{{epic.n}} origin/main` — only if this epic is ever executed as a leaf; parents never receive PRs.
- Milestone: v0.41.0 — Setwave-grade Engine (owner decision 2026-11-27). Sibling of #1524: the engine epic makes sessions trustworthy; this epic makes their endings productive. s1's record is also the natural input for #1532 (cost telemetry) and the future judge (#1531).
