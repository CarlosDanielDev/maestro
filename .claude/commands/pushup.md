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

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

3. If PR already exists, update it if needed: `gh pr edit <pr-number> --body "..."`

### Step 5: Link Issue to PR

The `Closes #<issue-number>` in the PR body automatically links the issue.

Additionally, add the issue as a linked reference:
```bash
gh pr edit <pr-number> --add-label "closes-issue"
```

If the repo doesn't have that label, skip this step (don't fail).

### Step 6: Complete Tasks

1. **On the Issue:** Add a comment and close it:
```bash
gh issue comment <issue-number> --body "Completed in PR #<pr-number>"
gh issue close <issue-number>
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
