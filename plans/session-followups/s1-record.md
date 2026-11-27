Parent: {{epic}}

A session's ending is barely readable after settle: the settle path runs through `check_completions` (`crates/maestro-tui/src/tui/app/completion_pipeline.rs:13`), and while session records DO persist in `maestro-state.json`, the 2026-11-27 hands-on measured them incomplete and unread — `worktree_branch: None` after teardown, `gates_pending: True` stuck on finished sessions, the completion cost recorded as $0.00 with the real value one log line above, and the Dashboard rendering `SESSIONS 0` with 3 records present. `.maestro/logs/<uuid>.log` keeps a compact activity trace with no structured outcome (`crates/maestro-core/src/session/logger.rs:20`). The harvester ({{s2-harvester}}) and any external observer need one append-only line per settled session with the settled truth. Related: #1370 wants a run-level ledger for external e2e observation — same pattern, different granularity; this record is session-level and is the harvester's input.

## Done when

- [ ] settling a session (Completed, Errored, Killed, Stalled) appends one jsonl line with `session_id`, `issue_numbers`, `status`, `settled_from`, `cost_usd`, `token_usage`, `files_touched`, `last_message`, `finished_at` — `session_end_record_appended_on_settle` test passes — owner: implementer
- [ ] the record survives worktree/branch teardown — `session_end_record_survives_teardown` test passes — owner: implementer
- [ ] a session killed before spawn appends no record — `no_record_for_never_spawned_session` test passes — owner: implementer
- [ ] the ledger path is injectable (no test writes into the repo checkout) — `session_end_record_path_is_injectable` test passes and `git status` is clean after `cargo test --workspace` — owner: implementer

## ADR stub

Where the record lives. Options: (a) append-only `.maestro/session-log.jsonl`, same pattern #1370 chose for the run ledger (recommended — state file churns and is rewritten whole; an append-only ledger cannot lose earlier lines to a bad write); (b) a `history` array inside `maestro-state.json` (rejected: the state file is rewritten per save and grows unboundedly — it already carries a 196 KB issue_cache).

## Out of scope

- The run-level ledger of #1370 (plan → launch → gates → PR → merge stages) — separate issue, separate consumer.
- Reading provider transcripts (~/.claude/projects/...) — the record carries what maestro already has in memory.

## Handoff

- `git worktree add -b feat/{{s1-record.n}}-session-end-record ../maestro-{{s1-record.n}} origin/main`
- `codegraph explore "check_completions settle_queue_item SessionLogger"` and read `crates/maestro-tui/src/tui/app/completion_pipeline.rs` + `crates/maestro-core/src/session/logger.rs`.
- Gate: the `.wave.json` gate. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes {{s1-record.n}}`.
