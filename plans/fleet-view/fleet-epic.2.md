Parent: #1066

The fleet board (#1067) shows everything; this slice makes rows actionable — the board becomes the front door: see a neglected external session, adopt it in place; see a managed session, jump to it. The adopt pipeline itself is #1053 (in flight) and #1054 (picker + adopt action); this slice reuses those calls from the board instead of duplicating them.

## Done when

- [ ] selecting an external row on the fleet board invokes the same adopt path #1054 built (warn-on-live confirmation included) and routes to the Interaction screen — `fleet_adopt_reuses_picker_pipeline` integration test asserts pool state and `TuiMode::Interaction` (action vocabulary: `src/tui/screens/mod.rs:120`) — owner: implementer
- [ ] selecting a managed/adopted row navigates to that session's existing screen without re-creating it — `fleet_jump_does_not_recreate_session` integration test — owner: implementer
- [ ] a board footer lists the keymap (adopt / jump / sort toggle / filter) in the DOS-style F-key bar convention — `fleet_board_footer_keymap` snapshot test — owner: implementer

## ADR stub

Action wiring: (a) the board emits the same `ScreenAction` variants #1054 introduced for its picker (zero new actions); (b) board-specific actions (rejected — two entry points to one pipeline must share the action vocabulary or drift). Recommendation (to argue with): (a).

## Out of scope

- New adopt semantics — anything the pipeline (#1053) and picker (#1054) do not already do.
- Kill/pause of foreign sessions from the board.

## Handoff

```bash
git worktree add -b story/1068-fleet-actions ../maestro-1068 origin/main
```

`codegraph explore "ScreenAction LaunchSession open_interaction_session"` and read `src/tui/screen_dispatch.rs:455`. Blocked by #1067 and #1054. Gate: the `.wave.json` gate. Skills: project-patterns, superpowers:test-driven-development. No AI attribution; none of the forbidden git commands. `Closes 1068`.
