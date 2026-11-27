Parent: #1051

Carlos runs many Claude Code sessions manually across ~10 repos (named via `--name`) and cannot see or manage them. maestro was built to orchestrate sessions but only sees the ones it launches itself. Decision (council verdict 2026-10-14, unanimous 3.5/3.5 per question): maestro **adopts** external sessions — a global TUI picker lists every Claude Code transcript on the machine, and selecting one resumes that exact conversation inside maestro's Interaction screen via `--resume`. Scope locked: Interactive chat only (OneShot deferred), global discovery, warn-but-allow on live sessions (mtime), TUI-only entry, Claude Code only. Locked technical constraint: adopted sessions run in their **original cwd** with no maestro worktree — `--resume` locates the transcript by cwd (`src/agent_provider/claude/interactive_types.rs:90`).

Children:

- #1052 — the scan: enumerate transcripts machine-wide (library function, not TUI-private).
- #1053 — the pipeline: bind the external id, resume in place, guard live sessions.
- #1054 — the surface: landing menu entry + picker + adopt action.

## Done when

- [ ] all three children closed, each by a PR whose ledger marks every Done-when item — `gh issue list -R CarlosDanielDev/maestro --state open` no longer lists any of them — owner: reviewer

## Handoff

Parents are never dispatched — the leaves carry the handoffs. This epic closes when its children close (`wave close-parents`).

```bash
git worktree add -b epic/1051-adopt-external ../maestro-1051 origin/main
```
