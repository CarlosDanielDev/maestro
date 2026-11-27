Parent: {{epic}}

Drafts without a review surface either get auto-filed (forbidden: consent doctrine) or rot in state. The Run summary is where the owner already looks at a session's ending — today still split between `CompletionSummaryData` (`crates/maestro-tui/src/tui/app/types.rs:588`) and `SessionSummaryState` (`crates/maestro-tui/src/tui/app/types.rs:655`) — #1496 is merging them into one Run summary, and this story adds the drafts list to that surface (related: #1496; if it lands first, build on it, otherwise build on today's CompletionSummary).

## Done when

- [ ] a session with `followup_drafts` shows a drafts section in the run summary: title, evidence line, and accept/reject per draft — `run_summary_lists_followup_drafts` snapshot test (80x24, through `handle_key` + `ui::draw`) passes — owner: implementer
- [ ] accepting a draft marks it `accepted` in state; rejecting removes it — `followup_draft_accept_reject_persist` test passes — owner: implementer
- [ ] every shown key acts on the shown draft (poka-yoke) — `keymap_is_poka_yoke_clean` and `help_never_gives_one_key_two_actions` stay green — owner: implementer
- [ ] no drafts, no section — the summary renders exactly as today when `followup_drafts` is empty, with no diff in existing snapshots — owner: reviewer

## ADR stub

Where the review lives. Options: (a) a section of the Run summary (recommended — the owner is already there at session end; no new screen); (b) a dedicated Follow-ups screen (rejected for now: a new mode + registry entry + Help surface for a list that is usually empty).

## Out of scope

- Editing a draft in the TUI — accept/reject only; editing happens on the exported files.
- Notifications/badge outside the run summary — the drafts wait; nothing is urgent about a draft.

## Handoff

- `git worktree add -b feat/{{s3-review.n}}-followup-review ../maestro-{{s3-review.n}} origin/main`
- Read `docs/tui-design-system.md` and `docs/design-system/index.html` (Grep, never Read whole) before touching `crates/maestro-tui/src/tui/`; reuse `ds::` widgets; snapshot rules from the wave addendum apply.
- Gate: the `.wave.json` gate. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes {{s3-review.n}}`.
