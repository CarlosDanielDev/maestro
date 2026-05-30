---
command: pushup
version: 1.0.0
description: Commit semantically, push, create PR, link issue, and complete tasks.
placeholders:
  - INCLUDE
includes:
  - core/premises.md
  - core/dependency-graph.md
source_provenance:
  ported_from: .claude/commands/pushup.md
  ported_at: 2026-05-13
---

# Push Up

Commit semantically, push, create PR, link issue, and complete tasks.

**Usage:** `/pushup` or `/pushup #123` (where #123 is the issue number)

{{INCLUDE path="core/premises.md"}}

---

## Instructions

This command automates the end-of-feature workflow. Execute ALL steps in order.

### Step 1: Determine the Issue

If `$ARGUMENTS` contains an issue number (e.g., `#123` or `123`), use that.

Otherwise, detect the issue from:
1. The current branch name (e.g., `feat/issue-123-description` → issue #123)
2. Recent commit messages mentioning an issue
3. If not found, ask the user: "Which issue does this PR close? (e.g., #123)"

### Step 2: Semantic Commit

1. Run `git status` to see all changes
2. Run `git diff --staged` and `git diff` to understand what changed
3. Run `git log --oneline -5` to match the repo's commit style
4. Stage all relevant files (avoid secrets, .env, credentials)
5. Ensure a gate log directory exists for editable drafts:

```bash
: "${GATE_LOG_DIR:=$(mktemp -d)}"
export GATE_LOG_DIR
```

6. Generate the mechanical commit draft:

```bash
scripts/commit-helper.sh <issue-number>
```

The helper chooses the Conventional Commits prefix from issue labels, writes
`$GATE_LOG_DIR/commit-draft.txt`, and includes `Closes #<issue-number>`.

7. Fill in only the first-line subject placeholder. Keep the helper-selected
   prefix and `Closes #<issue-number>` body unchanged. Use the existing HEREDOC
   pattern to overwrite the draft, then commit from the draft file:

```bash
cat > "$GATE_LOG_DIR/commit-draft.txt" <<'EOF'
<prefix>: <filled subject>

Closes #<issue-number>
EOF
git commit -F "$GATE_LOG_DIR/commit-draft.txt"
```

### Step 3: Push to Remote

1. Check if the current branch tracks a remote branch: `git rev-parse --abbrev-ref --symbolic-full-name @{u} 2>/dev/null`
2. If no upstream exists, push with `-u`: `git push -u origin <branch-name>`
3. If upstream exists, push normally: `git push`

### Step 4: Create Pull Request

1. Check if a PR already exists for this branch: `gh pr view --json number 2>/dev/null`
2. If NO existing PR, create one:
   - Title: use the commit subject exactly
   - Body: generate a skeleton, then fill in only 1-3 summary bullets

```
commit_subject="$(git log -1 --pretty=%s)"
: "${GATE_LOG_DIR:=$(mktemp -d)}"
export GATE_LOG_DIR
scripts/pr-skeleton.sh <issue-number> "$commit_subject"
# Edit only $GATE_LOG_DIR/pr-draft.md summary bullets; keep Closes and Test plan.
gh pr create --title "$commit_subject" --body-file "$GATE_LOG_DIR/pr-draft.md"
```

3. If PR already exists, update it if needed after regenerating/filling the body:
   `gh pr edit <pr-number> --body-file "$GATE_LOG_DIR/pr-draft.md"`

4. **Surface the new PR to a running maestro TUI for auto-review.** After a successful `gh pr create`, write a single-line JSON marker to `~/.maestro/last-pr-created`. A running maestro instance polls this file once per `check_completions` tick; on a fresh write it enqueues `TuiCommand::PrCreated` and triggers `/review`.

The write must be atomic (write to `.tmp`, then `mv`) so the consumer never reads a partially-written line. The maestro reader also refuses to follow a symlink at this path and validates `owner` and `repo` against argv-injection.

```bash
mkdir -p "$HOME/.maestro"
ts=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
marker="$HOME/.maestro/last-pr-created"
printf '{"pr_number":%d,"owner":"%s","repo":"%s","issue_number":%d,"ts":"%s"}\n' \
  "<pr-number>" "<owner>" "<repo>" "<issue-number>" "$ts" \
  > "${marker}.tmp"
mv "${marker}.tmp" "$marker"
```

The marker is consumed-once: maestro deletes it after dispatching the review. If the marker is malformed or fails the owner/repo guard, maestro logs a Warn entry and deletes it.

### Step 5: Link Issue to PR

The `Closes #<issue-number>` in the PR body automatically links the issue.

Additionally, add the issue as a linked reference:
```bash
gh pr edit <pr-number> --add-label "closes-issue"
```

If the repo doesn't have that label, skip this step (don't fail).

### Step 6: Update Milestone Dependency Graph (MANDATORY — runs BEFORE issue close)

Update the milestone's dependency graph to mark this issue with ✅. **This step runs before the issue close so that a failure here leaves the issue open and `/pushup` can be safely re-run** — closing the issue first would orphan the milestone graph if this step blew up.

1. Fetch the issue's milestone:
```bash
MILESTONE=$(gh issue view <issue-number> --json milestone --jq '.milestone.number')
```

2. If the issue has no milestone, log that no milestone graph exists and continue to Step 6.5.

3. If the issue has a milestone, run the mechanical updater and treat its exit code as the gate:
```bash
python3 scripts/update-milestone-graph.py --milestone "$MILESTONE" --issue "<issue-number>"
```

The script handles idempotency, anchored bullet replacement, level-header roll-up, `Sequence:` token-boundary rewrites, PATCH, and post-PATCH verification. A re-run where the issue is already stamped exits 0 with an "already marked" log line.

**This step is NON-NEGOTIABLE.** Every closed issue MUST be reflected in the milestone graph. If it fails, **STOP** — do not proceed to Step 6.5 (issue close).

The full canonical rules for milestone graph updates after issue closure:

{{INCLUDE path="core/dependency-graph.md"}}

### Step 6.5: Complete Tasks (issue close + checks)

Runs AFTER the milestone graph is correctly stamped. If this step fails after Step 6 succeeded, the milestone graph is correctly stamped but the issue stays open — re-run `/pushup` or close manually. **This partial state is recoverable** (running `/pushup` again will idempotently re-detect the milestone is already stamped via Step 6.3 and skip straight to closing the issue).

1. **On the Issue:** Add a comment and close it idempotently. A previous `/pushup` run may have already closed the issue (e.g., it failed mid-Step-6 and the user re-ran); the close step must NOT fail in that case.

```bash
issue_state=$(gh issue view <issue-number> --json state --jq '.state' 2>/dev/null || echo "ERROR")
if [ "$issue_state" = "CLOSED" ]; then
  echo "/pushup: issue #<issue-number> is already CLOSED — skipping comment + close (idempotent re-run)"
elif [ "$issue_state" = "ERROR" ]; then
  echo "/pushup: failed to read issue state; aborting close (will retry on next /pushup)" >&2
  exit 1
else
  gh issue comment <issue-number> --body "Completed in PR #<pr-number>"
  gh issue close <issue-number>
fi
```

2. **On the PR:** Verify all checks pass (informational only, don't block):
```bash
gh pr checks <pr-number> 2>/dev/null || echo "No checks configured or still running"
```

### Step 7: Summary

Print a final summary:

```
Push Up Complete

  Commit:  <commit-hash> (<commit-type>: <short-message>)
  Branch:  <branch-name>
  PR:      #<pr-number> - <pr-title> (<pr-url>)
  Issue:   #<issue-number> - Closed
```

---

## Error Handling

- If `gh` CLI is not installed, tell the user to install it: `brew install gh`
- If not authenticated, tell user to run: `gh auth login`
- If there are no changes to commit, skip to Step 3 (push any unpushed commits)
- If push fails, show the error and stop (don't create PR with stale code)
- If on `main` or `master` branch, WARN the user and ask for confirmation before proceeding

---

## Safety Checks

- NEVER force push
- NEVER push to `main` or `master` without explicit user confirmation
- NEVER commit files matching: `.env*`, `credentials*`, `*.key`, `*.pem`, `*.p12`
- Always show the user what will be committed before committing
