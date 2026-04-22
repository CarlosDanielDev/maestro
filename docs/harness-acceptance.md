# /implement Harness Acceptance Checklist

Run this checklist before any release that modifies `.claude/commands/implement.md`, `.claude/hooks/implement-gates.sh`, `.claude/hooks/parse_gatekeeper_report.py`, or `.claude/agents/subagent-gatekeeper.md`.

Total time: ~10 minutes.

---

## Prerequisites

- A scratch GitHub issue in this repo or a test repo. Ideally two: one with full DOR, one with missing Acceptance Criteria.
- `gh` authenticated to the relevant repo.
- Working tree clean.
- Baseline `cargo test` green.

---

## Acceptance runs

### 1. Happy path — full DOR, no blockers, no API endpoints

- [ ] Seed a feature issue with all required DOR sections and `## Blocked By: - None`.
- [ ] Run `/implement #<n> --orchestrator`.
- [ ] Verify Step 2 (pre-check hook) prints `gate log dir: /tmp/maestro-<n>-<ts>` and exits cleanly.
- [ ] Verify Step 4 (gatekeeper) emits a fenced `json gatekeeper` block and the parser extracts `"status": "PASS"`.
- [ ] Verify Step 5 creates branch `feat/issue-<n>-<slug>`.
- [ ] Verify architect + QA are invoked and produce a blueprint + test blueprint.
- [ ] Write the failing tests from the QA blueprint (Step 6d).
- [ ] Verify RED gate (Step 6e) fires and exits 0 — meaning `cargo test` returned non-zero because the new test fails. (If `cargo test` returned 0, the gate exits 3 with "RED GATE FAILED — cargo test passed, but implementation has not started.")
- [ ] Write the minimum implementation (Step 6f).
- [ ] Verify GREEN gate (Step 6g) fires and passes after implementation — `cargo test` returns 0, gate exits 0.
- [ ] Verify security-analyst + docs-analyst are invoked at the end.
- [ ] Verify Step 7 prints the handoff with log paths.

### 2. DOR auto-remediation — missing Acceptance Criteria

- [ ] Seed an issue missing `## Acceptance Criteria`.
- [ ] Run `/implement #<n>`.
- [ ] Verify Step 4 aborts with exit 5.
- [ ] Verify a comment was posted on the issue listing the missing sections.
- [ ] Verify the `needs-info` label was applied.

### 3. Blocker enforcement — open blocker

- [ ] Seed an issue with `## Blocked By: - #<blocker>` where `<blocker>` is an OPEN issue.
- [ ] Run `/implement #<n>`.
- [ ] Verify Step 4 aborts with exit 5 and lists the open blocker in stderr.

### 4. Closed issue — hard stop

- [ ] Pick a CLOSED issue.
- [ ] Run `/implement #<n>`.
- [ ] Verify Step 2 (hook) aborts with exit 1 and "Issue #<n> is CLOSED".

### 5. Dirty tree — stash flow

- [ ] Create an uncommitted change.
- [ ] Run `/implement #<n>`.
- [ ] At the prompt, type `S`.
- [ ] Verify the hook stashes the change with message "auto-stash before /implement #<n>".
- [ ] Verify `git stash list` shows the entry.

### 6. Baseline-green enforcement

- [ ] Introduce a failing test on `main` (e.g., temporarily break one test).
- [ ] Run `/implement #<n>`.
- [ ] Verify Step 2 aborts with exit 2 "BASELINE NOT GREEN".
- [ ] Restore the baseline test.

### 7. Idempotency — Continue on existing branch

- [ ] From the completed run of #1 above, do NOT delete the branch.
- [ ] Run `/implement #<n>` again.
- [ ] Verify the (C)/(R)/(A) prompt appears with the last 5 commits shown.
- [ ] Choose `C`.
- [ ] Verify the architect/QA receive the resumption context and return minimal responses.

### 8. Idempotency — Restart flow

- [ ] Re-run `/implement #<n>` on the same branch.
- [ ] Choose `R`.
- [ ] At the typed-confirmation prompt, type `RESTART`.
- [ ] Verify the branch is deleted and a fresh branch is created.

### 9. RED gate — skipped for `docs` task type

- [ ] Seed a docs-only issue with `type:docs` label.
- [ ] Run `/implement #<n>`.
- [ ] Verify Step 4 classifies as `task_type: docs`.
- [ ] Verify Step 6e (RED gate) is skipped.
- [ ] Verify Step 6g (GREEN gate) is skipped.

### 10. GREEN gate — fires on implementation failure

- [ ] Deliberately write implementation code that breaks a test.
- [ ] Verify Step 6g aborts with exit 4.

---

## Regression notes

Record any deviations observed during the checklist. File as GitHub issues tagged `bug` + `area:harness` if they block a release; `enhancement` otherwise.
