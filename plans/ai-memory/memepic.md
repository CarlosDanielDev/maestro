Parent: #{{memepic.n}}

Owner request (2026-10-19): plug **ai-memory** (https://github.com/akitaonrails/ai-memory, MIT, Rust) into maestro. ai-memory keeps long-term memory for coding agents as a git-backed Markdown wiki, with a per-user service, a launcher (`ai-memory run [wrapper options] <harness> [native argv...]`) that auto-wires hooks and an MCP entry into the harness, typed claim-once handoffs between harnesses, and zero-LLM defaults. Documented harnesses: Claude Code, Codex, OpenCode (Qwen is not in its support matrix). Its workstreams are scoped by workspace, project, repo fingerprint and **worktree fingerprint**, with 90 s renewable leases (`docs/managed-workstreams.md`), which matches maestro's one-worktree-per-session model.

maestro has no cross-session memory today: each session starts cold, and what survives is `maestro-state.json` and the logs. The agent command is spawned directly from `[agents.<id>] command` (`src/config/agents.rs:48`), with maestro's own argv appended (headless stream-json args at `src/agent_provider/claude/headless.rs:28`, spawn at `:177-193`), so wrapping it is not free: the wrapper owns the process group, may print its own banner, and edits harness config on first launch. Hence a spike first.

Children:

- {{memepic.1}} — spike: run real maestro-style sessions under `ai-memory run` in an isolated sandbox, measure, decide the seam.

Candidate slices, deliberately NOT filed until the spike decides (filing them now would be guessing):
- A) opt-in per-agent launch through ai-memory (a documented shim recipe, or a native `[agents.<id>] memory` setting);
- B) memory handoff when work moves between providers (relates to the quota routing in #1063 and the fleet board in #1066);
- C) memory status in the TUI (server up? workstream? lease?);
- D) a `maestro doctor` check and docs;
- E) coexistence with maestro's own hooks and permission modes.

## Done when

- [ ] the spike #{{memepic.1.n}} is closed and its ADR names the chosen seam, and every slice drafted from that decision is filed or explicitly dropped with a reason — `gh issue list -R CarlosDanielDev/maestro --search "ai-memory" --state open` shows only what the ADR kept — owner: reviewer

## Handoff

Parents are never dispatched — the leaves carry the handoffs. This epic closes when its children close (`wave close-parents`).

```bash
git worktree add -b epic/{{memepic.n}}-ai-memory ../maestro-{{memepic.n}} origin/main
```
