Parent: {{epic}}

Found by the 2026-11-27 session-followups study: `maestro-state.json` in this repo carries 5 `pending_prs` entries with status `permanently_failed`, attempts exhausted (3/3), `last_attempt_at` 2026-04-30 and 2026-05-25 — seven months old. `pr_retry` sets `PermanentlyFailed` (`crates/maestro-tui/src/tui/app/pr_retry.rs:89`) and only ever removes *completed* entries (`pr_retry.rs:128-131`); `maestro clean` does not touch `pending_prs`. The Dashboard renders the status (`crates/maestro-tui/src/tui/screens/dashboard/draw.rs:290`), so every launch re-renders seven-month-old corpses with no way to dismiss them. Related: #1169 guards `pr_retry` against legacy entries' paths — it does not reap them.

## Reprodução

1. `python3 -c "import json; [print(p['issue_number'], p['status'], p['last_attempt_at'][:10]) for p in json.load(open('maestro-state.json'))['pending_prs']]"`
   Saída observada (a real, não a esperada):

   ```
   545 permanently_failed 2026-04-30
   543 permanently_failed 2026-04-30
   542 permanently_failed 2026-04-30
   539 permanently_failed 2026-04-30
   42 permanently_failed 2026-05-25
   ```

2. Open the TUI: the Dashboard still lists them; no key dismisses them; `maestro clean` keeps them.

## Done when

- [ ] a defined reap path removes or archives `PermanentlyFailed` entries — `clean_stale_reaps_permanently_failed_pending_prs` test passes: entries older than the stale window are removed and the command prints each removed issue number — owner: implementer
- [ ] live entries are untouched: `RetryScheduled`/`AwaitingManualRetry` and anything inside the stale window survive the same run — `clean_stale_keeps_live_pending_prs` test passes — owner: implementer
- [ ] after the reap, the Dashboard pending-PR area no longer renders the removed entries — `dashboard_omits_reaped_pending_prs` snapshot test (80x24, through `handle_key` + `ui::draw`) passes — owner: implementer
- [ ] the Dashboard never shows an entry the state no longer holds — existing dashboard snapshot suite has no diff for the live cases — owner: reviewer

## ADR stub

Reap semantics. Options: (a) `maestro clean --stale` also reaps `PermanentlyFailed` pending_prs past the stale window, printing what it removed (recommended — same verb that already lists kept branches, #1483); (b) a `dismiss` key on the Dashboard row (rejected as the only path: needs the TUI open; the state file should be cleanable headless). Archiving instead of deleting is a one-line change if review prefers a tombstone.

## Out of scope

- Auto-retrying permanently-failed PRs — `pr_retry.rs:170` already has manual retry; this bug is about reaping, not reviving.
- The 5 existing entries in this checkout — they are the repro; the fix's first real run removes them.

## Handoff

- `git worktree add -b fix/{{bug-pendingprs.n}}-reap-permanently-failed-prs ../maestro-{{bug-pendingprs.n}} origin/main`
- `codegraph explore "PendingPrStatus PermanentlyFailed pending_prs"` and read `crates/maestro-tui/src/tui/app/pr_retry.rs` + the `maestro clean` command.
- Gate: the `.wave.json` gate. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes {{bug-pendingprs.n}}`.
