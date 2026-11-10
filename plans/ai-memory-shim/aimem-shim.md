Parent: #1084

Epic: maestro adopts ai-memory through documented per-agent shim scripts (the seam the #1084 spike ADR picked, option (a)). The agent `command` in `maestro.toml` points at a small shim that execs `ai-memory run --no-autowire --executable <real harness> <harness> "$@"`; maestro code does not change. Evidence lives in `docs/spikes/ai-memory.md`: with `--no-autowire` the wrapper's stdout is byte-identical to the harness's own stream-json, `--version` passes through for the health check, exit codes are preserved, and the post-exit chooser never fires on maestro paths (headless stdin is null, interactive passes `--executable`). Qwen stays out: ai-memory has no Qwen harness.

## Done when

- [ ] the shim recipe (one POSIX-sh snippet per agent), its flag contract (`--no-autowire` mandatory, `--executable` resolution, server URL/token setup pointing at ai-memory's own docs), and the failure modes measured by the spike (lease 409 text, degraded offline launch warning, config-home mismatch error) are documented in `docs/agents.md` or a new `docs/guides/ai-memory.md` linked from it — owner: implementer
- [ ] the autowire hazard from the spike incident is stated verbatim-grade: autowire follows the harness config home (`CLAUDE_CONFIG_DIR` and friends), not `HOME`, and rewires the harness's hooks/MCP on first launch — the shim always passes `--no-autowire` so maestro never lets a third party edit the user's harness config — owner: implementer
- [ ] a stub-based smoke test proves the shim end-to-end under `--ignored` / opt-in env (no provider cost, no host writes): maestro headless + `--version` preflight + exit-code preservation through a shim wrapping `tests/fixtures/e2e_stub_agent.sh` — owner: implementer

## Blocked By

- #1084

## Definition of Done

- [ ] docs reviewed against the spike matrix (each claim traceable to a scenario)
- [ ] smoke test green locally and opt-in only in CI
- [ ] no maestro source changes outside tests/docs
