Parent: #1063

maestro tracks spend (`cost_usd` per session, persisted) but not provider quota windows — the operational limit that actually interrupts sessions. The pattern is proven by akitaonrails' `ai-usagebar`: Claude windows come from `GET https://api.anthropic.com/api/oauth/usage` answered as `{utilization, resets_at}` per window (`five_hour`, `seven_day`; `src/anthropic/fetch.rs:21` and `src/anthropic/types.rs:19` in that repo, OAuth token read read-only from the credential file the official CLI already wrote). Its cache discipline: per-vendor directory, **flock**'d, TTL 60s, **HTTP 429 → vendor left alone 5 minutes**, last-good snapshot shown marked stale (`src/cache.rs`). maestro already abstracts providers (`src/agent_provider/types.rs:255`) and persists session state — this slice adds the window feed.

## Done when

- [ ] a quota module exposes `fn windows(provider_id) -> Option<ProviderWindows>` where `ProviderWindows` carries per-window `{utilization_pct: f64, resets_at: DateTime<Utc>}` — Claude Code implemented first via the OAuth credential file read read-only (never prompt, never store the token beyond process life) — unit test with a captured response fixture — owner: implementer
- [ ] the cache follows the proven discipline: per-provider file lock, TTL 60s, on HTTP 429 the provider is not polled for 5 minutes and `windows()` returns the stale snapshot flagged `stale: true` — unit tests cover fresh, stale, and locked paths — owner: implementer
- [ ] zero network in tests: the module takes an HTTP client trait; tests inject a fake — `grep -rn "reqwest\|ureq" src/**/*quota*` shows no direct client construction outside the trait impl — owner: implementer
- [ ] providers without a quota source (Ollama, self-hosted) return `None` cleanly and the UI can render "no data" — never 0% — unit test — owner: implementer

## ADR stub

Token acquisition: (a) read-only parse of the official CLI's credential file (ai-usagebar's mode 2 — zero user setup); (b) ask the user for an API key in config (mode 1 — more setup, more providers); (c) both, keyed per provider. Recommendation (to argue with): (c) with (a) for Claude Code and (b) as the extension point — the `AuthKind` split in ai-usagebar's `src/vendor.rs` is the checklist. Endpoint scraping beyond official surfaces (Cursor, Devin, etc.) stays out until a user asks.

## Out of scope

- Routing (#1065), UI badges, alerts — later slices.
- Providers beyond Claude Code in this slice.

## Handoff

```bash
git worktree add -b story/1064-quota-fetch ../maestro-1064 origin/main
```

`codegraph explore "provider_for_agent cost"` and read `src/agent_provider/types.rs`, `src/state/store.rs`. Gate: the `.wave.json` gate. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes 1064`.
