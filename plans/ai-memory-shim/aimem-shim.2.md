Parent: #{{aimem-shim.n}}

Test slice: keep the shim seam honest as maestro and ai-memory evolve. The spike measured the 3.4.5 binary by hand; this slice turns the load-bearing three properties into an opt-in E2E test that runs a shim script over `tests/fixtures/e2e_stub_agent.sh` through maestro's real transports. Opt-in because it needs the `ai-memory` binary on PATH; CI stays green without it.

## Done when

- [ ] an `#[ignore]`-gated test (run via `MAESTRO_AIMEM_E2E=1 cargo test --bin maestro aimem_shim -- --ignored` or the repo's opt-in convention) drives: (1) the health check path — `<shim> --version` yields the stub's version line and exit 0; (2) a headless turn — stdout byte-identical to the stub's own stream-json (`--no-autowire` in the shim), session id extracted, exit code preserved including a stub that exits 3 — owner: implementer
- [ ] the test builds its own sandbox (temp HOME, temp `CLAUDE_CONFIG_DIR`, temp data dir via `AI_MEMORY_DATA_DIR`, random free port for the server, stubs only — no real harness, no provider cost, no host writes) and fails with a clear message when `ai-memory` is absent — owner: implementer
- [ ] the test asserts the PTY-interactive invariant from the spike: with `--executable` in the shim, the post-exit chooser menu never blocks teardown (repo PTY backend, bounded timeout) — owner: implementer

## Blocked By

- None

## Definition of Done

- [ ] test green locally with the binary present, and skipped cleanly without it
- [ ] no new dependencies; sandbox cleanup verified (no files outside the temp tree)
