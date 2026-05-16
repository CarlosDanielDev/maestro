---
command: implement
version: 1.0.0
description: Fetch a GitHub issue and implement it following the enforced TDD harness.
placeholders:
  - HOOK_GATE
  - INCLUDE
  - INVOKE_SUBAGENT
includes:
  - core/premises.md
  - core/tdd-cycle.md
source_provenance:
  ported_from: .claude/commands/implement.md
  ported_at: 2026-05-13
---

# Implement Issue

Fetch a GitHub issue and implement it following the enforced TDD harness.

**Usage:** `/implement #123` or `/implement 123 --english --orchestrator`

{{INCLUDE path="core/premises.md"}}

---

## Arguments

`$ARGUMENTS` contains the issue number and optional flags.

### Supported Flags

| Flag | Short | Purpose |
|------|-------|---------|
| `--english` | `-e` | Set language to English |
| `--portuguese` | `-pt` | Set language to Português do Brasil |
| `--spanish` | `-s` | Set language to Español |
| `--orchestrator` | `-o` | Use Subagents Orchestrator mode |
| `--vibe-coding` | `-vc` | Use Vibe Coding mode |
| `--continue` | — | Step 5 idempotency: continue on existing branch (skip prompt) |
| `--restart` | — | Step 5 idempotency: delete existing branch and start fresh (skip prompt; still requires inner `RESTART` confirmation if interactive) |
| `--dirty-tree-action=stash\|abort\|ask` | — | Pre-check Gate 6: how to handle a dirty working tree. Pass-through to the gate hook. |
| `--auto-comment` | — | Step 4 DOR remediation: auto-post the gatekeeper-drafted comment + `needs-info` label. Default: print the proposed action and STOP for human review. |

`--training` / `-t` is explicitly rejected — Training mode is for provider-configuration directories, not implementation.

`--continue` and `--restart` are mutually exclusive — passing both is an error.

---

## Instructions

### Step 0: Parse arguments

Extract from `$ARGUMENTS`:
1. **Issue number**: first `\d+` with optional `#` prefix. Export as `$ISSUE_NUMBER` for downstream steps.
2. **Language flag** (if present).
3. **Mode flag** (if present).
4. **Idempotency flag** (`--continue` or `--restart`, if present). Reject with exit 1 if both are passed.
5. **Dirty-tree action** (`--dirty-tree-action=...`, if present) — captured for pass-through to the pre-check hook in Step 2.
6. **Auto-comment opt-in** (`--auto-comment`, if present) — export `AUTO_COMMENT=1` for Step 4 DOR auto-remediation.

```bash
export ISSUE_NUMBER="<n>"  # substitute the parsed number
```

All subsequent gate commands in this file reference `$ISSUE_NUMBER`, not bare `<n>`.

If `--training` or `-t` is detected, output:

```
Training mode is for configuring provider agents, skills, and commands.
/implement is for building features from GitHub issues.
Drop --training, or use a free-form prompt for provider configuration.
```

and exit 0 (not an error).

If no issue number found, ask: "Which issue should I implement?"

### Step 1: Language and mode selection

If flags provided, honor them. Otherwise, ask the user.

### Step 2: Pre-check hook (GATE — MANDATORY)

Run the mechanical pre-check hook. Abort on non-zero exit, printing stderr verbatim. Pass through `--dirty-tree-action=...` from Step 0 if the user supplied it.

```bash
{{HOOK_GATE script="implement-gates.sh" args="$ISSUE_NUMBER ${DIRTY_TREE_ACTION:+--dirty-tree-action=$DIRTY_TREE_ACTION}"}}
```

The hook prints `gate log dir: /tmp/maestro-$ISSUE_NUMBER-<ts>` on success AND writes the same path to a sentinel file. The sentinel allows subsequent shell calls to recover `GATE_LOG_DIR` without relying on env-var persistence (each fresh shell loses `export`).

The resolution chain (symlink-attack hardening on multi-user Linux) is `$XDG_RUNTIME_DIR` → `$HOME/.cache/maestro` → `${TMPDIR:-/tmp}`. The hook prints the chosen path as `sentinel: <path>` on stdout.

Recovery pattern for any later step (walks the same chain):

```bash
GATE_LOG_DIR=""
for candidate in \
  "${XDG_RUNTIME_DIR:-}/maestro-current-gate-dir" \
  "${HOME}/.cache/maestro/maestro-current-gate-dir" \
  "/tmp/maestro-current-gate-dir"; do
  if [ -n "$candidate" ] && [ -f "$candidate" ]; then
    GATE_LOG_DIR=$(cat "$candidate")
    break
  fi
done
```

The sentinel file is overwritten on the next `/implement` run, so it always points at the current gate session.

**Non-TTY behavior of the hook:** the gate hook detects no-TTY (`! -t 0`) and refuses to prompt for the dirty-tree action. If the tree is dirty and `--dirty-tree-action=` is not passed, the hook prints an actionable message and exits 6. Re-run the slash command with `--dirty-tree-action=stash` (auto-stash) or `--dirty-tree-action=abort` (clean fail) to unblock.

Exit codes:
- `0` — proceed.
- `1` — generic failure (gh missing, not authed, not in repo, closed issue). Abort with the hook's stderr.
- `2` — baseline cargo test failing. Abort — fix the baseline before starting.
- `6` — dirty tree, user chose abort. Abort cleanly.
- `7+` — preflight failure. Abort with its stderr.

### Step 3: Read cached issue JSON and summary

The hook has cached the issue JSON at `$GATE_LOG_DIR/issue.json` and a condensed
DOR summary at `$GATE_LOG_DIR/issue-summary.md`. Read them directly — no second
`gh` call. Prefer `issue-summary.md` for downstream subagents that only need the
issue requirements; keep `issue.json` for mechanical gates and fallback checks
that need labels, state, comments, or other structured fields.

```bash
cat "$GATE_LOG_DIR/issue.json"
cat "$GATE_LOG_DIR/issue-summary.md"
```

### Step 4: Gatekeeper (GATE — MANDATORY)

Run the mechanical DOR lint first. It writes `$GATE_LOG_DIR/dor-lint.json` and
always exits 0; the JSON verdict decides whether the subagent can be skipped.

```bash
scripts/dor-lint.sh "$GATE_LOG_DIR/issue.json" >/dev/null

lint_passed=$(jq -r .passed "$GATE_LOG_DIR/dor-lint.json")
all_blockers_closed=$(jq -r '.blocker_states | to_entries | all(.value == "CLOSED")' "$GATE_LOG_DIR/dor-lint.json")
contract_required=$(jq -r '.reasons | index("contract validation required") != null' "$GATE_LOG_DIR/dor-lint.json")

if [ "$lint_passed" = "true" ] && \
   [ "$all_blockers_closed" = "true" ] && \
   [ "$contract_required" = "false" ]; then
  python3 - "$GATE_LOG_DIR/dor-lint.json" "$GATE_LOG_DIR/gatekeeper.json" <<'PY'
import json
import sys

lint = json.load(open(sys.argv[1]))
task_map = {
    "feature": "implementation",
    "bug": "implementation",
    "trivial": "implementation",
    "docs": "docs",
    "refactor": "refactor",
}
report = {
    "report_version": 1,
    "status": "PASS",
    "task_type": task_map.get(lint.get("task_type"), "implementation"),
    "dor": {"passed": True, "missing_sections": [], "weak_sections": []},
    "blockers": {"passed": True, "open": []},
    "contracts": {"passed": True, "missing": []},
    "remediation": {"comment_body": "", "labels_to_add": []},
    "reasons": [],
}
json.dump(report, open(sys.argv[2], "w"), separators=(",", ":"))
open(sys.argv[2], "a").write("\n")
PY
  task_type=$(jq -r .task_type "$GATE_LOG_DIR/gatekeeper.json")
  echo "Gatekeeper fast-path PASS (task_type: $task_type)"
  export TASK_TYPE="$task_type"
else
  echo "DOR lint did not qualify for fast-path; invoking subagent-gatekeeper."
fi
```

If the fast path did not write `$GATE_LOG_DIR/gatekeeper.json`, invoke the gatekeeper subagent:

{{INVOKE_SUBAGENT name="gatekeeper" prompt="Classify issue $ISSUE_NUMBER (DOR, blockers, contracts, task_type). Return the structured JSON gatekeeper report."}}

The subagent's response will contain a fenced `json gatekeeper` code block. Pipe its full response through the parser:

```bash
if [ ! -f "$GATE_LOG_DIR/gatekeeper.json" ]; then
  echo "$SUBAGENT_RESPONSE" | python3 scripts/parse_gatekeeper_report.py > "$GATE_LOG_DIR/gatekeeper.json"
fi
```

Then branch on the parsed report:

```bash
status=$(jq -r .status "$GATE_LOG_DIR/gatekeeper.json")
task_type=$(jq -r .task_type "$GATE_LOG_DIR/gatekeeper.json")

if [ "$status" = "FAIL" ]; then
  dor_passed=$(jq -r .dor.passed "$GATE_LOG_DIR/gatekeeper.json")
  if [ "$dor_passed" = "false" ]; then
    comment_body=$(jq -r .remediation.comment_body "$GATE_LOG_DIR/gatekeeper.json")
    labels_csv=$(jq -r '.remediation.labels_to_add | join(", ")' "$GATE_LOG_DIR/gatekeeper.json")

    if [ "${AUTO_COMMENT:-0}" = "1" ]; then
      gh issue comment "$ISSUE_NUMBER" --body "$comment_body"
      for label in $(jq -r '.remediation.labels_to_add[]' "$GATE_LOG_DIR/gatekeeper.json"); do
        gh issue edit "$ISSUE_NUMBER" --add-label "$label"
      done
      echo "Gatekeeper FAIL: DOR auto-remediation posted (--auto-comment)." >&2
    else
      echo "Gatekeeper FAIL: DOR not satisfied." >&2
      echo "Proposed remediation (NOT posted; re-run with --auto-comment to post):" >&2
      echo "" >&2
      echo "  Issue:  #$ISSUE_NUMBER" >&2
      echo "  Labels: $labels_csv" >&2
      echo "  Comment body:" >&2
      echo "  ----" >&2
      printf '%s\n' "$comment_body" | sed 's/^/  /' >&2
      echo "  ----" >&2
    fi
  fi
  echo "Gatekeeper FAIL:" >&2
  jq -r '.reasons[]' "$GATE_LOG_DIR/gatekeeper.json" | while read -r r; do
    echo "  - $r" >&2
  done
  exit 5
fi

echo "Gatekeeper PASS (task_type: $task_type)"
export TASK_TYPE="$task_type"
```

Exit code `5` is reserved for gatekeeper failure.

### Step 5: Branch selection with idempotency

Check for an existing branch matching `feat/issue-${ISSUE_NUMBER}-*`:

```bash
existing=$(git branch --list "feat/issue-${ISSUE_NUMBER}-*" | head -1 | sed 's/^[ *]*//')
```

**If empty:** derive a slug from the issue title and create a new branch:

```bash
slug=$(jq -r .title "$GATE_LOG_DIR/issue.json" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-//;s/-$//' | cut -c -40)
git checkout -b "feat/issue-${ISSUE_NUMBER}-${slug}"
```

**If non-empty:** resolve the idempotency choice using the flags from Step 0, falling back to a TTY prompt when running interactively.

```
Branch `<existing>` already exists.

Recent commits on that branch:
<git log main..HEAD --oneline -5>
```

Resolution rules (in order):

1. If `--continue` was passed → take the **(C)ontinue** path below.
2. If `--restart` was passed → take the **(R)estart** path below (the inner `RESTART` typed confirmation is also waived under `--restart`, since the user already chose this path explicitly).
3. If neither flag was passed AND stdin is a TTY → fire the interactive prompt:

   ```
     (C)ontinue on this branch
     (R)estart — delete branch and start over
     (A)bort

   Choice [C/R/A]:
   ```

4. If neither flag was passed AND stdin is **not** a TTY → default to **(A)bort** with this message:

   ```
   Branch `<existing>` already exists and stdin is not interactive.
   Re-run with `/implement #<N> --continue` (resume) or `/implement #<N> --restart` (start over).
   ```

   Exit cleanly (no error code; the user's next invocation drives the choice).

Handle each choice:

**(C)ontinue:**
- `git checkout <existing>`.
- Re-invoke the gatekeeper (idempotent).
- When delegating to architect/QA in Step 6, prepend the resumption context prompt:

  > **Context for resumption:** This branch already has commits. Here is the history since `main`:
  >
  > ```
  > $(git log --oneline main..HEAD)
  > ```
  >
  > Before producing a full blueprint, inspect these commits. If the work described by the issue appears substantially done (architecture scaffolded, tests present, or implementation in place), return a **minimal response** acknowledging the existing state and listing only what remains. Do not duplicate work already in the branch. If the branch diverges from what you would design (e.g., different module layout, different abstractions), flag the divergence and recommend either reconciling or restarting — do not silently layer a conflicting plan on top.

- **Divergence handling:** if the architect or QA subagent flags divergence, the orchestrator presents a secondary prompt — but only when stdin is a TTY:

  ```
  Architect detected divergence between the existing branch and the
  issue's requirements:

  <architect's divergence summary>

    (R)econcile — continue and let subagents bridge the gap
    (S)witch to Restart — delete and start over
    (A)bort — inspect manually

  Choice [R/S/A]:
  ```

  - **(R)econcile**: proceed with Step 6's subagent sequence, trusting the architect/QA to bridge the gap via follow-up edits.
  - **(S)witch to Restart**: fall through to the Restart flow below (typed `RESTART` confirmation, branch deletion).
  - **(A)bort**: exit cleanly, tell the user to inspect manually.

  **Non-TTY default:** emit the divergence summary to the user, default to **(A)bort**, and instruct: `Re-run with /implement #<N> --restart` to take the Switch-to-Restart path explicitly, or fix the divergence manually then re-run with `--continue`. Do not silently reconcile — divergence is exactly the case the human should adjudicate.

**(R)estart:**
- If reached via the interactive prompt, require typed `RESTART` confirmation. If reached via `--restart`, the flag itself is the confirmation — skip the typed gate.
- `git checkout main && git branch -D "$existing"`.
- If the branch was pushed AND stdin is a TTY, prompt about remote deletion (default no). If non-TTY, skip remote deletion (safer default; user can prune manually).
- Create fresh branch.

**(A)bort:**
- Exit cleanly. Tell the user to `git checkout <branch>` manually to inspect.

If the user is already on the matching branch, skip the prompt and use Continue semantics.

### Step 6: Orchestrator-mode subagent sequence

Vibe mode skips 6a and 6c. All gates use `bash` (not `sh`) — `${PIPESTATUS[0]}` requires it.

The TDD cycle below is non-negotiable. The full canonical fragment:

{{INCLUDE path="core/tdd-cycle.md"}}

#### 6a. Architect → blueprint

Orchestrator mode only. Invoke the architect subagent with the condensed issue
summary from `$GATE_LOG_DIR/issue-summary.md` and the architecture blueprint
request. If Step 5 chose Continue, prepend the resumption context prompt.

{{INVOKE_SUBAGENT name="architect" prompt="Produce architecture blueprint for issue $ISSUE_NUMBER using $GATE_LOG_DIR/issue-summary.md; on Continue, prepend resumption context."}}

#### 6b. `/validate-contracts` (if architect blueprint touches API endpoints)

Skip if no endpoints.

#### 6c. QA → test blueprint

Orchestrator mode only. Invoke the QA subagent with the architect's blueprint. If Step 5 chose Continue, prepend the resumption context prompt.

{{INVOKE_SUBAGENT name="qa" prompt="Produce test blueprint from architect's blueprint for issue $ISSUE_NUMBER; on Continue, prepend resumption context."}}

#### 6d. Write tests from QA blueprint

You (the orchestrator) write tests. No subagent.

#### 6d-bis. Binding-gate selection (CI / non-Rust tasks)

For most tasks `cargo test` is the binding RED/GREEN gate. For tasks where the artifact under test isn't Rust source — workflow YAML, shell scripts, slash-command spec edits, pure deletions — `cargo test` is a regression guard and the binding gate is the tool that actually validates the changed artifact.

| Artifact under test | Binding RED/GREEN gate | Regression guard |
|---|---|---|
| Rust source (`src/**`, `tests/**`) | `cargo test --quiet` | — |
| Workflow YAML (`.github/workflows/**`) | `actionlint` (wired into `ci.yml`) | `cargo test --quiet` |
| Shell scripts (`scripts/**`) | `bash -n` + `shellcheck` on changed files | `cargo test --quiet` |
| Docs (`*.md`, `directory-tree.md`) | none (skipped) | `cargo test --quiet` |

For CI / non-Rust tasks the orchestrator runs the binding gate before and after implementation, and `cargo test` as a regression-only check. The 6e/6g `cargo test` blocks below remain the default; substitute the appropriate gate when the gatekeeper's advisory or the issue body indicates a non-Rust binding gate.

#### 6e. RED checkpoint (GATE)

Skipped if `TASK_TYPE` is `docs` or `refactor`.

```bash
if [ "$TASK_TYPE" != "docs" ] && [ "$TASK_TYPE" != "refactor" ]; then
  cargo test --quiet 2>&1 | tee "$GATE_LOG_DIR/red.log"
  red_exit=${PIPESTATUS[0]}
  if [ $red_exit -eq 0 ]; then
    echo "RED GATE FAILED — cargo test passed, but implementation has not started." >&2
    echo "Write a failing test for the new behavior before implementing." >&2
    exit 3
  fi
fi
```

Exit code `3` reserved for RED failure.

#### 6f. Implement

You (the orchestrator) write the minimum code to make the failing test pass.

#### 6g. GREEN checkpoint (GATE)

Skipped only if `TASK_TYPE` is `docs`.

```bash
if [ "$TASK_TYPE" != "docs" ]; then
  cargo test --quiet 2>&1 | tee "$GATE_LOG_DIR/green.log"
  green_exit=${PIPESTATUS[0]}
  if [ $green_exit -ne 0 ]; then
    echo "GREEN GATE FAILED — tests still failing after implementation." >&2
    echo "See $GATE_LOG_DIR/green.log" >&2
    exit 4
  fi
fi
```

Exit code `4` reserved for GREEN failure.

#### 6h. Refactor (if needed)

Refactor while tests stay green. Re-run the GREEN checkpoint after:

```bash
cargo test --quiet 2>&1 | tee "$GATE_LOG_DIR/green-refactor.log"
[ ${PIPESTATUS[0]} -eq 0 ] || { echo "Refactor broke tests"; exit 4; }
```

#### 6i. Security review

Both modes. Invoke the security analyst against the newly-written code.

{{INVOKE_SUBAGENT name="security-analyst" prompt="Review newly-written code for issue $ISSUE_NUMBER against OWASP Top 10, secrets handling, and input validation."}}

#### 6j. Documentation

Both modes. Mandatory at task end.

{{INVOKE_SUBAGENT name="docs-analyst" prompt="Update documentation and directory-tree.md for issue $ISSUE_NUMBER."}}

### Step 7: Handoff

Print a summary:

```
Implementation complete for Issue #$ISSUE_NUMBER: $TITLE

Gates passed:
  - Pre-check hook (ok)
  - Gatekeeper (task_type: $TASK_TYPE)
  - RED checkpoint (verified failing → passing)   # omit if task_type is docs/refactor
  - GREEN checkpoint (all tests pass)              # omit if task_type is docs

Logs: $GATE_LOG_DIR

Next: run /pushup to commit, push, create PR, and close the issue.
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success — also returned when `--training` is passed (wrong-tool message, not a failure) |
| 1 | Generic failure (gh missing, not authed, not in repo, closed issue) |
| 2 | Baseline cargo test failing |
| 3 | RED gate failed |
| 4 | GREEN gate failed |
| 5 | Gatekeeper FAIL (DOR, blockers, contracts) |
| 6 | Dirty tree, user declined stash |
| 7+ | Preflight failure (reserved for CI-gates spec) |

---

## Error Handling

- If `gh` CLI not installed → hook exits 1 with install hint.
- If `gh` not authenticated → hook exits 1 with `gh auth login` hint.
- If issue closed → hook exits 1. Re-open first or pick a different issue.
- If dirty tree → prompt (S)tash/(A)bort.
- If baseline fails → exit 2. Fix baseline first.
- If gatekeeper FAILs with DOR missing → proposed remediation printed to stderr for human review, exit 5. Re-run with `--auto-comment` to auto-post the comment and apply `needs-info` label.
- If blockers open → exit 5. Wait for blockers to close.
- If RED/GREEN fails → exit 3/4. Actionable error with log path.

---

## Do Not

- Run `/implement` for the same issue concurrently in two sessions.
- Bypass the hook by invoking subagents directly.
- Skip the RED gate for `implementation` task types — write the failing test first.
