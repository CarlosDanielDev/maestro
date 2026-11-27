Parent: #1066

maestro's session switcher (`src/tui/session_switcher.rs`) indexes only the current repo's pool. The machine-wide data already exists: `scan_transcripts` (`src/session/discovery.rs`, merged via #1056) lists every external Claude Code transcript with cwd, title, last-active, and a `skipped_lines` integrity count. This slice builds the fleet board on top of it — all sessions, all repos, one screen — borrowing `ghpending`'s two sort heuristics that map directly: stale-first ("most neglected session at the top") and activity-desc (default).

## Done when

- [ ] a fleet board screen lists, in one table: maestro pool sessions (current repo, from `maestro-state.json`) AND discovered external transcripts (machine-wide, from `scan_transcripts`), with columns repo (cwd), session name/title, kind (managed/external/adopted), status or last-active — insta snapshot with a fixed fixture set covering all three kinds — owner: implementer
- [ ] two sorts, key-toggled: last-active desc (default) and stale-first (least-recently-active at top) — `ghpending`'s `activity` and `stale` heuristics (`src/sort.rs` there); unit test pins both orders — owner: implementer
- [ ] filter-as-you-type over repo and title (same interaction as the session switcher's `filter_text`) — snapshot test — owner: implementer
- [ ] the scan runs on board open, not on a timer: a fresh `scan_transcripts` call per open, bounded by the 1s budget already pinned in `discovery_tests.rs` — owner: implementer
- [ ] adopted sessions and pool sessions deep-link: selecting a managed/adopted row navigates to its existing screen (session view / Interaction), external rows show the adopt action only if #1068 landed, else a hint — `fleet_rows_route_to_existing_screens` integration test — owner: implementer

## ADR stub

One screen, two data sources: (a) merge pool + scan rows in one model (this issue — the board is the product); (b) two tabs (rejected — the point is one glance); (c) poll in background (rejected — scan-on-open respects the 1s budget, no timers). Recommendation (to argue with): (a). Rows from other repos' `maestro-state.json` are NOT read — only transcripts; cross-repo maestro state is a different epic.

## Out of scope

- Reading other repos' `maestro-state.json` (their pools stay theirs).
- Open PR/issue digests per repo — `ghpending` already does it externally; a maestro digest is its own future issue if asked.
- Live process detection (`ps` sweep) — mtime recency stays the liveness signal.

## Handoff

```bash
git worktree add -b story/1067-fleet-board ../maestro-1067 origin/main
```

`codegraph explore "scan_transcripts SessionSwitcher filtered_sessions"` and read `src/session/discovery.rs`, `src/tui/session_switcher.rs`, `src/tui/screens/landing/types.rs`. Blocked by #1054 (picker/screen patterns land there first). Gate: the `.wave.json` gate. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes 1067`.
