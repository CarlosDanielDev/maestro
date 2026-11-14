---
name: tui-design-system
version: "1.0.0"
description: Maestro TUI design system — theme tokens, reusable widgets, screen contract and wiring, global keybindings, UX and snapshot-test rules. Use before ANY change under src/tui/ (new screen, widget, panel, keybind, color, text input) so you reuse what exists instead of rebuilding it.
allowed-tools: Read, Grep, Glob
---

# TUI Design System

Full reference: `docs/tui-design-system.md`. Read it before writing TUI code.

Five rules that prevent most rework:

1. **Colors only from `&Theme`** (`src/tui/theme.rs`). Panels via `theme.styled_block(title, focused)`. No raw `Color::`.
2. **Reuse widgets** in `src/tui/widgets/` (`Toggle`, `TextInput`, `NumberStepper`, `Dropdown`, `ListEditor`, `EmptyState`, `WizardFrame`, …) before building a new one.
3. **Text input = canonical `Input`** (`src/tui/widgets/input/`) or `tui_textarea` + `screens::wrap`. Never a hand-rolled `String` + cursor.
4. **New screen = 7 wiring sites.** `ui.rs` draw arm and `snapshot_tests/mod.rs` registration fail silently.
5. **Global keys win** (`handle_global_shortcuts` in `src/tui/input_handler.rs`): `Ctrl+Q`, `Ctrl+B`, `Ctrl+X`, `F1`–`F12`, `?`, `Shift+Q/M/C/P`. Don't rebind.

If you add a widget, token, or helper, add it to `docs/tui-design-system.md` in the same PR.
