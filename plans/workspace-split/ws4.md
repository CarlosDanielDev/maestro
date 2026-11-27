# Story — Move the TUI into the maestro-tui crate

Parent: #1302

`src/tui/` is 100,652 lines (`wc -l`, measured 2026-11-14) — widgets, 30+
screens, app shell, and the three god files (`src/tui/input_handler.rs:1`,
2,879 lines; `src/tui/screen_dispatch.rs:1`, 2,024; `src/tui/ui.rs:1`, 2,087).
Move all of it into `crates/maestro-tui` so it can only reach core through
the #1303 API plus ratatui/crossterm. The god files move as-is — their
surgery is the TUI Redesign milestone, not this epic.

## Done when

- [ ] zero TUI code outside `crates/maestro-tui/src/` — `grep -rn "ratatui" src/ crates/maestro-core crates/maestro-cli` clean — owner: implementer
- [ ] screens reach core only through the #1303 API — `cargo tree -p maestro-tui -e normal --depth 1` shows `maestro-core`, `ratatui`, `crossterm` and nothing else internal — owner: reviewer
- [ ] `cargo test -p maestro-tui` green; pending-snapshots gate (`.wave.json`) clean — owner: implementer
- [ ] `docs/tui-design-system.md` paths updated where the move changed them (`tests/tui_design_system.rs:1` fails CI on stale paths) — owner: implementer

## ADR stub

Move as-is vs split god files during the move. Recommend as-is: a pure
location change keeps `git blame` and review honest; mixing relocation with
refactor produces an unreviewable diff and hides regressions behind moves.
Dissent welcome.

## Out of scope

- Deleting, merging, or redesigning screens — TUI Redesign milestone.
- Changing widgets, theme tokens, or the design-system doc's rules.
- Behavior changes of any kind.

## Handoff

- `git worktree add -b feat/1306-tui-crate ../maestro-1306 origin/main`
- `codegraph explore "tui screens, widgets, input_handler, screen_dispatch"`
- Gate: `.wave.json`; `Closes #1306`.
