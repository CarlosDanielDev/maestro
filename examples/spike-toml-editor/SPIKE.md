# SPIKE: Schema-Driven TOML Editor TUI

**Issue:** #711
**Outcome:** **GO** for v0.29.0
**Effort estimate (L0 + L1):** 5–7 person-days
**Date:** 2026-05-18

This document captures empirical findings from the throwaway spike at
`examples/spike-toml-editor/`. The crate is a standalone Cargo workspace
(empty `[workspace]` table) so it does not inherit the host crate's lint
configuration. `unwrap()`/`expect()` are intentional inside the spike.

Run the data-layer tests:

```
cd examples/spike-toml-editor
cargo test
```

Run the TUI (uses the bundled fixture by default):

```
cd examples/spike-toml-editor
cargo run
```

Keybindings: `q` quit · `s` save · `tab/h/l` switch tab · `↑/↓/j/k` move
field · `enter` edit · `esc` cancel · in Enum edits, `↑/↓` cycle options ·
in Bool edits, `space` toggles.

---

## a) What worked well

- **`toml_edit::DocumentMut` is the right tool.** Comments, blank lines,
  inline arrays, and the `permission_mode` trailing comment all survived
  the round-trip across 14 automated tests.
- **`FieldSchema` enum + `Schema` struct** modeled `[project]`, `[sessions]`,
  and `[tui]` cleanly. Adding a field is one entry in a `&[FieldSchema]`
  const slice (see `src/schema.rs`).
- **ratatui rendering split** (`Tabs`, `List`, `Paragraph` + a modal
  `Clear`-then-popup overlay) was straightforward. The `widgets.rs` module
  is pure (`&ViewState -> Frame`); state lives in `main.rs::App`.
- **`EditedValue` boundary** (Bool/Int/Str) kept the UI layer ignorant of
  `toml_edit` internals. The state machine cycles through `EditBuffer`
  variants that map 1:1 onto `EditedValue` at commit time.
- **Standalone workspace trick:** an empty `[workspace]` table in the
  spike's `Cargo.toml` declares it as its own workspace root, so host
  lints, MSRV pins, and `[workspace.dependencies]` do not reach the spike.
  Host `cargo test` is unaffected (4603 + 4766 + … tests still pass).
- **TDD seam** was extremely clean: write tests → watch them fail to
  compile (RED) → implement `schema.rs` → watch them pass (GREEN). 14/14
  in < 50 ms.

## b) What was harder than expected

- **The `Item`/`Value`/`Decor` distinction is unintuitive.** Indexing into
  a `DocumentMut` gives you `Item`, but only `Value` carries decor. You
  must `.as_value()` / `.as_value_mut()` before reaching `.decor()`. See
  gotcha (c) below.
- **`Decor::prefix()` returns `Option<&RawString>`, not `Option<&str>`**.
  Setting it back requires `.as_str()` and a `.to_string()` round-trip
  because `RawString` does not implement `Clone` into anything the setter
  consumes directly. The spike uses:
  ```rust
  if let Some(p) = prefix {
      if let Some(prefix_str) = p.as_str() {
          v.decor_mut().set_prefix(prefix_str.to_string());
      }
  }
  ```
  This is verbose; production code should encapsulate it in a helper.
- **`ratatui` 0.28 deprecated `Frame::size()` in favor of `Frame::area()`.**
  Worth checking the latest version before promoting to production.
- **No surprises in `crossterm` 0.28** event handling, but `BackTab` only
  fires with `Shift+Tab` on most terminals; provide `h`/`l` as fallbacks.

## c) `toml_edit` gotchas

### Item vs Value vs Decor

`doc["sessions"]["permission_mode"]` returns `&mut Item`. `Item` may be
`Value`, `Table`, `ArrayOfTables`, or `None`. Decor lives on `Value` (and
on `Table` headers, but the spike does not edit those).

```rust
// Wrong: Item has no .decor()
let dec = doc["sessions"]["permission_mode"].decor(); // does not compile

// Right: drop into the Value first
if let Some(v) = doc["sessions"]["permission_mode"].as_value() {
    let suffix = v.decor().suffix();
}
```

### Decor preservation on leaf replacement

**Empirical answer:** assigning via `doc[t][k] = value(v)` does **not**
reliably preserve trailing inline comments. The spike adopts the
**snapshot-and-restore** pattern:

```rust
let (prefix, suffix) = match doc[table][key].as_value() {
    Some(v) => (v.decor().prefix().cloned(), v.decor().suffix().cloned()),
    None => (None, None),
};
doc[table][key] = value(new_scalar);
if let Some(v) = doc[table][key].as_value_mut() {
    if let Some(p) = prefix {
        if let Some(s) = p.as_str() { v.decor_mut().set_prefix(s.to_string()); }
    }
    if let Some(s) = suffix {
        if let Some(s2) = s.as_str() { v.decor_mut().set_suffix(s2.to_string()); }
    }
}
```

The canary test `trailing_comment_preserved_after_enum_edit` passes with
this implementation. Without it, the `# Options: …` comment on
`permission_mode` is dropped on save.

### `plugins = []` survives untouched

Editing an unrelated key in `[sessions]` does not perturb `plugins = []`
or its trailing `# Empty = no plugins loaded` comment. Verified by
`plugins_empty_array_and_comment_preserved`.

### Blank-line preservation

Blank lines between `[project]`, `[sessions]`, and `[tui]` survive any
edit. Verified by `blank_lines_between_sections_preserved`.

## d) Recommendation for v0.29.0

**GO.** The schema-driven editor is feasible with modest production hardening:

1. **Promote `schema.rs`** to `src/config/edit.rs` with `Result`-returning
   APIs (no `unwrap`/`expect`) and a typed error enum at the boundary per
   RUST-GUARDRAILS §2.
2. **Encapsulate decor snapshotting** into a single helper (`fn
   set_leaf_preserving_decor`) so the verbosity stays in one place.
3. **Auto-derive `Schema`** from the existing `Config` serde struct via a
   small `build.rs` or proc macro — manual schema definitions will sprawl
   as soon as nested tables (e.g. `[agents.claude]`) come into scope.
4. **Wire the editor into a settings TUI screen** in `src/tui/`. The
   spike's modal overlay pattern transplants cleanly.

Risks (all manageable):

- `toml_edit::RawString` → `String` round-trip for decor is moderately
  verbose; if `toml_edit` 0.23 changes the Decor API, the helper isolates
  the churn.
- Nested-table schema recursion is not yet exercised — covered by L2.

## e) Effort estimate per follow-up issue

| Level | Issue (proposed) | Estimate |
|---|---|---|
| L0 | `feat(config)`: `ConfigEditor` in `src/config/edit.rs` (typed errors, snapshot-restore helper, unit tests) | 1–2 days |
| L0 | `feat(config)`: full `maestro.toml` schema (all 11 tables matching `src/config.rs`) | 1 day |
| L1 | `feat(tui)`: settings screen (tab bar + field list + edit overlay), wired to `ConfigEditor` | 2–3 days |
| L1 | `feat(tui)`: validation feedback in edit overlay (Int out-of-bounds, Enum invalid value) | 1 day |
| L2 | `feat(tui)`: unsaved-changes indicator + save-on-quit prompt | 0.5 day |
| L2 | `feat(config)`: nested-table schema recursion (`[agents.claude]` etc.) | 1–2 days |
| L2 | `feat(tui)`: keyboard search (`/` filter) | 1 day |

**v0.29.0 milestone scope (L0 + L1):** 5–7 person-days.

### Dependency graph (for the milestone description)

```
Level 0 — no dependencies:
• L0a feat(config): ConfigEditor in src/config/edit.rs
• L0b feat(config): full maestro.toml schema definition

Level 1 — depends on L0a + L0b:
• L1a feat(tui): settings screen wired to ConfigEditor
• L1b feat(tui): validation feedback in edit overlay

Level 2 — depends on L1a (and L0a for L2b):
• L2a feat(tui): unsaved-changes indicator
• L2b feat(config): nested-table schema recursion
• L2c feat(tui): keyboard search

Sequence: L0a ∥ L0b → L1a → L1b ∥ L2a ∥ L2c
                          → L2b (from L0a)
```

## f) Architectural decisions the milestone should reconsider

1. **Use `DocumentMut` for the full config lifecycle**, not only on edit-
   write. The current `src/config.rs` parses via `serde + toml` (lossy
   round-trip). Switching the loader to `toml_edit::DocumentMut` allows
   hot-reload and edit-preserving saves without two parsers in flight.
   This is non-trivial — `serde` derives are convenient — so call it out
   in the L0 issue.
2. **Derive `Schema` from `Config`** via a macro. Hand-maintained schemas
   diverge from the real struct over time; the spike's three hard-coded
   tables already feel fragile.
3. **Boundary error type:** introduce `ConfigEditError { LoadFailed,
   SaveFailed, InvalidValue, UnknownField }` so the TUI can render
   actionable messages instead of `anyhow::Error::to_string()`.
4. **Decor helper as a shared seam.** The snapshot-restore pattern wants
   to live alongside `edit_value` so future contributors do not write
   `doc[t][k] = value(v)` and silently lose comments.

---

## Security checklist for the L0 promotion

The spike is local-only and user-chosen-path, so the items below are not
blockers here. They MUST be in the production-promotion PR:

1. **Restrict the path argument:** canonicalize, allow-list the filename
   (`maestro.toml`), reject symlinks, jail to project root or
   `$XDG_CONFIG_HOME/maestro/`. The spike accepts any `argv[1]` and will
   overwrite any file the user can write.
2. **Atomic write:** write to a sibling tempfile + `rename` rather than
   `fs::write` directly, so a crashed save cannot truncate the user's
   config.
3. **Validate schema-vs-document shape before mutation.** A hand-edited
   or corrupted `maestro.toml` missing a table the schema expects will
   panic inside `toml_edit` when `edit_value` indexes the missing parent.
   Surface `ConfigEditError::UnknownField` / `MissingTable` at the seam.
4. **Run `cargo audit` / `cargo deny`** once the crate moves into the
   host workspace; align `toml_edit` minor with whatever the host pins.
5. **Default `permission_mode` in `src/config.rs` stays `"default"`.**
   The fixture uses `"bypassPermissions"` for testing; do not copy the
   fixture into production defaults.

## Out of scope (per issue)

- Polished error messages, undo, search, multi-tab persistence, mouse,
  theming.
- Wiring to existing widgets in `crate::tui::widgets::*`.
- Any production-code change in `src/`.
