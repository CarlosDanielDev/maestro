# GitHub CLI (`gh`) Patterns

## Command Reference for Maestro

### Issues

```bash
# Create (with labels and milestone)
gh issue create --title "feat: X" --body "..." --label "type:feature,priority:P1" --milestone 3

# List by label
gh issue list --label "maestro:ready" --json number,title,body,labels,state

# Close
gh issue close 123

# Comment
gh issue comment 123 --body "Completed in PR #456"

# View
gh issue view 123 --json title,body,labels,milestone,state,comments
```

### Milestones

```bash
# Create
gh api repos/OWNER/REPO/milestones -X POST -f title="v1.0" -f description="..."

# List
gh api repos/OWNER/REPO/milestones --jq '.[].title'

# Update
gh api repos/OWNER/REPO/milestones/3 -X PATCH -f description="..."

# Find by title (useful for idempotency)
gh api repos/OWNER/REPO/milestones --jq '.[] | select(.title=="v1.0") | .number'
```

### Labels

```bash
# List all
gh label list --json name,color --limit 100

# Create
gh label create "type:feature" --color "1D76DB" --description "Feature request"

# Check if exists (exit code 0 = exists)
gh label list --json name --jq '.[].name' | grep -q "^type:feature$"
```

### Pull Requests

```bash
# Create
gh pr create --title "feat: X" --body "..." --base main

# View
gh pr view 123 --json number,title,body,state

# Check if PR exists for branch
gh pr view --json number 2>/dev/null

# Review
gh pr review 123 --comment --body "LGTM"
```

## Common Pitfalls in Maestro

### 1. Labels Must Exist Before Use
`gh issue create --label "nonexistent"` → HTTP 422. Always ensure labels exist first.

### 2. Milestone Must Be Referenced by Number, Not Title
`gh issue create --milestone "v1.0"` works but is fragile. Prefer `--milestone 3` (the number).

### 3. Body Length Limits
GitHub issue/PR body limit is ~65535 chars. Claude can generate plans exceeding this.
Truncate with: `body.chars().take(60000).collect::<String>() + "\n\n(truncated)"`

### 4. Rate Limits
- Authenticated: 5000 requests/hour
- `gh api` includes rate limit headers
- Batch operations (materializer creating 20 issues) should check remaining quota

### 5. Auth Errors Are Silent
`gh` exits with code 1 for both auth errors and other failures. Check stderr for:
- "not logged in"
- "authentication required"  
- "http 401"

Maestro's `is_gh_auth_error()` in `src/github/client.rs` handles this.

## Defensive Wrapper Pattern

```rust
/// Run a gh command with retry and error classification.
async fn run_gh_defensive(args: &[&str], max_retries: u32) -> Result<String> {
    let mut attempts = 0;
    loop {
        match run_gh(args).await {
            Ok(output) => return Ok(output),
            Err(e) => {
                attempts += 1;
                if is_auth_error(&e) {
                    return Err(e); // Don't retry auth errors
                }
                if is_rate_limit_error(&e) && attempts <= max_retries {
                    tokio::time::sleep(Duration::from_secs(2u64.pow(attempts))).await;
                    continue;
                }
                if attempts > max_retries {
                    return Err(e);
                }
                // Server errors: retry
                if is_server_error(&e) {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
                return Err(e); // Client errors: don't retry
            }
        }
    }
}
```
