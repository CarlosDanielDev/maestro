# Rate Limiting and Batching

## Rate Limits by Provider

| Provider | Limit | Header | Reset |
|----------|-------|--------|-------|
| GitHub (authenticated) | 5000 req/hr | `X-RateLimit-Remaining` | `X-RateLimit-Reset` (epoch) |
| GitHub (unauthenticated) | 60 req/hr | Same headers | Same |
| Azure DevOps | Tier-dependent | `Retry-After` | Varies |

## Batch Operations in Maestro

The adapt materializer is the heaviest API consumer. Creating a plan with 3 milestones and 15 issues requires ~18+ API calls:

```
3 × create_milestone     =  3 calls
15 × ensure_labels        = 15 calls (worst case, 1 per unique label)
15 × create_issue         = 15 calls
1 × create_tech_debt_issue = 1 call
                           ─────────
                           34 calls total
```

At 5000/hr this is fine, but for large plans (50+ issues) or rapid re-runs, we should be defensive.

## Backoff Strategy

```rust
const MAX_RETRIES: u32 = 3;
const BASE_DELAY_MS: u64 = 1000;

async fn with_backoff<F, T>(operation: F) -> Result<T>
where
    F: Fn() -> Pin<Box<dyn Future<Output = Result<T>>>>,
{
    for attempt in 0..=MAX_RETRIES {
        match operation().await {
            Ok(val) => return Ok(val),
            Err(e) if is_rate_limit_error(&e) && attempt < MAX_RETRIES => {
                let delay = BASE_DELAY_MS * 2u64.pow(attempt);
                tracing::warn!("Rate limited, retrying in {}ms (attempt {}/{})", delay, attempt + 1, MAX_RETRIES);
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}
```

## Batching Best Practices

1. **Collect all labels first, create in one pass** — don't check-create per issue
2. **Create milestones before issues** — issues reference milestone numbers
3. **Add a small delay between API calls** in batch operations (50ms) to avoid burst throttling
4. **Log progress** — `Creating issue 7/15...` so the user knows it's working
5. **Support partial completion** — if issue #8 fails, don't lose issues #1-7
