Parent: #{{aimem-shim.n}}

Docs slice: write the shim recipe users copy into `maestro.toml`. The spike (`docs/spikes/ai-memory.md`) proved the seam; this slice makes it usable. `provider_for_agent` (`src/commands/run.rs:256`) takes `[agents.<id>] command` verbatim as the spawned binary, so a per-agent shim script is the whole integration:

```sh
#!/bin/sh
# ~/bin/maestro-ai-memory-claude
exec ai-memory run --no-autowire --executable "$(command -v claude)" claude "$@"
```

```toml
[agents.claude]
command = "/Users/me/bin/maestro-ai-memory-claude"
```

## Done when

- [ ] a new `docs/guides/ai-memory.md` (linked from `docs/agents.md` and `README.md` docs table if one exists) carries: the shim snippet, the maestro.toml wiring, the ai-memory server prerequisite (install + `serve`, port/auth per ai-memory's docs — never `ai-memory setup` from maestro docs), and a troubleshooting table built from the spike's verbatim errors: lease `409 … already active: owned by …`, degraded-launch warning when the server is down (launch continues, hooks spool locally), `--fresh cannot be combined with a native session…`, and the config-home mismatch error — owner: implementer
- [ ] the autowire hazard section states, with the spike reference: first launch without `--no-autowire` rewrites the harness config home (hooks + MCP entry), autowire scope follows `CLAUDE_CONFIG_DIR`-style resolution rather than `HOME`, and maestro's shim therefore always passes `--no-autowire` — owner: implementer
- [ ] drift check passes and the doc is added to `directory-tree.md` — owner: implementer

## Blocked By

- None

## Definition of Done

- [ ] every claim in the doc cites a spike scenario number
- [ ] no code changes; `cargo test` untouched and green
