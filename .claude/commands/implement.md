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
