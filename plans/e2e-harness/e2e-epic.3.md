Parent: #1047

#942 is a manual-QA sandbox: launch an Interaction session, drive turns, terminate, check teardown — all by hand. The only automated coverage of the interactive transport is in-process (`src/agent_provider/claude/interactive_session_tests.rs:209` runs `PortablePtyBackend` against `testdata/claude-transcript/stub-claude.sh`), and it cannot see the binary around it. The interactive transport reads `$HOME` for transcripts (`src/agent_provider/claude/mod.rs:175`), which the existing test redirects to a temp dir — the same trick works at binary level. Blocked by #1049: the harness (sandbox layout, stub conventions, binary spawn helper) lands there first.

## Por que não é story

No product behavior changes; the gain is turning the #942 manual matrix into a test that runs on every PR.

## Done when

- [ ] an E2E test launches the built binary into an Interaction session against a PTY stub (reuse the `stub-claude.sh` pattern with `HOME=<tmp>`), injects one user turn, receives the stub reply, and asserts the reply landed in `.maestro/logs/<session-id>.log` — owner: implementer
- [ ] the termination path is asserted: stub signals end-of-turn, the session reaches its terminal state in `maestro-state.json`, and teardown removes only the session's worktree/branch — owner: implementer
- [ ] a comment on #942 states which matrix rows are now automated and which stay manual (Produce-PR flow needs `gh`) — owner: implementer

## ADR stub

PTY allocation: (a) `portable-pty` in-test (already a dependency, `src/agent_provider/claude/pty_backend.rs` — same crate the product uses); (b) shell shim (`script -q /dev/null`) — zero Rust but platform-specific and brittle to assert on; (c) skip the pty and hope the headless guard of #1048 covers interaction (no — Interaction is PTY-transport by definition). Recommendation (to argue with): (a). Session setup: drive the TUI with injected keystrokes versus a session-launching subcommand — start from whatever the #1049 harness made cheap and only add keystroke-driving if launch needs it.

## Out of scope

- Produce-PR termination flow (needs real `gh`).
- Multi-turn / long sessions, budget cutoffs mid-session.
- Driving arbitrary TUI screens by keystroke.

## Handoff

```bash
git worktree add -b chore/1050-e2e-interaction ../maestro-1050 origin/main
```

`codegraph explore "PortablePtyBackend interactive_session"` and read `src/agent_provider/claude/interactive_session_tests.rs` plus `testdata/claude-transcript/stub-claude.sh`. Gate: `cargo fmt --check && cargo clippy && cargo test && bash scripts/check-file-size.sh`. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes 1050`.
