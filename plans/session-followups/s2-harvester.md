Parent: {{epic}}

With the session-end record on disk ({{s1-record}}), a cheap one-shot agent can read the ending and draft follow-up candidates. Today the equivalent work is done by hand at wave end: the 2026-11-26 session reconciled 25 unfiled items against main and filed #1485–#1500 — judgment work (4 of 25 were stale, fixed, or duplicate), so a regex extractor is not enough, but the bar is a draft for human review, not a filed issue.

## Done when

- [ ] with `[followups] harvest = true`, settling a session with a terminal record spawns one one-shot harvester run whose structured output (title, evidence `file:line`, repro sketch, suggested labels) lands in state as `followup_drafts` — `harvester_drafts_land_in_state` test passes against the scripted stub provider (same pattern as the `conv-947` fixtures in `crates/maestro-tui/src/tui/app/interaction_pipeline_tests.rs:70`) — owner: implementer
- [ ] with the config absent or false, no harvester spawns — `harvester_disabled_by_default` test passes — owner: implementer
- [ ] a harvester failure (non-zero exit, invalid JSON) never blocks or fails the completion pipeline — `harvester_failure_is_fail_open` test passes and a Warn line lands in the activity log — owner: implementer
- [ ] harvester cost is recorded on the originating session with a `harvest` marker, and a per-run cost cap stops a runaway harvester — `harvester_cost_capped` test passes — owner: implementer

## ADR stub

Which agent runs the harvest. Options: (a) the cheapest configured provider model, one-shot, no tools beyond reading the record (recommended — the record already carries the evidence; tool access invites wandering); (b) the same agent/model as the finished session (rejected: cost, and the session's own context is the thing being summarized, not a prerequisite). Second fork: where drafts persist — in `maestro-state.json` under `followup_drafts` (recommended: small, reviewed-then-cleared) vs a second jsonl (rejected: two ledgers for one feature).

## Out of scope

- Filing anything to GitHub — drafts are local state until {{s3-review}} + {{s4-export}} + `wave plan` behind an explicit OK.
- Harvesting setwave judge JSONs — the judge flow already files its follow-ups through the orchestrator; this feature covers maestro-product sessions.

## Handoff

- `git worktree add -b feat/{{s2-harvester.n}}-followup-harvester ../maestro-{{s2-harvester.n}} origin/main`
- `codegraph explore "dispatch_completion_job SessionLogger followup_drafts"` and read `crates/maestro-tui/src/tui/app/completion_pipeline.rs` + the {{s1-record}} record writer.
- Gate: the `.wave.json` gate. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes {{s2-harvester.n}}`.
