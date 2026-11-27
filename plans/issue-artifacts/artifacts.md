Parent: #1524

Feasibility studied 2026-11-27 (`plans/session-followups/STUDY.md` §9). An issue's knowledge today has no home in maestro: studies, plan dirs, evidence patches (e.g. `plans/session-followups/team-residue.patch`, cited by #1520), benchmark logs and verdicts live scattered across `plans/`, handoff dirs and `/tmp` — and the local copies die with the machine (the lab's "box era" left only stale PRs). The gap was proven by hand: attaching the benchmark study to #1518 required a manual comment (`#1518` comment 7224777327) because neither GitHub's API nor maestro has an artifact concept. Maestro-side facts: `compose_prompt` (`crates/maestro-core/src/orchestration/dispatch.rs:90`) assembles three text sections with no file notion; the issue browser already renders a preview pane (`crates/maestro-tui/src/tui/screens/issue_browser/draw.rs:34`) — a ready surface; zero artifact/attachment concept in core (grep).

## Done when

- [ ] `maestro artifacts attach <issue> <file>` creates or updates ONE artifact comment on the issue (marker-delimited, edit-in-place, `<details>`-collapsed) instead of stacking comments — `artifact_attach_is_idempotent` test passes against the fake provider — owner: implementer
- [ ] the issue browser preview lists an issue's artifacts (name, size, updated-at) parsed from the marker comments — `issue_browser_lists_artifacts` snapshot test (80x24) passes — owner: implementer
- [ ] seat prompts carry artifact REFERENCES (name + URL/path), never inlined content — `prompt_carries_artifact_references_not_content` test asserts prompt size stays flat as artifact size grows — owner: implementer
- [ ] artifacts survive restart and machine change by living GitHub-side; a local `.maestro/artifacts/` cache is optional and disposable — `artifact_list_survives_fresh_state` test passes with an empty state file — owner: reviewer
- [ ] files over the comment limit are refused with the repo-committed alternative named (path under `plans/`), never silently truncated — `artifact_oversize_refused_loudly` test asserts the exact refusal copy — owner: reviewer

## ADR stub

Store: (a) GitHub artifact-comments (recommended — durable, cross-machine, zero new storage; measured lesson: local state does not travel); (b) local-only `.maestro/artifacts/` (rejected as the store, kept as cache). One fork.

## Out of scope

- Binary attachments (images, webm) — comment-hosted text artifacts first; binaries keep the existing upload flows.
- Artifact versioning/diffing — edit-in-place with the comment's own history is enough to start.
- The consumers (harvester #1535, judge #1531, export #1537) — they adopt artifacts as inputs in their own slices.

## Handoff

- `git worktree add -b feat/{{artifacts.n}}-issue-artifacts ../maestro-{{artifacts.n}} origin/main`
- `codegraph explore "compose_prompt issue_browser preview issue_cache"` and read `crates/maestro-core/src/orchestration/dispatch.rs:90` + `crates/maestro-tui/src/tui/screens/issue_browser/draw.rs:34`.
- Gate: the `.wave.json` gate. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes {{artifacts.n}}`.
