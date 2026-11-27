Parent: #1047

No test in the repo launches the `maestro` binary itself: `grep -rn CARGO_BIN_EXE tests/ src/` prints nothing, and the integration suite's charter forbids process spawning (`src/integration_tests/mod.rs:161`, "No process spawning. Events are injected via handle_event()"). The binary path — TUI entry, state writes, gate subprocesses, log files — is covered only by manual QA (#942). Rehearsal on d1ec627 proved the full offline scenario works with a stub agent (exit 0, `"status": "completed"` in `maestro-state.json`, `ASSISTANT:` line in `.maestro/logs/`), and proved the one blocker: `--once` fails without a TTY.

Children:

- #1048 — the blocker: `--once` headless entry.
- #1049 — the harness core + first scenario.
- #1050 — the interaction-session scenario (automates #942).

## Done when

- [ ] all three children closed, each by a PR whose ledger marks every Done-when item — `gh issue list -R CarlosDanielDev/maestro --state open` no longer lists any of them — owner: reviewer

## Handoff

Parents are never dispatched — the leaves carry the handoffs. This epic closes when its children close (`wave close-parents`).

```bash
git worktree add -b epic/1047-e2e-harness ../maestro-1047 origin/main
```

