Parent: #1063

With windows flowing (#1064), maestro's pool can stop launching sessions into a provider that is about to die. ai-usagebar's pacing math — elapsed-pct through the window vs actual-usage-pct, with a tolerance band — distinguishes "on pace" from "burning ahead" (`src/pacing.rs` in that repo); its alert discipline fires once per crossing with hysteresis (re-arm only below threshold − 7 points, `src/notify.rs`). maestro adds the decision those tools can't make: it *has* multiple providers to choose from.

## Done when

- [ ] a pacing function classifies each window `on_track | ahead | critical` (actual vs elapsed, 5-point tolerance band) — unit table test over synthetic windows — owner: implementer
- [ ] when a session launch specifies no explicit provider, the pool picks the configured candidate with the most window headroom (highest `resets_at` margin among `on_track` providers); explicit provider choice always wins — integration test with injected fake quota data — owner: implementer
- [ ] the session row / dashboard shows a quota badge per provider (glyph + resets-in, pacing glyph `↑→↓` when known, "no data" when not) — insta snapshot — owner: implementer
- [ ] an alert fires once when a window crosses 97% (hysteresis: re-arms below 90), never repeatedly — `quota_alert_fires_once_with_hysteresis` unit test over the decide function (evidence for the pipeline it feeds: `src/session/pool.rs:483`) — owner: implementer
- [ ] opt-in config `[quota] enabled = false` default-off for v1; off means zero network — integration test asserts no requests when disabled — owner: implementer

## ADR stub

Default provider selection: (a) automatic headroom routing when unset (this issue); (b) warn-only (suggest, never choose); (c) do nothing. Recommendation (to argue with): (a) — it is the reason the feature exists; (b) is the safe middle if the first cut misrouts. Multi-window weighting (5h vs weekly) starts simple: the binding window is the one with less remaining absolute headroom.

## Out of scope

- Providers beyond Claude Code (fetch extension per provider = follow-up slices of #1064's module).
- Migrating a running session mid-flight when quota dies (pause + reroute = separate issue if it hurts).
- The `maestro cost` report shape.

## Handoff

```bash
git worktree add -b story/1065-quota-routing ../maestro-1065 origin/main
```

`codegraph explore "spawn SessionPool pick"` and read `src/session/pool.rs`, `src/tui/screens/home/mod.rs`. Blocked by #1064. Gate: the `.wave.json` gate. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes 1065`.
