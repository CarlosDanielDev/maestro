Parent: #1075

Found by the judge of PR #1072 (headless `--once`). In the event loop (`src/tui/mod.rs:896` area), when `app.screen_state.home_screen.is_some() && app.pool.total_count() == 0` the loop sets `tui_mode = Dashboard` and `continue`s. The once-mode exit is excluded for `TuiMode::Dashboard` by the `!matches!(app.tui_mode, Dashboard | IssueBrowser | PromptInput | CompletionSummary)` guard, so on the next iteration the exit is unreachable. Since #1072 `--once` claims no terminal and polls no keys, a bare `maestro run --once` with nothing queued now spins on the Dashboard forever instead of exiting — a silent CI hang. Pre-existing logic, made reachable headless by #1072; `run --prompt/--issue/--milestone` enqueue before the loop so they are not affected.

## Reprodução

1. In a temp dir with a minimal `maestro.toml` and no prompt/issue/milestone, run with piped stdio: `maestro run --once --skip-doctor < /dev/null > out 2> err; echo exit=$?`
2. Wait 30s.
   Saída observada (a real, não a esperada): the process does not exit (expected: exits 0 immediately — nothing to run).

## Done when

- [ ] in once mode with an empty pool and everything done, the loop returns `Ok(())` instead of parking on the Dashboard — `once_mode_empty_pool_exits` unit test over the extracted completion-ready fn passes, with a mutation check — owner: implementer
- [ ] interactive (non-once) behavior unchanged: the Dashboard park for an empty pool still happens when `once_mode` is false — `non_once_empty_pool_still_parks_on_dashboard` test passes and the TUI snapshot suite has no diff — owner: implementer
- [ ] the reproduction above exits 0 within 5 seconds in a non-TTY subprocess — pasted before/after output in the PR — owner: implementer

## Out of scope

- A hard timeout/watchdog for sessions that never complete (separate concern).

## Handoff

```bash
git worktree add -b fix/1075-once-empty-pool ../maestro-1075 origin/main
```

`codegraph explore "completion_ready once_mode"` and read the extracted completion-exit fn in `src/tui/mod.rs`. Builds on the completion-ready extraction merged in #1072. Gate: the `.wave.json` gate. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes 1075`.
