Parent: #1047

`run --once` is documented as CI/scripting mode ("Exit after all sessions complete — CI/scripting mode, no dashboard return", `src/cli.rs:118`), but it crashes when stdin/stdout are not a TTY. `enable_raw_mode()` runs unconditionally at TUI entry (`src/tui/mod.rs:135`), while the `once_mode` short-circuit sits inside the event loop (`src/tui/mod.rs:896`) — the terminal is claimed before the code decides it is not needed. Rehearsed on d1ec627 against a stub agent (full offline scenario, `--prompt` path): the session state was already persisted before the crash, so everything up to TUI entry works.

## Reprodução

1. In a temp dir with a minimal `maestro.toml` (`[agents.stub] command = <stub script>`), run with piped stdio:

   ```
   maestro run --prompt "hello" --once --skip-doctor < /dev/null > run.out 2> run.err
   ```

   Saída observada (a real, não a esperada):

   ```
   exit=1
   Error: Device not configured (os error 6)
   ```

2. Same command inside a pty (`script -q /dev/null maestro run ...`): exit 0, `"status": "completed"`. The TTY is the only difference.

## Done when

- [ ] the command from the Reprodução exits 0 in a non-TTY subprocess and `maestro-state.json` shows the session `"status": "completed"` — proven by the E2E test of #1049 running it without a pty — owner: implementer
- [ ] the interactive dashboard path is unchanged: still enters raw mode + alternate screen when not `--once` — `cargo test` and the TUI snapshot suite pass untouched — owner: implementer

## ADR stub

Guard placement: (a) skip terminal init when `app.once_mode` — the loop already returns early for it (`src/tui/mod.rs:896`); the loop's `terminal.draw` calls need the same guard, so `--once` runs a draw-free loop; (b) check `io::stdout().is_terminal()` regardless of `--once` — silently goes headless when piped, which changes behavior for interactive subcommands in scripts; (c) document a pty requirement. Recommendation (to argue with): (a) — scoped to the flag that already promises CI behavior; (b) hides failures of subcommands that genuinely need the screen.

## Out of scope

- Headless rendering of TUI screens (no draw in `--once`, not a degraded draw).
- Other subcommands' TTY behavior.

## Handoff

```bash
git worktree add -b fix/1048-once-headless ../maestro-1048 origin/main
```

`codegraph explore "once_mode enable_raw_mode"` and read `src/tui/mod.rs` entry through the loop exit. Gate: `cargo fmt --check && cargo clippy && cargo test`. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes 1048`.
