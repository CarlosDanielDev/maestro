# Implement Issue

Fetch a GitHub issue and implement it following the enforced TDD harness.

**Usage:** `/implement #123` or `/implement 123 --english --orchestrator`

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

`--training` / `-t` is explicitly rejected — Training mode is for `.claude/` configuration, not implementation.

---

## Instructions

### Step 0: Parse arguments

Extract from `$ARGUMENTS`:
1. **Issue number**: first `\d+` with optional `#` prefix. Export as `$ISSUE_NUMBER` for downstream steps.
2. **Language flag** (if present).
3. **Mode flag** (if present).

```bash
export ISSUE_NUMBER="<n>"  # substitute the parsed number
```

All subsequent gate commands in this file reference `$ISSUE_NUMBER`, not bare `<n>`.

If `--training` or `-t` is detected, output:

```
Training mode is for configuring .claude/ agents, skills, and commands.
/implement is for building features from GitHub issues.
Drop --training, or use a free-form prompt for .claude/ configuration.
```

and exit 0 (not an error).

If no issue number found, ask: "Which issue should I implement?"

### Step 1: Language and mode selection

If flags provided, honor them. Otherwise, ask the user.

### Step 2: Pre-check hook (GATE — MANDATORY)

Run the mechanical pre-check hook. Abort on non-zero exit, printing stderr verbatim.

```bash
bash .claude/hooks/implement-gates.sh "$ISSUE_NUMBER"
```

The hook prints `gate log dir: /tmp/maestro-$ISSUE_NUMBER-<ts>` on success; capture this path and `export GATE_LOG_DIR=<path>` for downstream steps.

Exit codes:
- `0` — proceed.
- `1` — generic failure (gh missing, not authed, not in repo, closed issue). Abort with the hook's stderr.
- `2` — baseline cargo test failing. Abort — fix the baseline before starting.
- `6` — dirty tree, user chose abort. Abort cleanly.
- `7+` — preflight.sh failure. Abort with its stderr.

### Step 3: Read cached issue JSON

The hook has cached the issue JSON at `$GATE_LOG_DIR/issue.json`. Read it directly — no second `gh` call.

```bash
cat "$GATE_LOG_DIR/issue.json"
```

### Step 4: Gatekeeper (GATE — MANDATORY)

Invoke `subagent-gatekeeper` via the `Agent` tool. Pass the issue JSON (from `$GATE_LOG_DIR/issue.json`), selected mode, and repo name.

The subagent's response will contain a fenced `json gatekeeper` code block. Pipe its full response through the parser:

```bash
echo "$SUBAGENT_RESPONSE" | python3 .claude/hooks/parse_gatekeeper_report.py > "$GATE_LOG_DIR/gatekeeper.json"
```

Then branch on the parsed report:

```bash
status=$(jq -r .status "$GATE_LOG_DIR/gatekeeper.json")
task_type=$(jq -r .task_type "$GATE_LOG_DIR/gatekeeper.json")

if [ "$status" = "FAIL" ]; then
  dor_passed=$(jq -r .dor.passed "$GATE_LOG_DIR/gatekeeper.json")
  if [ "$dor_passed" = "false" ]; then
    # Auto-remediation for DOR failures.
    comment_body=$(jq -r .remediation.comment_body "$GATE_LOG_DIR/gatekeeper.json")
    gh issue comment "$ISSUE_NUMBER" --body "$comment_body"
    for label in $(jq -r '.remediation.labels_to_add[]' "$GATE_LOG_DIR/gatekeeper.json"); do
      gh issue edit "$ISSUE_NUMBER" --add-label "$label"
    done
  fi
  # Print reasons for the operator.
  echo "Gatekeeper FAIL:" >&2
  jq -r '.reasons[]' "$GATE_LOG_DIR/gatekeeper.json" | while read -r r; do
    echo "  - $r" >&2
  done
  exit 5
fi

# PASS — continue with the classified task_type.
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

**If non-empty:** fire the idempotency prompt. Show recent commits:

```
Branch `<existing>` already exists.

Recent commits on that branch:
<git log main..HEAD --oneline -5>

  (C)ontinue on this branch
  (R)estart — delete branch and start over
  (A)bort

Choice [C/R/A]:
```

Handle each choice:

**(C)ontinue:**
- `git checkout <existing>`.
- Re-invoke the gatekeeper (idempotent).
- When delegating to architect/QA in Step 6, prepend the resumption context prompt from the spec (§Idempotency UX → Continue semantics):

  > **Context for resumption:** This branch already has commits. Here is the history since `main`:
  >
  > ```
  > $(git log --oneline main..HEAD)
  > ```
  >
  > Before producing a full blueprint, inspect these commits (via Read / Grep on the branch). If the work described by the issue appears substantially done (architecture scaffolded, tests present, or implementation in place), return a **minimal response** acknowledging the existing state and listing only what remains. Do not duplicate work already in the branch. If the branch diverges from what you would design (e.g., different module layout, different abstractions), flag the divergence and recommend either reconciling or restarting — do not silently layer a conflicting plan on top.

- **Divergence handling (spec §Idempotency UX → Divergence handling):** if the architect or QA subagent flags divergence, the orchestrator presents a secondary prompt:

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

**(R)estart:**
- Require typed `RESTART` confirmation.
- `git checkout main && git branch -D "$existing"`.
- If the branch was pushed, prompt about remote deletion (default no).
- Create fresh branch.

**(A)bort:**
- Exit cleanly. Tell the user to `git checkout <branch>` manually to inspect.

If the user is already on the matching branch, skip the prompt and use Continue semantics.
