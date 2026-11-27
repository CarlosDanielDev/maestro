Parent: #1051

Today the only reader of `~/.claude/projects/` is the interactive transport, and only for the session it is already in: path building at `src/agent_provider/claude/interactive_types.rs:85-95` (munge = `/` and `.` → `-`), incremental reader `tail_transcript` at `src/agent_provider/claude/interactive.rs:346`, line parser at `src/agent_provider/claude/transcript_parser.rs:36`. No module enumerates transcripts. Verified on this machine: 174 project dirs; each transcript line carries `sessionId` (uuid = file name) and `cwd` (the real path — so the repo comes from the lines, not from reversing the munged dir name); metadata lines carry `ai-title` (already parsed as metadata by `transcript_parser.rs`); recency = file mtime.

## Done when

- [ ] a library module (e.g. `src/session/discovery.rs`) exposes `scan_transcripts(home: &Path) -> Vec<DiscoveredSession>` where `DiscoveredSession` carries session id, cwd, title (from `ai-title`, fallback: first user message truncated), last-active mtime — unit-tested against a fixture tree of fake project dirs under `tests/fixtures/transcripts/` (2 projects, 3 transcripts, one with `ai-title`, one without) — owner: implementer
- [ ] cwd and session id come from the transcript lines, not from the munged directory name — `discovery_scan_roundtrips_cwd_from_lines` passes with a fixture whose munged dir name does not round-trip — owner: implementer
- [ ] a full scan of a realistic tree (100+ transcripts, use the fixture tree scaled up programmatically) completes in under 1s — `cargo test` asserts the bound with a generous margin — owner: implementer
- [ ] no writes: the module only reads (`grep -rn "write\|create" src/session/discovery.rs` matches nothing outside tests) — owner: implementer

## ADR stub

Module placement: (a) `src/session/discovery.rs` as a library function (council verdict: extract the scan so a CLI verb is cheap later); (b) private helper inside the TUI screen (rejected — buries the reuse); (c) read munged-dir names instead of line cwd (rejected — munging is lossy: `-` is ambiguous). Recommendation (to argue with): (a). Recency: file mtime, no content hashing.

## Out of scope

- Live-process detection via `ps` (mtime recency is the only liveness signal this slice emits; the warn policy lives in #1053).
- Codex/qwen/other providers' transcripts (council Q5a).
- Adopting anything — this slice only lists.

## Handoff

```bash
git worktree add -b story/1052-discovery-scan ../maestro-1052 origin/main
```

`codegraph explore "transcript_path parse_transcript_line"` and read `src/agent_provider/claude/interactive_types.rs` plus `transcript_parser.rs`. Gate: `cargo fmt --check && cargo clippy && cargo test && bash scripts/check-file-size.sh`. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes 1052`.
