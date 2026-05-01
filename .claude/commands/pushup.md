# Push Up

Commit semantically, push, create PR, link issue, and complete tasks.

**Usage:** `/pushup` or `/pushup #123` (where #123 is the issue number)

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
5. Create a **semantic commit** following [Conventional Commits](https://www.conventionalcommits.org/):
   - `feat:` for new features
   - `fix:` for bug fixes
   - `refactor:` for refactoring
   - `test:` for test additions/changes
   - `docs:` for documentation
   - `chore:` for maintenance tasks
   - `style:` for formatting changes
   - `perf:` for performance improvements
6. The commit message body should reference the issue: `Closes #<issue-number>`
7. Use HEREDOC format for the commit message

### Step 3: Push to Remote

1. Check if the current branch tracks a remote branch: `git rev-parse --abbrev-ref --symbolic-full-name @{u} 2>/dev/null`
2. If no upstream exists, push with `-u`: `git push -u origin <branch-name>`
3. If upstream exists, push normally: `git push`

### Step 4: Create Pull Request

1. Check if a PR already exists for this branch: `gh pr view --json number 2>/dev/null`
2. If NO existing PR, create one:
   - Title: Short, descriptive (under 70 chars), matching the semantic commit type
   - Body format:

```
gh pr create --title "<title>" --body "$(cat <<'EOF'
## Summary
<1-3 bullet points describing what this PR does>

Closes #<issue-number>

## Test plan
- [ ] <testing checklist items>
EOF
)"
```

3. If PR already exists, update it if needed: `gh pr edit <pr-number> --body "..."`

4. **Surface the new PR to a running maestro TUI for auto-review (#545 P1).** After a successful `gh pr create`, write a single-line JSON marker to `~/.maestro/last-pr-created`. A running maestro instance polls this file once per `check_completions` tick; on a fresh write it enqueues `TuiCommand::PrCreated` and triggers `/review`.

The write must be atomic (write to `.tmp`, then `mv`) so the consumer never reads a partially-written line. The maestro reader also refuses to follow a symlink at this path and validates `owner` and `repo` against argv-injection (security review concern #8 on #545).

```bash
mkdir -p "$HOME/.maestro"
ts=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
marker="$HOME/.maestro/last-pr-created"
printf '{"pr_number":%d,"owner":"%s","repo":"%s","ts":"%s"}\n' \
  "<pr-number>" "<owner>" "<repo>" "$ts" \
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

1. **Fetch the issue's milestone:**
```bash
gh issue view <issue-number> --json milestone --jq '.milestone.number'
```

2. **If the issue has a milestone**, fetch its current description:
```bash
gh api repos/<owner>/<repo>/milestones/<milestone-number> --jq '.description'
```

3. **Idempotency check (MANDATORY before any string-replace):**

   - If the description already contains `• ✅ #<issue-number>` (or `- ✅ #<issue-number>`) at the start of a bullet line, the issue is already marked complete. Log "issue already marked complete in milestone graph — skipping idempotent re-stamp" and SKIP to step 7 (do not PATCH).
   - This keeps a re-run of `/pushup` safe.

4. **Anchored bullet replace.** Update the issue's bullet entry from `• #<issue-number>` to `• ✅ #<issue-number>` — but ONLY when `#<issue-number>` appears at the start of a bullet item (preceded by start-of-line + `• ` or `- `). NEVER replace `#<issue-number>` tokens that appear inside prose.

   Safe (DO replace):
   ```
   • #521 feat(tui): keybinding to manually trigger PR creation
   - #521 feat(tui): keybinding to manually trigger PR creation
   ```

   Unsafe (DO NOT replace — these are prose, not bullet entries):
   ```
   Level 1 — depends on #521 (must merge first):
   This blocks #521 because the AC4 preflight needs to land first.
   See note in #521 about the architect's reconciliation.
   ```

   The model-driven PATCH must read the description, find the matching bullet line, replace ONLY that line, and leave every other occurrence of `#<issue-number>` untouched.

5. **Level header roll-up.** If, after the replace in step 4, every bullet at the same indentation level inside its `Level N — …:` block is now prefixed with `✅`, append ` (COMPLETED ✅)` to that level header — but only if it is not already there (idempotent).

6. **Sequence-line update.** The `Sequence:` line uses tokens like `#521 ∥ #520 ∥ #525` separated by `→` and `∥`. Replace only EXACT token matches: a `#<issue-number>` token bounded by whitespace, `(`, `)`, `→`, or `∥` — not a prefix-match. Example:

   - Token `#52` must NOT match inside `#521`. Use word-boundary logic: `#52` is a different token than `#521`.
   - When all bullets in a level are now ✅, the parenthesized group of that level in `Sequence:` becomes `✅(LN: #a ∥ #b ∥ ...)` (idempotent — if it is already in `✅(LN: …)` form, leave it alone).

7. **PATCH the milestone (only if step 3's idempotency check passed):**
```bash
gh api repos/<owner>/<repo>/milestones/<milestone-number> -X PATCH -f description="<updated-description>"
```

8. **Verify the PATCH succeeded** by re-fetching `description` and asserting your edit is present. If verification fails, abort `/pushup` here — the issue is still open, so the run can be retried safely.

**This step is NON-NEGOTIABLE.** Every closed issue MUST be reflected in the milestone graph. If it fails, **STOP** — do not proceed to Step 6.5 (issue close).

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
✅ Push Up Complete!

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
