Parent: {{epic}}

Found by the 2026-11-27 session-followups study: the repo checkout carries 99 stray session logs in `.maestro/logs/` (44–575 bytes each, all dated 2026-11), written by tests. Content is synthetic (`session_spawned`, `conv-947` — the scripted-turn fixture of `crates/maestro-tui/src/tui/app/interaction_pipeline_tests.rs:70`). Cause: `App::new` hardcodes `SessionLogger::new(SessionLogger::default_dir())` (`crates/maestro-tui/src/tui/app/mod.rs:402`) and `default_dir()` is the relative path `.maestro/logs` (`crates/maestro-core/src/session/logger.rs:20-23`), so any test that builds a real `App` and drives a session writes into whatever cwd the test binary runs from — for root-package integration tests (`src/integration_tests/`), the repo checkout itself. Stray logs pollute `git status`-adjacent tooling and make real session logs hard to find.

## Reprodução

1. `rm -rf .maestro/logs && cargo test --quiet completion_pipeline`
2. `ls .maestro/logs | wc -l`
   Saída observada (a real, não a esperada): a non-zero count — new `<uuid>.log` files appear in the repo checkout (99 accumulated over ~2 weeks of test runs at audit time).

## Done when

- [ ] the session logger directory is injectable and every test that builds an `App` uses a temp dir (injected via constructor/param — no env mutation, per the Rust 2024 `set_var` rule) — `app_tests_write_no_logs_to_cwd` test passes — owner: implementer
- [ ] `cargo test --workspace --locked` from the repo root leaves `.maestro/logs` untouched — `git status --porcelain .maestro/logs` prints nothing after a full run, pasted in the PR — owner: implementer
- [ ] production behavior unchanged: a real `maestro` run still logs to `./.maestro/logs` — existing logger tests stay green — owner: reviewer

## ADR stub

Fix point. Options: (a) `App::new` takes the log dir (or a logger) as a parameter and test builders pass a TempDir (recommended — explicit, no hidden global); (b) `default_dir()` resolves against a per-run temp when `cfg(test)` (rejected: `cfg(test)` does not apply to integration tests of other crates, which is exactly the leaking case).

## Out of scope

- Deleting the 99 existing stray logs — housekeeping, one `rm` away, owner's yes.
- Log retention/rotation (`cleanup_old_logs` already exists, logger.rs:154).

## Handoff

- `git worktree add -b fix/{{bug-logs.n}}-test-log-leak ../maestro-{{bug-logs.n}} origin/main`
- `codegraph explore "SessionLogger default_dir App::new"` and read `crates/maestro-tui/src/tui/app/mod.rs:402` + `crates/maestro-core/src/session/logger.rs`.
- Gate: the `.wave.json` gate. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes {{bug-logs.n}}`.
