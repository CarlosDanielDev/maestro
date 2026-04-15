# Error Taxonomy and Recovery

## GitHub CLI (`gh`) Error Codes

| HTTP Code | Meaning | Common Cause | Recovery Strategy |
|-----------|---------|-------------|-------------------|
| **401** | Unauthorized | Token expired, `gh auth` needed | Detect via `is_gh_auth_error()`, set `gh_auth_ok = false`, prompt user |
| **403** | Forbidden | Rate limit exceeded OR insufficient permissions | Check `X-RateLimit-Remaining` header; if 0, wait for reset; if permissions, warn user |
| **404** | Not Found | Repo, milestone, or issue doesn't exist | Verify resource exists before referencing; create if missing |
| **422** | Validation Failed | Duplicate resource, invalid labels, body too long | **Most common in Maestro** — see below |
| **502/503** | Server Error | GitHub outage | Retry with exponential backoff (max 3 attempts) |

## HTTP 422 — The Most Dangerous Error

422 means "I understood your request but can't process it." Common triggers in Maestro:

### 1. Non-existent Labels
```
gh issue create --label "type:feature"
→ 422 if "type:feature" label doesn't exist on the repo
```
**Fix:** Always `ensure_labels_exist()` before creating issues:
```rust
async fn ensure_labels_exist(client: &dyn GitHubClient, labels: &[String]) -> Vec<String> {
    let existing = client.list_labels().await.unwrap_or_default();
    let existing_names: HashSet<&str> = existing.iter().map(|l| l.name.as_str()).collect();
    
    for label in labels {
        if !existing_names.contains(label.as_str()) {
            // Create with default color, or skip with warning
            if let Err(e) = client.create_label(label, &default_color(label)).await {
                tracing::warn!("Could not create label '{}': {}", label, e);
            }
        }
    }
    labels.to_vec() // Return only labels that now exist
}
```

### 2. Duplicate Milestones
```
gh api repos/OWNER/REPO/milestones -X POST -f title="v1.0"
→ 422 if milestone "v1.0" already exists
```
**Fix:** Check-then-create or catch-then-find:
```rust
async fn ensure_milestone(client: &dyn GitHubClient, title: &str, desc: &str) -> Result<u64> {
    match client.create_milestone(title, desc).await {
        Ok(number) => Ok(number),
        Err(e) if is_duplicate_error(&e) => {
            // Milestone exists — find and reuse it
            client.find_milestone_by_title(title).await
        }
        Err(e) => Err(e),
    }
}
```

### 3. Issue Body Too Long
GitHub has a ~65535 character limit on issue bodies. Claude-generated plans can exceed this.
**Fix:** Truncate body with a "See full details in..." link, or split into multiple issues.

### 4. Invalid Milestone Reference
```
gh issue create --milestone 999
→ 422 if milestone #999 doesn't exist
```
**Fix:** Verify milestone exists before referencing. Use the number returned from `create_milestone`, not a hardcoded value.

## Azure DevOps (`az`) Error Patterns

| Error | Meaning | Recovery |
|-------|---------|----------|
| `TF401019` | Item already exists | Check-then-create pattern |
| `TF400813` | Field validation error | Validate fields before API call |
| `VS403403` | Rate limit exceeded | Backoff and retry |
| `TF401349` | Permission denied | Warn user, skip operation |

## Error Detection Pattern

```rust
fn is_duplicate_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("already exists")
        || msg.contains("validation failed")
        || msg.contains("422")
        || msg.contains("tf401019") // Azure DevOps duplicate
}

fn is_rate_limit_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("rate limit")
        || msg.contains("403")
        || msg.contains("retry-after")
        || msg.contains("vs403403") // Azure DevOps rate limit
}
```

## Golden Rule

**Every `gh`/`az` call that creates a resource MUST handle the "already exists" case.** The materializer, PR creator, label manager, and CI poller all need this. Never `?`-propagate a creation call without considering duplicates.
