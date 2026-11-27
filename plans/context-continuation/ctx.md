Parent: #1524

Owner decision 2026-11-27: a long session must checkpoint at 70% context and hand off to a continuation session at 85% — today the machinery exists but the actions don't reach the work. Measured on nightly: `context_pct` is parsed per stream frame and checked on every event; at the commit threshold the "checkpoint" is one Warn line in the TUI activity log (`crates/maestro-tui/src/tui/app/context_overflow.rs:19-33`) that an unattended seat never sees — no commit happens, so a session dying at the wall still loses uncommitted work; the autofork at the overflow threshold does spawn a continuation child with a continuation prompt and optional handoff compression (`crates/maestro-core/src/session/fork.rs:16-18`), but the defaults fire at 50%/70% (`crates/maestro-core/src/config/sessions.rs:325-330`) and the handoff carries progress notes, not the settled session record. The `dead_code` header on `crates/maestro-core/src/session/context_monitor.rs:1` is stale — the monitor is wired; what is missing is actions with teeth.

## Done when

- [ ] crossing the checkpoint threshold (default 70%) makes maestro itself commit and push the seat's worktree as `checkpoint: context <pct>%` on the issue branch — `checkpoint_commits_seat_worktree` test passes with a stub session crossing the threshold — owner: implementer
- [ ] crossing the continuation threshold (default 85%) forks with a continuation prompt that carries the structured handoff: issue, branch, worktree path, progress and the latest session-end record when #1534 has landed — `continuation_prompt_carries_handoff` test asserts the prompt contents — owner: implementer
- [ ] defaults change to checkpoint 70 / continuation 85 (config still overrides both, `max_fork_depth` unchanged) — `context_thresholds_default_70_85` test passes — owner: implementer
- [ ] both actions are never silent: activity log and run summary state exactly what was checkpointed or forked, naming branch and child session — `context_actions_report_themselves` test asserts the log lines — owner: implementer
- [ ] lab proof: with `commit_prompt_pct = 5` and `overflow_threshold_pct = 10` in the lab's maestro.toml, `maestro run --once --issue N` prints the checkpoint commit and the fork lines, and the continuation session finishes the issue — before/after output pasted in the PR — owner: reviewer

## ADR stub

How the checkpoint reaches an unattended one-shot seat: (a) maestro commits the seat's worktree itself at the threshold (recommended — works for every seat kind, no provider cooperation, and automates the standing addendum rule "commit and push at every checkpoint"); (b) inject a directive turn telling the seat to commit (rejected as the only path: only Interactive sessions can receive mid-run turns). One fork.

## Out of scope

- Smart-zone sizing at dispatch (issue too big for the zone is born split) — that is #1525/#1530 territory.
- Provider-side compaction internals and TurboQuant compression tuning.

## Handoff

- `git worktree add -b feat/{{ctx.n}}-context-checkpoint-continuation ../maestro-{{ctx.n}} origin/main`
- `codegraph explore "check_context_overflow ContextMonitor prepare_fork continuation_prompt"` — the two files cited above are the whole seam. Remove the stale `dead_code` header on the monitor module while there.
- Gate: the `.wave.json` gate. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes {{ctx.n}}`.
