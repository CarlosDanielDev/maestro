---
title: /implement Harness Enforcement — Design
date: 2026-04-21
status: Approved (design phase)
author: Carlos Daniel
---

# /implement Harness Enforcement — Design

## Summary

Harden the `/implement` slash command by converting its procedural prose into
enforceable gates. Today the command is a natural-language playbook the model
is trusted to follow. This spec adds three cooperating pieces — a pre-check
shell hook, a consultive `subagent-gatekeeper`, and a rewritten command body
— so that DOR verification, `Blocked By` dependency resolution, TDD RED/GREEN
phasing, and idempotency on re-run become mechanically enforced rather than
reliant on the model remembering the rules in `CLAUDE.md`.

The design explicitly excludes PR-level CI quality gates (complexity,
coverage, mutation, layering, races, leaks, N+1). Those belong to a separate
spec (`PR quality-gate CI pipeline`) which this harness bridges to via a
single optional pre-flight hook.

## Context and Problem

`.claude/commands/implement.md` today is a markdown prompt template. At
runtime, Claude Code substitutes `$ARGUMENTS` and feeds the rendered markdown
to the model as instructions. All "orchestration" is prose the model is
trusted to follow. Concrete gaps vs. `CLAUDE.md`:

1. **DOR gate is incomplete.** `CLAUDE.md §3` requires verifying Overview,
   Acceptance Criteria, Files to Modify, **Blocked By**, Definition of Done,
   etc. The command only checks API contracts at Step 4.5. The full DOR gate
   lives only in prose.
2. **No dependency-graph enforcement.** `CLAUDE.md §4` is non-negotiable
   about `Blocked By` and milestone graph updates. `/implement` never reads
   `Blocked By` and never verifies upstream issues are closed. (`/pushup`
   handles the milestone update on closure, so that side is covered.)
3. **No hard TDD enforcement.** "Write tests FIRST (RED)" is a bullet, not a
   gate. There is no "verify tests fail before you write code" checkpoint.
4. **Subagent calls are not scripted.** The file names `subagent-architect`,
   `subagent-qa`, etc., but the `Agent` tool invocation is entirely up to the
   model at runtime.
5. **Mode flag drift.** `CLAUDE.md` mentions Training mode; the command
   supports only Orchestrator and Vibe Coding as flags.
6. **No idempotency.** Running `/implement #123` twice re-fetches,
   re-branches (or collides), and re-delegates. No state awareness.

Resolution shape chosen during brainstorming:

- **D — hybrid** (hooks for mechanical checks + gatekeeper subagent for
  semantic checks).
- **B — strict abort + auto-remediation for DOR** (post comment, apply
  `needs-info` label).
- **B — mid-flow `cargo test` checkpoint** for RED/GREEN, with a
  baseline-green assertion as a prefix.
- **B — detect + prompt** for idempotency (soft — no state files).

## Non-Goals

- **CI quality-gate pipeline** (complexity, coverage, mutation, layering,
  races, leaks, N+1). Out of scope; a separate spec will cover the full
  three-tier CI rollout. This harness only bridges to it via
  `.claude/hooks/preflight.sh` (optional, skipped if absent).
- **Persistent per-issue state** (receipt files, resumable plan artifacts).
  Deliberately rejected in favor of soft idempotency.
- **Cross-repo orchestration.** Blockers can be in other repos (handled via
  `gh api`), but the implementation flow itself is single-repo.
- **Automated end-to-end tests against a real GitHub repo.** Replaced with
  a manual acceptance checklist run before harness-modifying releases.

## Architecture Overview

Three cooperating pieces, each with a clear job:

```
┌──────────────────────────────────────────────────────────────┐
│  /implement #<n> (rewritten command body, prose + bash)     │
│                                                              │
│  Step 0: parse args                                          │
│  Step 1: language/mode selection                             │
│                                                              │
│  Step 2: [GATE] .claude/hooks/implement-gates.sh            │
│          └─> cheap mechanical checks + preflight bridge      │
│                                                              │
│  Step 3: gh issue view <n>                                  │
│                                                              │
│  Step 4: [GATE] subagent-gatekeeper                         │
│          └─> structured YAML report                          │
│          └─> orchestrator performs side effects              │
│                                                              │
│  Step 5: branch / idempotency prompt                         │
│                                                              │
│  Step 6: architect → /validate-contracts → qa                │
│          └─> [GATE] RED checkpoint (cargo test)              │
│          └─> implement                                       │
│          └─> [GATE] GREEN checkpoint (cargo test)            │
│          └─> refactor → re-run GREEN                         │
│          └─> security-analyst → docs-analyst                 │
│                                                              │
│  Step 7: handoff → suggest /pushup                           │
└──────────────────────────────────────────────────────────────┘
```

**Mental model:**
- Hook = bouncer at the door (cheap, mechanical, deterministic).
- Gatekeeper = visa clerk (semantic, drafts remediation).
- Command = concierge (orchestrates, acts on results, no judgment).

Each piece has one job with a clean interface: shell exit code; subagent
structured YAML; command flow control. Layers can be tested and evolved
independently.

## Components

### 1. Pre-check shell hook: `.claude/hooks/implement-gates.sh`

New file. Executable. Takes issue number as `$1`. Performs mechanical checks
only — no LLM, no judgment, no network beyond `gh auth status` and
baseline-green assertion. Exits:

| Code | Meaning |
|------|---------|
| 0    | Continue |
| 1    | Generic failure (gh missing, not authed, not in repo, closed issue) |
| 2    | Baseline cargo test failing |
| 6    | Dirty tree, user declined stash |

Calls `.claude/hooks/preflight.sh` if that file exists and is executable.
Silent skip if absent.

### 2. Consultive subagent: `subagent-gatekeeper`

New file: `.claude/agents/subagent-gatekeeper.md`. Registered in the
subagent registry. Purely consultive per `CLAUDE.md §1` — reads via
`gh issue view` / `gh api`, never mutates. Returns a structured YAML report
the orchestrator parses.

Delegated checks: DOR presence, DOR semantic quality, `Blocked By`
resolution, API-contract presence, task-type classification.

**Frontmatter (matches existing subagent convention):**

```yaml
---
name: subagent-gatekeeper
description: DOR, Blocked By, and API-contract gatekeeper for /implement
color: yellow
model: sonnet
tools: Read, Glob, Grep, Bash
---
```

Note that `Bash` is listed — this is a deliberate departure from the other
consultive subagents (architect, qa, security-analyst, master-planner) which
do not have `Bash`. The gatekeeper needs it to resolve blockers via
`gh issue view <NNN>` and `gh api`. Tradeoff accepted because:

1. The gatekeeper's prompt explicitly restricts `Bash` usage to **read-only**
   `gh` commands: `gh issue view`, `gh api repos/.../issues/...`, `gh auth status`.
2. The gatekeeper never runs `gh issue comment`, `gh issue edit`,
   `gh issue close`, `gh api -X PATCH/POST/DELETE`, or any other mutating
   verb. The orchestrator performs all side effects.
3. The gatekeeper's description and system prompt reinforce: "You are
   consultive only. You never mutate issues. You draft remediation text for
   the orchestrator to post."

If future tooling supports tool-level verb restrictions (e.g., `Bash(gh:read-only)`),
tighten at that layer. For v1, prompt-level discipline is sufficient.

### 3. Rewritten command body: `.claude/commands/implement.md`

Becomes shorter, more literal. Every gate is either a `bash` block with an
exit-code check or a subagent invocation with structured-output parsing.
No free-form "remember to check X" prose.

### 4. Optional parser utility: `.claude/hooks/parse-gatekeeper-report.py`

Small Python (stdlib only) script that extracts the fenced YAML block from
the gatekeeper's response and converts it into a shell-consumable form
(JSON via `json.dumps`). ~30 lines. Tested independently.

### 5. Acceptance checklist: `docs/harness-acceptance.md`

Manual walk-through run before any release that modifies the harness.

## Execution Flow

### Step 0 — Argument parsing

Parse `$ARGUMENTS` for:
- Issue number (first `\d+` with optional `#` prefix).
- Language flag (`--english/-e`, `--portuguese/-pt`, `--spanish/-s`).
- Mode flag (`--orchestrator/-o`, `--vibe-coding/-vc`).

If `--training/-t` is detected:
```
Training mode is for configuring .claude/ agents, skills, and commands.
/implement is for building features from GitHub issues.
Drop --training, or use a free-form prompt for .claude/ configuration.
```
Exit 0 (not an error — just wrong tool).

If no issue number: prompt the user.

### Step 1 — Language and mode selection

Unchanged from current command. Honor flags or ask.

### Step 2 — Pre-check hook (GATE)

```bash
bash .claude/hooks/implement-gates.sh <issue-number>
```

The hook performs, in order:

1. `git rev-parse --git-dir` — not in a repo → exit 1.
2. `command -v gh` — missing → exit 1 with install hint.
3. `gh auth status` — not authed → exit 1 with auth hint.
4. **Fetch issue JSON (full, cached for Step 3):**
   ```bash
   GATE_LOG_DIR="/tmp/maestro-<n>-$(date +%s)"
   mkdir -p "$GATE_LOG_DIR"
   gh issue view <n> --json title,body,labels,assignees,milestone,state,comments \
     > "$GATE_LOG_DIR/issue.json"
   export GATE_LOG_DIR  # command reads from here
   ```
   If the fetch fails (issue not found, auth expired mid-run) → exit 1.
5. Parse state from the cached JSON. Closed → exit 1.
6. `git status --porcelain` non-empty → print status → prompt (S)tash/(A)bort.
   On S: `git stash push -m "auto-stash before /implement #<n>"`. On A: exit 6.
7. `cargo test --quiet` → baseline must be green. Non-zero → exit 2.
8. `.claude/hooks/preflight.sh` if present and executable → honor exit code.

On non-zero exit, the command aborts and prints the hook's stderr verbatim.
On success, the command reads `$GATE_LOG_DIR/issue.json` for Step 3 — no
second `gh` call.

### Step 3 — Issue already fetched

The hook has cached the issue JSON at `$GATE_LOG_DIR/issue.json`. The
command reads it directly and passes it to the gatekeeper. No additional
`gh` call. Saves one round-trip and one rate-limit hit per invocation.

### Step 4 — Gatekeeper (GATE)

Invoke `subagent-gatekeeper` with the issue JSON and the selected mode.
Expected response contains a fenced YAML block (see Gatekeeper Contract below).
Parse via `.claude/hooks/parse-gatekeeper-report.py`.

- `status: FAIL` with `dor.passed: false` → orchestrator runs:
  ```bash
  gh issue comment <n> --body "<remediation.comment_body>"
  gh issue edit <n> --add-label needs-info
  ```
  Then exit 5.
- `status: FAIL` with other reasons → print `reasons`, exit 5.
- `status: PASS` → remember `task_type`, continue.

### Step 5 — Branch selection with idempotency

```bash
existing=$(git branch --list "feat/issue-<n>-*" | head -1 | sed 's/^[ *]*//')
```

If empty → create `feat/issue-<n>-<slug>` and check out.

If non-empty → present the idempotency prompt (see Idempotency UX).

### Step 6 — Orchestrator-mode subagent sequence

Vibe mode skips 6a and 6c.

**6a. `subagent-architect`** — returns architecture blueprint.

**6b. `/validate-contracts`** — if the architect's blueprint touches API
endpoints. (Contract presence was already verified at the gatekeeper; this
step confirms field-level alignment.)

**6c. `subagent-qa`** — returns test blueprint (test cases, mocks, expected
behaviors).

**6d. Model writes tests** — from the QA blueprint.

**6e. RED checkpoint (GATE)** — skipped for `task_type in {docs, refactor}`.
See TDD section below.

**6f. Model implements** — minimum code to pass the failing test.

**6g. GREEN checkpoint (GATE)** — skipped only for `task_type == docs`.

**6h. Refactor** — model refactors if needed; GREEN re-runs afterward.

**6i. `subagent-security-analyst`** — reviews implemented code.

**6j. `subagent-docs-analyst`** — updates docs and `directory-tree.md`.

### Step 7 — Handoff

```
Implementation complete for Issue #<n>: <title>

Gates passed:
  - Pre-check hook (ok)
  - Gatekeeper (task_type: implementation)
  - RED checkpoint (verified failing → passing)
  - GREEN checkpoint (all tests pass)

Logs: /tmp/maestro-<n>-<timestamp>/

Next: run /pushup to commit, push, create PR, and close the issue.
```

## Gatekeeper Subagent Contract

### Input

- Raw `gh issue view` JSON (`title`, `body`, `labels`, `milestone`, `state`,
  `comments`).
- Selected mode (`orchestrator` or `vibe`).
- Repo name for `gh api` calls (`<owner>/<repo>`).

### Checks

#### 1. DOR section presence

Parses the issue body for required `##` headings based on type. Type
inferred from labels (`type:bug` or `type:feature`), fallback to presence of
`## Steps to Reproduce`.

| Section | Feature | Bug |
|---------|---------|-----|
| `## Overview` | required | required |
| `## Current Behavior` | — | required |
| `## Expected Behavior` | required | required |
| `## Steps to Reproduce` | — | required |
| `## Acceptance Criteria` | required | required |
| `## Files to Modify` | required | optional |
| `## Test Hints` | required | optional |
| `## Blocked By` | required | required |
| `## Definition of Done` | required | required |

Missing required sections → `dor.passed: false`.

#### 2. DOR semantic quality

- `## Acceptance Criteria` must contain ≥1 checklist item (`- [ ]` or
  `- [x]`). Free prose only → `dor.weak_sections: [Acceptance Criteria]`.
- `## Blocked By` must contain either `- None` or one-or-more `- #NNN` (or
  `- owner/repo#NNN`) lines. Free prose like "none currently" → weak.

#### 3. Blocker resolution

For each `#NNN` in `## Blocked By`:

```bash
gh issue view <NNN> --json state,title
```

Or for cross-repo:

```bash
gh issue view owner/repo#NNN --json state,title
```

Any blocker with `state != CLOSED` → `blockers.passed: false`. Self-referential
blockers (issue blocks itself) → FAIL with explicit reason.

#### 4. API contract presence

Regex issue body for endpoint hints: `GET /`, `POST /`, `PUT /`, `PATCH /`,
`DELETE /`, `/api/`. For each match, check `docs/api-contracts/` for any JSON
schema file referencing that endpoint path. Missing schema → FAIL with the
endpoint listed in `contracts.missing`.

#### 5. Task-type classification

Derived from labels (in priority order):
- `type:docs` → `docs`
- `type:refactor` → `refactor`
- `type:test` → `test-only`
- otherwise → `implementation` (default — strictest gates).

### Output format

The gatekeeper emits a **fenced code block with the `yaml gatekeeper`
language tag**. The parser extracts the content of the first matching
fence. A `report_version` field pins the schema so the parser can reject
drift.

````
```yaml gatekeeper
report_version: 1
status: PASS | FAIL
task_type: implementation | docs | refactor | test-only
dor:
  passed: true | false
  missing_sections: []
  weak_sections: []
blockers:
  passed: true | false
  open:
    - number: 42
      title: "feat: upstream scaffolding"
      state: OPEN
contracts:
  passed: true | false
  missing:
    - "POST /api/items"
remediation:
  comment_body: |
    Thanks for the issue! Before we can start, the following DOR sections
    are required:
    - `## Acceptance Criteria` (testable checklist items)
    - `## Blocked By` (issue numbers or `None`)
    See .claude/CLAUDE.md §3 for the full DOR table.
  labels_to_add:
    - needs-info
reasons:
  - "Missing required section: ## Acceptance Criteria"
  - "Blocker #42 is still OPEN"
```
````

The fenced code block is the machine-readable decision surface. Prose above
and below the fence is human-readable explanation; the parser ignores it.

**Why a fenced code block + `report_version` instead of a sentinel line:**

- Triple-backtick fencing is markdown-native and renders cleanly in the
  orchestrator's display.
- The `yaml gatekeeper` language tag uniquely identifies the fence — the
  parser grepping for ```` ```yaml gatekeeper ```` will never false-match
  against normal YAML code blocks the subagent might include in its prose.
- `report_version: 1` lets the parser reject schema drift. If a future
  version of the gatekeeper emits `report_version: 2` with new fields, v1
  parsers fail closed (abort) rather than silently dropping data.
- Avoids collision with YAML document separators (`---`) which a
  multi-document YAML emitter would produce.

### Side-effect discipline

The gatekeeper NEVER mutates. It reads (`gh issue view`, `gh api`), it
classifies, and it drafts the comment body. The orchestrator is the one
that runs `gh issue comment` and `gh issue edit --add-label`. This preserves
the `CLAUDE.md §1` "subagents are consultive only" rule.

### Edge cases

- Empty `## Blocked By` section (heading present, no content) → FAIL.
- Free prose in `## Blocked By` ("none currently") → FAIL.
- Self-referential blocker → FAIL.
- Cross-repo blocker (`- owner/repo#123`) → resolve via
  `gh issue view owner/repo#123`.
- Issue body with no label → default `task_type: implementation`.
- Multiple `## Blocked By` headings → treat as malformed, FAIL.

## TDD RED/GREEN Verification

### Baseline-green assertion (Step 2, pre-check hook)

```bash
cargo test --quiet 2>&1 > /tmp/maestro-baseline.log
baseline_exit=$?
if [ $baseline_exit -ne 0 ]; then
  echo "BASELINE NOT GREEN — existing tests are failing before /implement ran."
  echo "The RED gate would pass for the wrong reason. Fix baseline first."
  echo "See /tmp/maestro-baseline.log"
  exit 2
fi
```

Closes the one gaming hole in the RED gate: if the project already has
failing tests, the RED gate would trivially pass.

### RED checkpoint (Step 6e)

```bash
cargo test --quiet 2>&1 | tee /tmp/maestro-<n>-<ts>/red.log
red_exit=${PIPESTATUS[0]}
if [ $red_exit -eq 0 ]; then
  echo "RED GATE FAILED — cargo test passed, but implementation has not started."
  echo "Write a failing test for the new behavior before implementing."
  exit 3
fi
```

Non-zero exit (compile error OR test failure) is sufficient. Both are valid
RED states in Rust — a new module's first test often fails to compile
because the referenced types do not yet exist.

### GREEN checkpoint (Step 6g)

```bash
cargo test --quiet 2>&1 | tee /tmp/maestro-<n>-<ts>/green.log
green_exit=${PIPESTATUS[0]}
if [ $green_exit -ne 0 ]; then
  echo "GREEN GATE FAILED — tests still failing after implementation."
  echo "See /tmp/maestro-<n>-<ts>/green.log"
  exit 4
fi
```

Exit zero required. Re-run after refactor (Step 6h).

### Skipping rules

| task_type | RED (6e) | GREEN (6g) |
|-----------|----------|------------|
| `implementation` | fires | fires |
| `refactor` | skipped | fires |
| `test-only` | skipped | fires |
| `docs` | skipped | skipped |

Default (unclassified) = `implementation`. Strictest behavior is the safe
default.

### Location

Inline in `implement.md` as literal shell snippets the orchestrator runs
via the `Bash` tool. Kept inline for v1 because the snippets are small and
self-documenting. If they grow, extract into `.claude/hooks/red-gate.sh` /
`green-gate.sh`.

### Shell compatibility note

The RED and GREEN snippets use `${PIPESTATUS[0]}` to capture the exit code
of `cargo test` through a `tee` pipe. `PIPESTATUS` is **bash-specific**;
it does not exist in POSIX `sh` or `dash`. The snippets are therefore
required to run under `bash`, not `sh`.

In practice this is not a concern because:

1. The `Bash` tool in Claude Code invokes `/opt/homebrew/bin/fish` on
   this machine (per the session environment), which delegates shell
   commands to `bash` via the standard shebang on scripts. Inline `bash`
   commands executed via the tool run in a bash subshell.
2. On macOS (this project's target), `/bin/bash` is always available.
3. The hook script `.claude/hooks/implement-gates.sh` explicitly declares
   `#!/usr/bin/env bash` to guarantee the interpreter, regardless of the
   caller's default shell.

If the harness is ever ported to a minimal Linux container (e.g., Alpine)
where `bash` isn't default, the baseline-green and gate snippets must
either install `bash` or rewrite to POSIX-compatible exit-code capture:

```sh
cargo test --quiet > "$LOG" 2>&1
red_exit=$?
cat "$LOG"  # equivalent to tee for visibility
```

Documented but not implemented for v1.

## Idempotency UX

Soft idempotency. No state files, no receipt system. One UX prompt at Step 5
when an existing branch is detected.

### Trigger

```bash
existing=$(git branch --list "feat/issue-<n>-*" | head -1 | sed 's/^[ *]*//')
```

- Empty → proceed with normal branch creation.
- Non-empty → present the prompt.
- User is already on the matching branch → skip the prompt, implicit Continue.

### The prompt

```
Branch `feat/issue-<n>-<slug>` already exists.

Recent commits on that branch:
  <git log main..HEAD --oneline -5 output>

  (C)ontinue on this branch
  (R)estart — delete branch and start over
  (A)bort

Choice [C/R/A]:
```

### (C)ontinue semantics

1. `git checkout <existing-branch>`.
2. Re-invoke the gatekeeper (always safe to re-run).
3. Continue into Step 6, but pass `git log --oneline main..HEAD` as context
   to the architect and QA subagents. Specifically, prepend this to each
   delegated prompt:

   > **Context for resumption:** This branch already has commits. Here is
   > the history since `main`:
   >
   > ```
   > <git log --oneline main..HEAD output>
   > ```
   >
   > Before producing a full blueprint, inspect these commits (via Read
   > / Grep on the branch). If the work described by the issue appears
   > substantially done (architecture scaffolded, tests present, or
   > implementation in place), return a **minimal response** acknowledging
   > the existing state and listing only what remains. Do not duplicate
   > work already in the branch. If the branch diverges from what you would
   > design (e.g., different module layout, different abstractions), flag
   > the divergence and recommend either reconciling or restarting — do
   > not silently layer a conflicting plan on top.

4. Run RED gate.
   - If RED (test failing) → proceed to implementation.
   - If GREEN (tests pass) → skip to Step 6h (refactor) / 6g (GREEN re-verify).
5. Proceed from wherever the flow naturally lands.

**Divergence handling:** If the architect or QA subagent flags a
divergence (step 3's prompt explicitly asks them to), the orchestrator
surfaces this to the user with a prompt:

```
Architect detected divergence between the existing branch and the
issue's requirements:

<architect's divergence summary>

  (R)econcile — continue and let subagents bridge the gap
  (S)witch to Restart — delete and start over
  (A)bort — inspect manually

Choice [R/S/A]:
```

This closes the underspecified hand-off that the spec reviewer flagged:
rather than the flow silently layering a conflicting plan, the orchestrator
forces a conscious decision when the architect's blueprint diverges from
branch reality.

### (R)estart semantics

Destructive — requires typed confirmation:

```
About to delete branch `feat/issue-<n>-<slug>` and any uncommitted changes on it.
Type RESTART (uppercase, exactly) to confirm:
```

Only literal `RESTART` proceeds. Then:

```bash
git checkout main
git branch -D feat/issue-<n>-*
```

If the branch has been pushed (`git rev-parse --abbrev-ref --symbolic-full-name @{u}`
succeeds), an additional prompt:

```
This branch has been pushed to origin. Local branch will be deleted.
Also delete remote branch? (y/N):
```

Default no — remote deletion requires explicit `y`. On `y`:

```bash
git push origin --delete <branch>
```

After cleanup, create fresh branch and proceed with normal flow.

### (A)bort semantics

Exit cleanly. Print:

```
Aborted. To inspect the existing branch:
  git checkout <branch>
```

### Edge cases

- **Multiple matching branches** (shouldn't happen): list all, ask which one
  to continue from or to restart.
- **Branch already merged to main**: treat as Restart with no prompt —
  merged work can't be meaningfully continued.
- **User already on the matching branch**: skip prompt, run Continue
  implicitly.

## Edge Cases and Pre-flight Bridge

### Pre-flight bridge

Inside `.claude/hooks/implement-gates.sh`, after mechanical checks:

```bash
if [ -x .claude/hooks/preflight.sh ]; then
  bash .claude/hooks/preflight.sh
  preflight_exit=$?
  if [ $preflight_exit -ne 0 ]; then
    echo "Pre-flight CI checks failed. Fix before starting a new branch."
    exit $preflight_exit
  fi
fi
```

**Ownership — explicit:** The CI-gates spec (separate project, not this one)
owns the contents of `preflight.sh`, its tests, and its exit-code semantics
from 7+. This spec's responsibility ends at the conditional invocation
above. The file is absent today; the harness works fine without it. When
the CI-gates project lands, it drops `preflight.sh` into place and the
bridge activates automatically.

Implementers should resist the temptation to "pre-stub" `preflight.sh`
from this spec's PRs — that mixes concerns and forces coordination between
two independent projects. If `preflight.sh` needs scaffolding, it lives in
the CI-gates spec's rollout, not here.

### Audit trail

Every invocation writes to `/tmp/maestro-<n>-<timestamp>/`:
- `gate-hook.log` — pre-check hook stdout/stderr.
- `gatekeeper.json` — parsed gatekeeper report.
- `red.log`, `green.log` — RED/GREEN checkpoint output.

The command prints the log directory path at handoff (success) and inside
each gate-failure message (failure). No active cleanup — `/tmp` is ephemeral.

### Parallel invocations

Not defended against. Documented in the command body:

> Do not run `/implement` for the same issue concurrently in two sessions.

A lock file system is overkill for a single-dev tool.

### Non-Rust projects

Out of scope. The RED/GREEN gates hard-code `cargo test`. If the harness is
ever ported, replace with a language-appropriate test runner lookup.

## Exit Code Convention

Consistent across the pre-check hook, preflight bridge, and inline gate
snippets.

| Code | Source | Meaning |
|------|--------|---------|
| 0 | any | Continue / success |
| 1 | hook / command | Generic failure (gh missing, not authed, not in repo, closed issue, training mode rejected) |
| 2 | hook | Baseline cargo test failing |
| 3 | command (inline) | RED gate failed |
| 4 | command (inline) | GREEN gate failed |
| 5 | command (after gatekeeper) | Gatekeeper FAIL (DOR, blockers, contracts) |
| 6 | hook | Dirty tree, user declined stash |
| 7+ | preflight.sh | Reserved for CI-gates project |

## Testing Strategy

### Layer 1 — Shell scripts (deterministic)

Framework: [`bats`](https://github.com/bats-core/bats-core).

Test suite: `tests/hooks/implement-gates.bats`.

Fixtures:
- Temp git repo.
- `PATH`-shimmed fake `gh` with canned JSON responses.
- Fake `cargo` for baseline/RED/GREEN simulation.

Covered cases (minimum):
- Clean tree, authed gh, open issue, blockers closed → exit 0.
- Dirty tree, user A → exit 6.
- Dirty tree, user S → `git stash list` contains auto-stash.
- Closed issue → exit 1 with the right message.
- Baseline cargo test failing → exit 2.
- Missing gh → exit 1 with install hint.
- Preflight present + zero → continue.
- Preflight present + non-zero → propagate exit code.
- Preflight absent → silent skip.

Required at PR time for any change to hook scripts.

### Layer 2 — Gatekeeper parser (deterministic)

Test suite: `tests/hooks/parse_gatekeeper_test.py`.

Fixtures (strings, in-repo):
- Valid report with status PASS.
- Valid report with status FAIL + all fields populated.
- Malformed fence (opening without closing).
- Multiple fences — first one used.
- Missing required fields — parser raises explicit error.

Framework: Python `unittest` (stdlib only). Required at PR time.

### Layer 3 — Gatekeeper subagent conformance (non-deterministic)

Fixtures directory: `tests/gatekeeper/fixtures/`.

Baseline fixtures (~10):
- `good-feature.json` — clean feature, expect PASS.
- `good-bug.json` — clean bug, expect PASS.
- `missing-acceptance.json` — expect `dor.missing_sections: [Acceptance Criteria]`.
- `weak-acceptance.json` — prose only, expect `dor.weak_sections: [Acceptance Criteria]`.
- `blocker-open.json` — expect `blockers.passed: false`.
- `blocker-self-ref.json` — expect FAIL with self-ref reason.
- `endpoint-no-schema.json` — expect `contracts.passed: false`.
- `cross-repo-blocker.json` — expect resolution via `gh api`.
- `docs-label.json` — expect `task_type: docs`.
- `refactor-label.json` — expect `task_type: refactor`.

Runner: invokes the gatekeeper with each fixture (mocked `gh` responses),
asserts on the fenced structured region only. Tolerates prose variation
above the fence.

Cadence: weekly cron + before-release. Not per-PR (subagent calls are
slow + costly).

### Layer 4 — End-to-end smoke (manual)

Checklist: `docs/harness-acceptance.md`.

Steps:
1. Seed a scratch GitHub repo with a fully-DOR'd issue and one closed
   blocker.
2. Run `/implement #<n>`. Walk through each expected gate message.
3. Verify: pre-check runs, gatekeeper PASS, branch created, architect +
   QA invoked, RED gate fires and blocks if no failing test, GREEN gate
   fires after implementation.
4. Re-run `/implement #<n>` on the same branch. Verify (C)/(R)/(A) prompt.
5. Kill the flow mid-way, re-run, choose (R), verify branch deletion with
   typed `RESTART` confirmation.

Cost: ~10 minutes. Run before any release that modifies the harness.

### Layer 5 — Observability

`/tmp/maestro-<n>-<timestamp>/` logs + gate-failure messages that reference
the log paths. No structured telemetry for v1. Upgrade if recurring
incidents demand it.

## File Inventory

**New files:**
- `.claude/agents/subagent-gatekeeper.md` — consultive subagent definition.
- `.claude/hooks/implement-gates.sh` — pre-check hook.
- `.claude/hooks/parse-gatekeeper-report.py` — parser utility.
- `tests/hooks/implement-gates.bats` — shell tests.
- `tests/hooks/parse_gatekeeper_test.py` — parser tests.
- `tests/gatekeeper/fixtures/*.json` — ~10 conformance fixtures.
- `tests/gatekeeper/run-conformance.sh` — conformance runner.
- `docs/harness-acceptance.md` — manual acceptance checklist.

**Modified files:**
- `.claude/commands/implement.md` — rewritten with explicit bash gates.
- `.claude/CLAUDE.md` — adds `subagent-gatekeeper` to the registry table.

**Not modified (out of scope):**
- `.claude/commands/pushup.md` — already handles milestone graph updates;
  unaffected by this spec.
- `.claude/hooks/preflight.sh` — owned by the CI-gates spec.

## Rollout Plan (high level)

Detailed breakdown belongs to the implementation-plan phase. At a glance:

1. **Gatekeeper subagent + parser utility + tests.** New subagent lands
   with its conformance fixtures and the tiny Python parser.
2. **Pre-check hook + shell tests.** The `bats` suite lands with the hook.
3. **Rewrite `implement.md`.** Wire both pieces into the new flow. RED/GREEN
   bash snippets inline.
4. **Acceptance checklist + CLAUDE.md registry update.**
5. **End-to-end walk** against a scratch repo. Fix any regressions before
   tagging.

Each of 1-3 is an independent PR. 4-5 bundle into a final cleanup PR.

## Open Questions / Deferred Decisions

Surfaced during brainstorming, explicitly parked:

- **Cross-repo blocker resolution:** kept in v1 (`gh issue view owner/repo#N`).
  Small added surface, real utility.
- **Remote branch deletion on Restart:** default-no explicit prompt. Accepted
  risk that the command's surface grows slightly.
- **Hook reads stdin for dirty-tree prompt:** works via the `Bash` tool.
  Accepted risk that a future runtime change could buffer stdin differently.
  If it breaks, the orchestrator takes over the prompt via `AskUserQuestion`.
- **`bats` vs `shellspec` vs plain shell + diff:** `bats` chosen — mature,
  well-known, readable.
- **JSON fixtures vs real test-repo issues:** JSON fixtures only, v1.
  Real-repo testing is covered by the manual acceptance checklist.
- **Python stdlib dependency for the parser:** accepted. Python ships on
  macOS + most Linux images. A pure-shell YAML parser would be uglier than
  the problem justifies.

- **Baseline-green cost optimization:** v1 runs the full `cargo test` suite
  as the baseline assertion, which adds 30+ seconds on large projects. A
  future optimization: use `cargo test --no-run --quiet` (compile-only, no
  execution, ~5-10 seconds) for the baseline. This catches the most common
  failure mode (broken tree from a recent unrelated commit) without running
  the full suite. It would NOT catch a regression that only surfaces at
  runtime, but the RED gate catches those anyway — any test that now fails
  at runtime will make the RED gate pass trivially, which baseline-green
  exists to prevent. Deferred to v2 once the harness has been exercised
  enough to confirm the full-run assertion is overkill.

## References

- `.claude/CLAUDE.md` — orchestrator agent rules (DOR §3, Dependency §4,
  TDD §5, subagent registry).
- `.claude/commands/implement.md` — current command (to be rewritten).
- `.claude/commands/pushup.md` — handles milestone graph updates on closure.
- `.claude/commands/validate-contracts.md` — read-only contract validator.
- Memory: `feedback_rust_testing.md`, `feedback_rust_style.md` — project
  testing and style discipline.
