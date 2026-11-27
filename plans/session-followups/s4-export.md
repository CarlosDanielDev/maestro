Parent: {{epic}}

Accepted drafts need a door to GitHub that keeps the consent gate: the setwave `define` contract — a local plan dir (`index.tsv` + one body per key), linted with the same rules `wave lint` runs, and `wave plan` as the only writer, behind `--dry-run` and an explicit OK (setwave `skills/define/SKILL.md`; precedent plan dirs: `plans/followups-1072-1073/`, `maestro-handoffs/fu20/` → #1505–#1515). This story makes maestro emit that dir from accepted drafts — a new `maestro followups export` subcommand following the existing CLI command shape (`crates/maestro-cli/src/commands/logs.rs:4`) — and setwave needs no changes.

## Done when

- [ ] an export command (CLI: `maestro followups export [--dir plans/followups-<date>]`) writes `index.tsv`, optional `deps.tsv`, and one body per accepted draft in the setwave issue-template shape (`## Done when`, `## Handoff`, `Parent:` line, `file:line` evidence) — `followups_export_writes_plan_dir` test passes — owner: implementer
- [ ] the exported dir passes the define lint (`lint_body` over every body after `fill_refs` with provisional numbers, the same block the define skill runs) — `followups_export_lints_clean` test runs the lint and passes — owner: implementer
- [ ] the export path never invokes `gh` or any network write — a grep-style test in the spirit of setwave's `tests/test_define_skill.py` fails the build if a GitHub write command appears in the export module — owner: implementer
- [ ] after export, accepted drafts are marked `exported` in state and a second export skips them — `followups_export_is_idempotent` test passes — owner: implementer
- [ ] the command prints the exact next step: `wave plan plans/followups-<date> --dry-run` — asserted in the export test output — owner: reviewer

## ADR stub

Body quality floor. Options: (a) export only drafts whose fields fill every required template section, and list the rest as "needs refinement" in the command output (recommended — a lint-red plan dir teaches the owner to distrust the exporter); (b) export everything and let `wave plan` refuse (rejected: pushes the failure to the far side of the consent gate).

## Out of scope

- Running `wave plan` (dry-run or real) from inside maestro — the owner runs it; maestro prints the command.
- Milestone assignment — chosen at `wave plan` time by the owner, not by the harvester.

## Handoff

- `git worktree add -b feat/{{s4-export.n}}-followups-export ../maestro-{{s4-export.n}} origin/main`
- `codegraph explore "followup_drafts"` and read the setwave define skill + `plans/followups-1072-1073/` for the target format.
- Gate: the `.wave.json` gate. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes {{s4-export.n}}`.
