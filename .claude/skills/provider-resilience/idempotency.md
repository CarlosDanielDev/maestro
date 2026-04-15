# Idempotency Patterns

## Principle

Every Maestro operation that creates remote resources MUST be idempotent — running it twice produces the same result without errors or duplicates.

## Pattern: Check-Then-Create

```rust
async fn ensure_resource(client: &Client, name: &str) -> Result<u64> {
    // 1. Try to find existing
    if let Some(existing) = client.find_by_name(name).await? {
        tracing::info!("{} already exists (#{}), reusing", name, existing.number);
        return Ok(existing.number);
    }
    // 2. Create if not found
    client.create(name).await
}
```

**When to use:** Before creating milestones, labels, or other uniquely-named resources.

## Pattern: Create-Then-Recover

```rust
async fn create_or_reuse(client: &Client, name: &str) -> Result<u64> {
    match client.create(name).await {
        Ok(number) => Ok(number),
        Err(e) if is_duplicate_error(&e) => {
            // Already exists — find and return it
            client.find_by_name(name).await?
                .ok_or_else(|| anyhow!("Resource '{}' reported as duplicate but not found", name))
        }
        Err(e) => Err(e),
    }
}
```

**When to use:** When the "find" call is expensive or when creation is the common path (first run).

## Maestro Components That Need Idempotency

| Component | Resource | Current State | Pattern to Use |
|-----------|----------|---------------|----------------|
| `adapt/materializer.rs` | Milestones | NOT idempotent (crashes on dup) | Create-Then-Recover |
| `adapt/materializer.rs` | Issues | NOT idempotent | Check-Then-Create (by title) |
| `adapt/materializer.rs` | Labels | NOT handled | Ensure-Before-Use |
| `github/client.rs` | PR creation | Idempotent (checks existing) | OK |
| `tui/app/issue_completion.rs` | Issue close | Idempotent (close is no-op if closed) | OK |
| `tui/app/issue_completion.rs` | PR creation | Has retry queue | OK |

## Label Idempotency

Labels are the most common 422 source. Standard label sets for Maestro-generated issues:

```rust
const STANDARD_LABELS: &[(&str, &str)] = &[
    ("type:feature", "1D76DB"),
    ("type:bug", "D93F0B"),
    ("type:docs", "0075CA"),
    ("type:chore", "EDEDED"),
    ("priority:P0", "B60205"),
    ("priority:P1", "D93F0B"),
    ("priority:P2", "FBCA04"),
    ("maestro:ready", "0E8A16"),
    ("maestro:in-progress", "F9D0C4"),
    ("maestro:done", "0E8A16"),
    ("maestro:failed", "D93F0B"),
];
```

Before materializing, ensure all labels used in the plan exist:
```rust
async fn ensure_plan_labels(client: &dyn GitHubClient, plan: &AdaptPlan) -> Result<()> {
    let needed: HashSet<String> = plan.milestones.iter()
        .flat_map(|m| &m.issues)
        .flat_map(|i| &i.labels)
        .cloned()
        .collect();
    
    let existing = client.list_labels().await?;
    let existing_names: HashSet<&str> = existing.iter().map(|l| l.as_str()).collect();
    
    for label in &needed {
        if !existing_names.contains(label.as_str()) {
            let color = STANDARD_LABELS.iter()
                .find(|(name, _)| *name == label.as_str())
                .map(|(_, color)| *color)
                .unwrap_or("EDEDED"); // default gray
            client.create_label(label, color).await?;
        }
    }
    Ok(())
}
```

## Testing Idempotency

Every materializer/creator test should include:
1. **First run:** succeeds, creates resources
2. **Second run (same input):** succeeds, reuses existing resources
3. **Partial failure recovery:** first run fails mid-way, second run completes from where it left off
