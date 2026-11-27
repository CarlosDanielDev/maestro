# Story — Slim maestro-cli to command dispatch only

Parent: #1302

`src/cli.rs:1` (1,728 lines) and `src/commands/` implement the headless
surface — `run`, `status`, `init`, `doctor`, `sync-templates`. After #1305
they should live in `crates/maestro-cli`, depend only on `maestro-core`, and
contain zero terminal-UI code. The `maestro` binary becomes a thin `main`
that wires CLI parsing to either headless execution or the TUI.

## Done when

- [ ] CLI code lives under `crates/maestro-cli/src/` and the workspace builds green — owner: implementer
- [ ] zero `ratatui`/`crossterm`/tui imports in `maestro-cli` — owner: implementer
- [ ] `maestro run`, `maestro status`, `maestro init --non-interactive`, `maestro doctor`, `maestro sync-templates --check` unchanged on a fixture project — owner: implementer
- [ ] the binary's TUI entry (bare `maestro`) still opens the dashboard — owner: reviewer

## ADR stub

Bin depends on both crates vs a `tui` feature flag on the CLI crate.
Recommend the bin depending on both: feature flags on the CLI would let head
installs silently compile out the TUI, and the dependency graph stays honest
— two frontends, one core. Dissent welcome.

## Out of scope

- Changing CLI commands, flags, or output text.
- TUI crate work — #1306.
- Shell completions / man page generation (follow-up if paths change).

## Handoff

- `git worktree add -b feat/1307-cli-crate ../maestro-1307 origin/main`
- `codegraph explore "cli, commands, main"`
- Gate: `.wave.json`; `Cargo.toml` is a serial path; `Closes #1307`.
