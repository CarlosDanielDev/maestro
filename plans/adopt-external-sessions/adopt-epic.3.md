Parent: #1051

maestro's TUI has no view of sessions it did not launch. The landing screen dispatches menu items to `TuiMode` variants (`src/tui/screens/landing/types.rs:31-41`, key handler `src/tui/screens/landing/mod.rs:181`), list-and-select UI patterns exist twice — issue browser with filter + multi-select + launch overlay (`src/tui/screens/issue_browser/mod.rs:186`, overlay at `:58`, `ScreenAction::LaunchSession` at `:333`) and the session switcher (`src/tui/session_switcher.rs`) — and adopted sessions would land in the Interaction screen (`src/tui/screens/interaction/`, history renders `Vec<TurnRecord>`). The scan they would call lands with #1052; the adopt call with #1053. Council verdict (2026-10-14, unanimous): TUI-only entry, global scope, live sessions warned not blocked.

## Done when

- [ ] a landing menu item opens a new picker screen listing `DiscoveredSession`s from `scan_transcripts` sorted by last-active desc, each row showing repo (from cwd), title, relative last-active time — insta snapshot test renders the screen with a fixed fixture list — owner: implementer
- [ ] rows whose transcript mtime is inside the live threshold show a visible live/active badge (the #1053 warning surfaced as UI), and selecting one asks for confirmation before adopting; stale rows adopt without prompt — `picker_live_row_confirms_before_adopt` (snapshot + integration) — owner: implementer
- [ ] selecting a session creates the adopted session in the pool (id, cwd, no worktree) and routes to the Interaction screen — integration test in `src/integration_tests/` asserts pool state and `TuiMode::Interaction` — owner: implementer
- [ ] the picker filters as you type (same interaction as the session switcher's `filter_text`) and handles an empty result (no transcripts on the machine) with an empty-state message — snapshot tests cover both — owner: implementer
- [ ] the screen respects the 400-line cap — `scripts/check-file-size.sh` passes; screen state slot + `TuiMode` variant follow the existing pattern (`src/tui/app/types.rs:15-34`, `:228-236`) — owner: implementer

## ADR stub

New `TuiMode` variant + screen module (copy `issue_browser` structure) versus reusing the session switcher overlay with a second data source (rejected — the switcher indexes maestro's own pool, adoption needs cwd/title/recency columns the switcher does not have; overloading it forks its semantics). Recommendation (to argue with): new screen, minimal: list + filter + select + confirm-on-live. Empty-state before error-state: no `~/.claude` at all is a normal first-run condition.

## Out of scope

- CLI verb `maestro adopt` (council Q4a: deferred — the scan is a library function so the verb is cheap when asked; kill criterion — a request within one release cycle reopens Q4).
- Remote/SSH machine discovery, scheduled re-scan, background indexing.
- Deleting or archiving external transcripts from maestro.

## Handoff

```bash
git worktree add -b story/1054-adopt-picker ../maestro-1054 origin/main
```

`codegraph explore "LandingMenuItem TuiMode SessionSwitcher filtered_sessions"` and read `src/tui/screens/landing/types.rs`, `src/tui/screens/issue_browser/mod.rs`, `src/tui/screens/interaction/mod.rs`. TDD: snapshots are the spec — write the fixture-list snapshot RED before the screen exists. Gate: `cargo fmt --check && cargo clippy && cargo test && bash scripts/check-file-size.sh`. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes 1054`.
