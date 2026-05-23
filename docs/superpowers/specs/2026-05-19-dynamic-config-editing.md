---
title: Dynamic-Key and Array-of-Tables Settings Editing — Design
date: 2026-05-19
status: Implemented (v0.29.0 + v0.29.5)
author: Carlos Daniel
issue: #719
milestone: v0.29.0
---

# Dynamic-Key and Array-of-Tables Settings Editing — Design

## 1. Summary

v0.29.0 made every **static-shape** config table schema-driven (#714 / #715 /
#716) and locked the schema as the single source of truth for the TUI
renderer and `docs/configuration.md` (#717). What it explicitly did **not**
solve is the **dynamic-shape** half of `maestro.toml`:

- `[agents.<id>]` — user-keyed map of agent providers.
- `[modes.<id>]` — user-keyed map of session modes.
- `[teams.<id>]` — user-keyed map of orchestration team presets.
- `[[sessions.completion_gates.commands]]` — ordered array-of-tables of
  shell-command gates.

These sections are first-class to the product (Multi-Agent v0.25.0 made
`[agents.*]` mandatory; orchestration teams are next in line; completion
gates are how `[sessions]` enforces pre-PR quality), but today they are
editable only by hand-editing TOML. This spec defines the mental model,
the UX flows, and the schema extensions required to bring them into the
schema-driven renderer without compromising the comment-preserving
round-trip guarantee.

**This is a design issue.** No code lands under #719 itself; the spec
spawns follow-up implementation issues listed in §10.

## 2. Background

### What v0.29.0 already ships

| Component | Path | Role |
|---|---|---|
| Schema model | `src/config/schema.rs` | `FieldSchema`, `FieldKind`, `TableSchema`, flat `SCHEMA` array walked by `schema_for_config()`. |
| Field arrays | `src/config/schema/core.rs`, `extras.rs` | `const`-promoted `FieldSchema` slices per top-level table. |
| Schema renderer | `src/tui/screens/settings/schema_tab/build.rs` | `from_schema(table, config) -> Vec<SettingsField>`; recursively flattens `NestedTable`; maps `FieldKind` → `WidgetKind`. |
| Writeback | `src/tui/screens/settings/schema_tab/sync.rs` | `sync_to_config(table, fields, config)`; label-based widget lookup; `toml::Value` round-trip. |
| Tab integration | `src/tui/screens/settings/tabs/*.rs` | Each tab is `from_schema(schema_table("name"), config)`. |
| Comment-preserving save | `Config::save_into_str` in `src/config/mod.rs` | `toml_edit::DocumentMut` overlay; preserves comments, blank lines, key order, unknown sections. |

### What is excluded

- **Budget tab** (#785) — float-precision limitation, hand-coded for
  now, orthogonal to this work.
- **Flags tab** — feature-flag store, not a `maestro.toml` section.
- **Theme/Layout/Advanced multi-table sync** — already solved by
  `sync_multi_table` in `tabs/mod.rs`; this spec does not touch it.

### Why this is its own design pass

Static-shape sections share a property the renderer relies on: **the set
of widgets is known at compile time** from a `const` `FieldSchema`
array. Every label, every default, every validator is statically
addressable. Dynamic-shape sections invert that: the set of widgets
depends on what the user has typed in. Three concrete consequences:

1. **Widget identity is no longer just a label** — there can be two
   "command" fields if there are two completion-gate entries.
2. **The user must be able to add and remove rows**, not just edit
   them — a verb the schema renderer doesn't yet have.
3. **Order matters for arrays-of-tables** — `[[completion_gates.commands]]`
   runs in declaration order; reordering is a real edit.

These three properties together demand new `FieldKind` variants, new
widgets, new keybinds, and a new round-trip strategy that survives the
`toml::Value` ↔ `toml_edit::DocumentMut` overlay.

## 3. Locked design decisions

Six load-bearing decisions, taken now so the implementation issues don't
re-litigate them.

| # | Question | Pick | Rationale |
|---|---|---|---|
| Q1 | Mental model for `[agents.<id>]` — row or sub-tab? | **Sub-tab** for `agents` and `modes`; **row** for `teams` (lighter struct) and **row** for `completion_gates.commands`. | Agents have 16 fields per entry — a row collapses to unreadable scroll. Modes are smaller but bind to agents, so co-locating them as sub-tabs matches the user's mental flow. Teams already have a wizard (Orchestration spec, #2026-05-05); the Settings view for `[teams.*]` is the simpler read-then-tweak surface. |
| Q2 | Add-flow UX | **Modal name prompt + immediate jump to the new sub-tab/row in edit mode.** | "+ New" sentinel rows do not generalize across sub-tab and row layouts; a modal is universal. |
| Q3 | Remove-flow UX | **Confirm dialog with the section ID echoed; soft-delete via undo flash for 5 seconds.** | Two-step destructive ops match the rest of the TUI (`Ctrl+r` reset already shows confirm). The 5-second flash is cheap and avoids a "graveyard" UI. |
| Q4 | Rename-flow UX | **Not supported in v0.29.0.** Rename = delete + add (user copy-pastes). | `toml_edit` key renames are nontrivial: they break the comment-preservation invariant when the renamed key has trailing comments. Add the verb later if real demand surfaces. |
| Q5 | Reorder for array-of-tables | **`Alt+↑` / `Alt+↓` to move the focused row up/down.** | Matches `vim` `:m` muscle memory and avoids stealing plain arrows from field navigation. |
| Q6 | Schema extensions | Two new variants: **`FieldKind::Map { entry_fields }`** and **`FieldKind::VecOfStruct { entry_fields }`**. | Map = dynamic-key (`HashMap`/`BTreeMap` in Rust). VecOfStruct = array-of-tables (`Vec` in Rust). Both carry a child `&'static [FieldSchema]` describing one entry. |
| Q7 | Identifier validation rules | **Anchored regex `^[a-z0-9][a-z0-9_-]{0,62}$`** + deny-list of reserved ids (`default`, `entries`, `extends`, `kind`, `enabled`, plus any field name in the corresponding entry schema). Comparison is byte-exact post-regex; lowercase-only makes case-folding moot. | Avoids argv-landmine ids (`-rm`), shadowing of selector fields (`agents.default`), and length DoS. Strict subset of TOML bare-key syntax — no `toml_edit` escape needed. |
| Q8 | `extends` cycle handling for teams | **Cross-entry validator rejects self-reference and any cycle in `teams.<id>.extends`.** Tested with cycles of length 1 and 2 (§9). | Without this, naive resolver recurses → stack overflow / DoS-of-config. Same validator pass also checks `agents.default` references an existing key. |
| Q9 | Undo buffer for Remove | **Single-slot LIFO; second delete within 5s overwrites the first** (older deletion is gone from the buffer but the user can still abandon via `Ctrl+r` reset since neither has hit disk yet). The flash banner only ever shows the most recent delete. | Multi-slot buffer adds UI surface (which deletion is the user undoing?) without solving a real workflow. |

## 4. Mental model

### Two shapes, two interaction patterns

```
Static            Dynamic-key Map                Array-of-Tables
[project]         [agents.<id>]                  [[completion_gates.commands]]
 ├─ field         ├─ <id_1> (sub-tab)             ├─ [0] name/run/required (row)
 └─ field         │   ├─ field                    ├─ [1] name/run/required (row)
                  │   └─ field                    └─ [2] ...
                  └─ <id_2> (sub-tab)
                      ...

User verbs:       Add (modal) / Remove (confirm)  Add (modal) / Remove (confirm)
                  / Switch (Ctrl+→/←)              / Reorder (Alt+↑/↓)
                  / Edit (existing schema flow)    / Edit (existing schema flow)
```

### Why sub-tab vs row split

For each dynamic section we asked: *Does the user typically interact with
one entry at a time, or with the whole list at once?*

| Section | Per-entry field count | Cross-entry comparison common? | UX choice |
|---|---|---|---|
| `agents.<id>` | 16 | Rare — agents are configured once and edited per-entry | **Sub-tabs** (Ctrl+→/Ctrl+← switches between agent ids; main field list is the active agent's schema) |
| `modes.<id>` | 3 | Sometimes — comparing prompts across modes | **Sub-tabs**, but with a "+ Compare" overlay deferred to v0.30.0 |
| `teams.<id>` | ~5 typed + nested `role_overrides` map | Common — eyeballing role bindings across teams | **Row table** (collapsible to show `role_overrides` per row) |
| `completion_gates.commands[]` | 3 | Always — order matters by definition | **Row table** (ordered) |

The pattern that ties them together is the same widget primitive at the
schema layer: a `Map(...)` or `VecOfStruct(...)` `FieldKind`. The
sub-tab vs row distinction is a **renderer-side hint**, not a schema
concept — `FieldSchema` can carry an optional
`presentation: Presentation::Subtabs | Presentation::Rows` field.

## 5. UX flows

The mockups below are the canonical reference. Tabs widen to 80×24 (the
existing snapshot baseline). All keybinds are additive to the existing
settings keymap (`Tab` switches top-level tab, `↑/↓` moves field focus,
`Space/Enter` toggles or edits, `Ctrl+s` saves).

### 5.1 Agents tab — sub-tab list view (no entries yet)

```
╭ Settings ─ Project │ Sessions │ Budget │ GitHub │ Notif │ Gates │ Review │ Agents │ Modes │ Teams │ Theme │ Layout │ Flags │ TurboQuant │ Advanced ╮
│                                                                              │
│  [agents]                                                                    │
│                                                                              │
│  default: claude                                                             │
│                                                                              │
│  ┌─ Sub-tabs ─────────────────────────────────────────────────────────────┐  │
│  │                                                                        │  │
│  │   No agents configured.                                                │  │
│  │                                                                        │  │
│  │   Press [a] to add a new agent entry.                                  │  │
│  │   Built-in agents (claude, codex, qwen, opencode, ollama, minimax)     │  │
│  │   are auto-detected by `maestro doctor` but only appear here once      │  │
│  │   they are written to maestro.toml as [agents.<id>].                   │  │
│  │                                                                        │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  Help: [a] add  [d] delete  [Ctrl+→/←] switch entry  [Tab] next tab  [Ctrl+s] save │
╰──────────────────────────────────────────────────────────────────────────────╯
```

### 5.2 Agents tab — sub-tab list view (with two entries; second is active)

```
╭ Settings ─ Project │ Sessions │ Budget │ GitHub │ Notif │ Gates │ Review │ Agents │ Modes │ Teams │ Theme │ Layout │ Flags │ TurboQuant │ Advanced ╮
│                                                                              │
│  [agents]                                                                    │
│                                                                              │
│  default: ▾ claude                                                           │
│                                                                              │
│  ┌─ Sub-tabs ─────────────────────────────────────────────────────────────┐  │
│  │  claude     │ ◆ codex ◆ │ + Add agent                                  │  │
│  │                                                                        │  │
│  │  [agents.codex]                                                        │  │
│  │                                                                        │  │
│  │     kind             ▾ codex                                           │  │
│  │   ▸ enabled          [✓]                                               │  │
│  │     command          codex                                             │  │
│  │     base_url         (n/a — subprocess agent)                          │  │
│  │     model            (none)                                            │  │
│  │     env              [ ] (list editor — press Enter to edit)           │  │
│  │     extra_args       [ --json ]                                        │  │
│  │     permission_mode  ▾ default                                         │  │
│  │     allowed_tools    [ ]                                               │  │
│  │     sandbox          (none)                                            │  │
│  │     json             (unset)                                           │  │
│  │     ephemeral        (unset)                                           │  │
│  │     profile          (unset)                                           │  │
│  │     request_timeout  (unset)                                           │  │
│  │     api_key_env      (none)                                            │  │
│  │                                                                        │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  Help: [a] add  [d] delete  [Ctrl+→/←] switch entry  [Tab] next tab  [Ctrl+s] save │
╰──────────────────────────────────────────────────────────────────────────────╯
```

### 5.3 Add-flow modal — name prompt

Triggered by `[a]` from any dynamic-section tab. Same modal shape for
agents, modes, teams, and completion-gates rows (where the prompt asks
for the gate's `name`, not the section key).

```
                          ┌─ Add agent ─────────────────────────────┐
                          │                                         │
                          │   Identifier (becomes [agents.<id>]):   │
                          │                                         │
                          │   ▸ qwen-fast_______________________    │
                          │                                         │
                          │   Kind:                                 │
                          │   ▾ qwen                                │
                          │                                         │
                          │   Identifier must match [a-z0-9_-]+     │
                          │   and not collide with an existing id.  │
                          │                                         │
                          │   [Enter] create  [Esc] cancel          │
                          └─────────────────────────────────────────┘
```

On `Enter`: the modal closes, a stub `[agents.qwen-fast]` row is inserted
into the schema renderer's in-memory state with defaults derived from
the `kind` selection (same defaults that `AgentConfigRaw::from` already
applies — e.g., picking `Ollama` auto-fills `base_url = "http://localhost:11434"`),
the sub-tab list activates the new entry, and the field focus jumps to
the first editable field (`enabled`). The user can `Ctrl+s` to write
the new section to `maestro.toml`, or `Ctrl+r` to abandon.

### 5.4 Remove-flow confirm dialog

Triggered by `[d]` from any dynamic-section row or active sub-tab.

```
                          ┌─ Remove agent? ─────────────────────────┐
                          │                                         │
                          │   You are about to delete:              │
                          │                                         │
                          │     [agents.qwen-fast]                  │
                          │                                         │
                          │   The TOML section and all 16 fields    │
                          │   will be removed from maestro.toml on  │
                          │   the next save. Other agents and the   │
                          │   `default` selector are unchanged.     │
                          │                                         │
                          │   [y] confirm  [n] cancel               │
                          └─────────────────────────────────────────┘
```

After confirm, a flash banner appears at the bottom of the settings
screen for 5 seconds:

```
  ⓘ Removed [agents.qwen-fast].  [u] undo
```

`[u]` within the 5-second window restores the deleted entry in memory
(values intact); after 5 seconds the entry is gone from the screen but
still not on disk — only `Ctrl+s` commits the deletion to
`maestro.toml`.

### 5.5 Completion-gates row table — reorder flow

The `[[sessions.completion_gates.commands]]` array is rendered as a row
table because order matters by definition. `Alt+↑` / `Alt+↓` swap the
focused row with its neighbor.

```
╭ Settings ─ … │ Sessions │ … ╮
│                                                                              │
│  [sessions]                                                                  │
│  ... (existing static fields above) ...                                      │
│                                                                              │
│  [sessions.completion_gates]                                                 │
│     enabled                  [✓]                                             │
│                                                                              │
│  [[sessions.completion_gates.commands]]                                      │
│  ┌──┬───────────────┬─────────────────────────────────────────┬──────────┐   │
│  │ #│ name          │ run                                     │ required │   │
│  ├──┼───────────────┼─────────────────────────────────────────┼──────────┤   │
│  │ 0│ fmt           │ cargo fmt --check                       │ [✓]      │   │
│  │ 1│ clippy        │ cargo clippy -- -D warnings             │ [✓]      │   │
│  │▸2│ test          │ cargo test --all                        │ [✓]      │   │
│  │ 3│ deny          │ cargo deny check                        │ [ ]      │   │
│  └──┴───────────────┴─────────────────────────────────────────┴──────────┘   │
│                                                                              │
│  Help: [a] add row  [d] delete  [Alt+↑/↓] reorder  [Enter] edit cell  [Tab] next tab │
╰──────────────────────────────────────────────────────────────────────────────╯
```

Pressing `Alt+↑` with row 2 focused swaps rows 1 and 2; the focus
follows the moved row (focus stays on the `test` gate, now at index 1).
The mutation is purely in-memory until `Ctrl+s`. Round-trip
implications are in §6.4.

## 6. Schema extension plan

### 6.1 Two new `FieldKind` variants

```rust
// src/config/schema.rs
#[derive(Debug, Clone, Copy)]
pub enum FieldKind {
    // ... existing variants ...

    /// Dynamic-key sub-table. The user adds/removes/renames entries;
    /// every entry has the same `entry_fields` schema. The schema renderer
    /// shows this as a sub-tab strip by default (Presentation::Subtabs).
    /// Maps to `BTreeMap<String, T>` / `HashMap<String, T>` on the Rust side.
    Map {
        entry_fields: &'static [FieldSchema],
    },

    /// Ordered array-of-tables. Entries are integer-indexed; reorder is
    /// part of the edit verb set. Rendered as a row table by default
    /// (Presentation::Rows). Maps to `Vec<T>`.
    VecOfStruct {
        entry_fields: &'static [FieldSchema],
    },
}
```

`DefaultValue` gains a `Empty` variant for both: dynamic sections have
no synthesizable default at the schema layer beyond "start empty."

### 6.2 Optional `Presentation` hint

A field-level hint that does **not** affect serialization, only
rendering. Lives next to `FieldKind`:

```rust
pub enum Presentation { Subtabs, Rows }

pub struct FieldSchema {
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub default: DefaultValue,
    pub kind: FieldKind,
    pub validator: Option<Validator>,
    pub presentation: Option<Presentation>,  // NEW; None = renderer default
}
```

Defaults:

- `FieldKind::Map { .. }` → `Subtabs` if `presentation` is `None`.
- `FieldKind::VecOfStruct { .. }` → `Rows` if `presentation` is `None`.
- All existing variants ignore this field.

Compatibility: every existing const-promoted `FieldSchema` literal needs
`presentation: None` appended. The migration is mechanical and is
explicitly carved into its own follow-up issue (§10.A.1).

### 6.3 Schema registry growth

```rust
// src/config/schema/dynamic.rs (new file)
pub(super) const AGENTS_ENTRY_FIELDS: &[FieldSchema] = &[
    FieldSchema { key: "kind",           label: "Kind",           kind: FieldKind::Enum(AGENT_KINDS), .. },
    FieldSchema { key: "enabled",        label: "Enabled",        kind: FieldKind::Bool, .. },
    FieldSchema { key: "command",        label: "Command",        kind: FieldKind::String, .. },
    FieldSchema { key: "base_url",       label: "Base URL",       kind: FieldKind::String, .. },
    FieldSchema { key: "model",          label: "Model",          kind: FieldKind::String, .. },
    FieldSchema { key: "extra_args",     label: "Extra Args",     kind: FieldKind::StringList, .. },
    FieldSchema { key: "permission_mode", label: "Permission Mode", kind: FieldKind::Enum(PERMISSION_MODES), .. },
    FieldSchema { key: "allowed_tools",  label: "Allowed Tools",  kind: FieldKind::StringList, .. },
    FieldSchema { key: "sandbox",        label: "Sandbox",        kind: FieldKind::String, .. },
    FieldSchema { key: "request_timeout_secs", label: "Request Timeout (s)", kind: FieldKind::Int { .. }, .. },
    FieldSchema { key: "api_key_env",    label: "API Key Env Var", kind: FieldKind::String, .. },
    // env / config_overrides / cli_flags are nested maps — deferred to §10.B (v0.30.0)
];

pub(super) const MODES_ENTRY_FIELDS: &[FieldSchema] = &[
    FieldSchema { key: "system_prompt",   label: "System Prompt",   kind: FieldKind::String, .. },
    FieldSchema { key: "allowed_tools",   label: "Allowed Tools",   kind: FieldKind::StringList, .. },
    FieldSchema { key: "permission_mode", label: "Permission Mode", kind: FieldKind::Enum(PERMISSION_MODES), .. },
];

pub(super) const COMPLETION_GATE_ENTRY_FIELDS: &[FieldSchema] = &[
    FieldSchema { key: "name",     label: "Name",     kind: FieldKind::String, .. },
    FieldSchema { key: "run",      label: "Run",      kind: FieldKind::String, .. },
    FieldSchema { key: "required", label: "Required", kind: FieldKind::Bool, .. },
];

// teams entry: bindings is a free-form HashMap<String, toml::Value>; v1
// renders bindings as a flat StringList of "role=agent_id" pairs and
// skips role_overrides editing (deferred to v0.30.0). See §7 decision matrix.
pub(super) const TEAMS_ENTRY_FIELDS: &[FieldSchema] = &[
    FieldSchema { key: "extends",    label: "Extends",     kind: FieldKind::String, .. },
    FieldSchema { key: "primitive",  label: "Primitive",   kind: FieldKind::Enum(TEAM_PRIMITIVES), .. },
    FieldSchema { key: "min_agents", label: "Min Agents",  kind: FieldKind::StringList, .. },
    FieldSchema { key: "bindings",   label: "Bindings",    kind: FieldKind::StringList, ..  /* role=agent format */ },
];
```

The four new dynamic sections are then registered as `TableSchema`
entries that wrap a single `Map`/`VecOfStruct` field:

```rust
TableSchema {
    name: "agents",
    label: "Agents",
    fields: &[FieldSchema {
        key: "entries",
        label: "Agents",
        kind: FieldKind::Map { entry_fields: AGENTS_ENTRY_FIELDS },
        presentation: Some(Presentation::Subtabs),
        ..
    }, FieldSchema {
        key: "default",
        label: "Default agent",
        kind: FieldKind::String,
        ..
    }],
}
```

(`agents.default` remains a regular leaf — it's the existing `default`
field on `AgentsConfig`, which selects from the keys of `entries`.)

### 6.4 Round-trip with `toml_edit` — add / remove / reorder semantics

The existing `Config::save_into_str` flow (`src/config/mod.rs:144`):

```
1. Parse original text → toml_edit::DocumentMut.
2. Parse original text → Config (typed model).
3. Serialize current in-memory Config → canonical TOML string.
4. Serialize original Config → canonical TOML string.
5. Diff the two strings; for each delta, mutate the DocumentMut.
6. Render DocumentMut → new file text.
```

This works for static-shape changes because the diff is keyed by dotted
path. Dynamic sections require three extensions:

**Add.** The in-memory `Config` now has a key `agents.qwen-fast` not
present on disk. The differ already sees this as "new section." It
emits a fresh `[agents.qwen-fast]` block at the end of the document
(via `toml_edit::table().set_implicit(false)` plus inserts for each
field). Comments are preserved everywhere else.

**Remove.** The in-memory `Config` is missing `agents.qwen-fast` that
was on disk. The differ removes the entire `[agents.qwen-fast]` block
from the `DocumentMut`. Trailing blank lines collapse — explicitly
tested via golden fixtures.

**Reorder (VecOfStruct).** This is the load-bearing case the differ
does NOT handle today. `[[sessions.completion_gates.commands]]` reorder
appears in the canonical-TOML diff as N pairs of "field changed in
slot i" — but the entries themselves are structurally identical, just
swapped. The differ must compare arrays-of-tables **element-wise by
content hash**, not by index, then detect "this is a permutation" and
emit reorder mutations on the `toml_edit` array. The follow-up issue
for §10.A.3 owns this.

**Rename.** Out of scope for v0.29.0 (Q4). Implementation note for
future work: `toml_edit::Table::rename_key` exists but does not move
trailing comments; the user-facing fix is to require the user to
re-type the section name into a freshly-added entry, then delete the
old one. The 5-second undo flash makes this acceptable.

### 6.5 Renderer integration

`from_schema` (in `schema_tab/build.rs`) recursively flattens
`NestedTable` today. The new variants are **not** flattened — they
produce a single `WidgetKind::DynamicMap` or `WidgetKind::DynamicRows`
that owns its own sub-state:

```rust
pub enum WidgetKind {
    // ... existing variants ...
    DynamicMap(DynamicMapWidget),    // sub-tab list + per-entry FieldGroup
    DynamicRows(DynamicRowsWidget),  // row table + per-row FieldGroup
}
```

The widgets internally use the same primitive widgets (`Toggle`,
`NumberStepper`, `TextInput`, `Dropdown`, `ListEditor`) for each entry's
fields — they just compose them dynamically based on the current set of
keys / row count, not a static label list. **Label uniqueness** (today's
`sync_to_config` uses `widget.label() == label` to dispatch) is solved
by namespacing labels: `agents.<id>.command`, not `command`. The
existing label-lookup loop is unchanged.

## 7. Decision matrix — what ships under #719

| Section | In scope for v0.29.0 follow-ups? | Notes |
|---|---|---|
| `[agents.<id>]` (top-level fields) | **Yes** | All 11 scalar/list fields. Sub-tab presentation. |
| `[agents.<id>].env` (nested map) | **No — v0.30.0** | Map-of-string-to-string; not in `FieldKind::Map { .. }` v1. |
| `[agents.<id>].config_overrides` (`BTreeMap<String, toml::Value>`) | **No — v0.30.0** | Free-form sub-table; needs a "TOML-value editor" widget that v1 does not have. |
| `[agents.<id>].cli_flags` (free-form) | **No — v0.30.0** | Same reason. |
| `[modes.<id>]` | **Yes** | All 3 fields. Sub-tab presentation. |
| `[teams.<id>]` (extends, primitive, min_agents, bindings as `role=agent` list) | **Yes** | Row presentation. Bindings rendered as `StringList` of `role=agent` strings — parsed on save. |
| `[teams.<id>].role_overrides` (nested map of structs) | **No — v0.30.0** | Map of struct = nested dynamic section; needs `FieldKind::Map` inside `FieldKind::Map`. Out of v1 to keep the test surface bounded. Users still edit by hand or via the Team Wizard (#2026-05-05). |
| `[[sessions.completion_gates.commands]]` | **Yes** | Row presentation with reorder. |
| Any other array-of-tables in `maestro.toml` | n/a | None exist today. |

## 8. Risks and open questions

### Risks

1. **`toml_edit` reorder diff complexity.** Element-wise content-hash
   comparison is more code than the existing static differ. Mitigation:
   ship the reorder follow-up (§10.A.3) behind an off-by-default flag
   for one release, gate by golden fixture covering at least 5
   permutation patterns. If it slips, reorder UX falls back to
   delete-and-re-add (still functional, loses comment preservation on
   the moved rows only).

2. **Schema test count drift.** `schema_test::EXPECTED_NON_NESTED_FIELDS = 52`
   asserts the field count exactly. Dynamic sections break this invariant
   because the count depends on user input. Mitigation: split the
   assertion into "static fields count" + "dynamic sections count"
   (cardinality of `TableSchema` entries whose top-level field is `Map`
   or `VecOfStruct`).

3. **Auto-doc (`docs/configuration.md`) representation.** #717 walks
   `schema_for_config()` and emits a per-table fields table. For dynamic
   sections the doc shape must change to "fields per entry" + a prose
   note. Mitigation: explicit follow-up (§10.A.5) that lands docs
   update with the schema change so #717's golden test does not break
   silently.

4. **Label collisions on identifiers containing dots.** A user named
   `agents.my.weird.id` would break dotted-path lookup. Mitigation:
   identifier validation per §3 Q7 (anchored regex + reserved deny-list)
   enforced at modal-prompt time (mockup §5.3). Reject at add, never at
   save.

5. **Silent failures on add.** The current builder falls back to defaults
   on `toml::Value` serialization errors. Dynamic-section adds must fail
   loudly. Mitigation: add modal returns `Result<(), ValidationFeedback>`
   and surfaces errors as a flash banner inline with the modal.

6. **Permutation hash collision in `VecOfStruct` reorder differ.** §6.4
   detects reorders by hashing each `InlineTable` and comparing
   multisets between before/after. A non-canonical hash (e.g.,
   `format!("{:?}", v)` over a `HashMap`) is non-deterministic on map
   key order — two distinct entries could share a hash and be silently
   swapped, corrupting the array. Mitigation: hash uses sorted-key
   canonical TOML serialization (`toml::ser::to_string` is not stable
   for maps; use a normalized intermediate). Property test: distinct
   entries never share a hash; identical content always hashes equal
   (committed under §9).

7. **`extends` cycle on teams.** §6.3 exposes `teams.<id>.extends: String`
   referencing another team id. Naive resolver follows the chain →
   stack overflow on self- or 2-cycle. Mitigation per §3 Q8: cross-entry
   validator detects cycles via depth-bounded walk (depth ≤ 16, fail
   closed). Same pass confirms `agents.default` references an existing
   key (already implemented at `src/config/agents.rs:248` — confirm it
   rejects rather than loops).

### Open questions

A. **Sub-tab strip width when there are many entries.** If a user has
   12 agents, the sub-tab strip wraps or truncates. Spec defaults to
   truncation with `…` and `Ctrl+→`/`Ctrl+←` navigation; an overflow
   dropdown is deferred to v0.30.0.

B. **Should `teams.<id>.bindings` get its own widget?** v1 punts to
   `StringList` of `role=agent` pairs. A typed two-column `MapEditor`
   widget would be friendlier but adds widget surface. Punt to v0.30.0
   unless a contributor wants to take the smaller follow-up first.

C. **Validators for dynamic sections.** Today validators are
   `fn(&toml::Value) -> Result<(), String>`. For dynamic sections we
   probably want a cross-entry validator (e.g., `agents.default` must
   reference an existing key). v1 keeps validators per-field; the
   cross-entry check stays in `AgentsConfig::validate` (already
   implemented at `src/config/agents.rs:248`).

## 9. Testing strategy

| Layer | Test type | Coverage |
|---|---|---|
| Schema extensions | Unit | `FieldKind::Map` / `VecOfStruct` round-trip via `serde`; presentation hint preserved; const-array compile-time |
| Renderer (`from_schema` for dynamic) | Unit + insta | Empty state, 1 entry, many entries; sub-tab strip render at 80×24 / 60×20 / 120×40 |
| Add modal | Unit + insta | Valid id accepted; invalid id rejected with inline feedback; kind-aware defaults populated |
| Remove flow | Unit | Confirm cancels = no-op; confirm accepts = entry removed; undo within 5s restores; undo after 5s no-op |
| Reorder (VecOfStruct) | Unit + insta + property | Single swap, swap-of-swap = identity, repeated Alt+↓ reaches end and stops; insta covers row table at indices 0/middle/last selected |
| Permutation hash | Property | Distinct entries never collide; identical entries always equal; map-key order does not affect hash (canonical sorted-key form) |
| Comment-preserving round-trip | Golden fixtures | Add new `[agents.x]` preserves all prior comments; remove `[agents.x]` collapses trailing blank lines correctly; reorder of 3 commands preserves leading comments on commands that did NOT move |
| Schema docs (#717) | Insta | New tables render as "dynamic section, one row per entry" with per-entry field list |
| Cross-entry validation | Unit | `agents.default = "foo"` with no `agents.foo` is rejected at save time with a focused error; `teams.a.extends = "a"` (self-cycle) rejected; `teams.a.extends = "b"` ∧ `teams.b.extends = "a"` (2-cycle) rejected |
| Identifier validation | Unit | `^[a-z0-9][a-z0-9_-]{0,62}$` accepts `qwen-fast`, `claude_3`; rejects `-rm`, `foo.bar`, `Claude` (uppercase), `default` (reserved), 64-char id |
| Undo buffer semantics | Unit | Delete-then-delete within 5s: only the 2nd delete is in the undo slot; undo restores the 2nd; the 1st remains pending until `Ctrl+s` (still recoverable via `Ctrl+r`) |

Fixtures committed to `tests/fixtures/dynamic_config/`:

- `agents_add.toml.before` + `.after`
- `agents_remove.toml.before` + `.after`
- `completion_gates_reorder.toml.before` + `.after`
- `comments_preserved.toml` (mixed scenario)

## 10. Follow-up implementation issues

Filed as children of #719 once this spec is approved. Two tiers: **A** is
in-scope for v0.29.0 closure; **B** is v0.30.0 work that depends on A.

### A. v0.29.0 (closes the dynamic-config gap)

| ID | Title | Scope | Dependencies | Milestone |
|---|---|---|---|---|
| A.1 ✅ | `feat(config/schema): introduce FieldKind::Map and FieldKind::VecOfStruct variants + Presentation hint` | Adds the two enum variants, the `Presentation` field on `FieldSchema`, and `presentation: None` to every existing literal. No renderer wiring yet; unit tests assert the schema compiles and round-trips through serde. **Shipped in #789** (`feat/issue-789-feat-config-schema-introduce-fieldkind-m`). | — | v0.29.0 |
| A.2 | `feat(tui/widgets): DynamicMap and DynamicRows widget primitives` | New `WidgetKind` variants; per-entry sub-state; integration with the existing keymap. Widget-level tests only (no schema integration yet). Includes the Add modal, Remove confirm + undo flash, and Alt+↑/↓ reorder. | A.1 | v0.29.0 |
| A.3 | `feat(config): toml_edit comment-preserving round-trip for dynamic sections` | Extends `Config::save_into_str` to handle add/remove of dynamic sub-tables and reorder of array-of-tables. Element-wise content-hash compare for permutation detection. Golden fixtures committed. | A.1 | v0.29.0 |
| A.4a ✅ | `refactor(tui/settings): wire Agents, Modes, completion_gates.commands tabs through schema renderer` | Shipped in #792 (v0.29.0). `[teams.<id>]` wiring was carved out due to `TeamConfig.bindings: #[serde(flatten)]` requiring a sync-time adapter. | A.1, A.2, A.3 | v0.29.0 |
| A.4b ✅ | `feat(tui/settings): wire Teams tab through schema renderer with bindings round-trip adapter` | Shipped in #803 (v0.29.5). Adds `SettingsTab::Teams` (index 9), `TEAMS_TABLE` schema, `teams_bindings.rs` encode/decode adapter, `validate_extends` hook. | A.4a | v0.29.5 |
| A.5 ✅ | `feat(docs): auto-generate docs/configuration.md sections for dynamic config tables` | Shipped in #793 (v0.29.0). `teams` added to `SCHEMA_BACKFILL_PENDING` (schema autogen for `FlattenedMap` is a separate refactor). | A.1 | v0.29.0 |

**Sequence:** ✅A.1 → ✅A.2 ∥ ✅A.3 → ✅A.4a → ✅A.4b → ✅A.5

### B. v0.30.0 (deferred for cardinality and widget reasons)

| ID | Title | Scope | Dependencies | Milestone |
|---|---|---|---|---|
| B.1 | `feat(tui/widgets): MapEditor — typed two-column key/value editor` | New widget for `BTreeMap<String, String>` (covers `agents.<id>.env`) and for `teams.<id>.bindings` (rendered as `role=agent` pairs typed). | A.2 | v0.30.0 |
| B.2 | `feat(config/schema): support nested Map<String, Struct> for role_overrides` | Allows `FieldKind::Map { entry_fields }` to appear *inside* an entry's `entry_fields`. Unblocks editing `[teams.<id>.role_overrides.<role>]`. | A.1, A.4 | v0.30.0 |
| B.3 | `feat(tui/settings): rename verb for dynamic sub-tables` | Adds `[R]ename` to the dynamic widget keymap. Resolves the trailing-comment-preservation issue on `toml_edit::Table::rename_key`. | A.3 | v0.30.0 |
| B.4 | `feat(tui/widgets): TomlValueEditor for free-form sub-tables` | Widget for `BTreeMap<String, toml::Value>` (covers `agents.<id>.config_overrides`, `cli_flags`). Lifts the "deferred" rows from §7. | A.2 | v0.30.0 |
| B.5 | `feat(tui/settings): sub-tab strip overflow dropdown` | When a dynamic section has > N entries (default 8) the strip collapses behind a `▾ More …` dropdown. | A.4 | v0.30.0 |

## 11. Design Decisions

**ETC Assessment:**
- `Presentation` is a hint, not a contract — adding a new presentation
  (e.g., `Presentation::Cards`) is a renderer-side change with no schema
  ripple. Easy to change.
- `FieldKind::Map` / `VecOfStruct` are additive enum variants. Existing
  matches in `schema_tab/build.rs` need a new arm; the compiler enforces
  exhaustiveness. Locked-in cost: every existing const-array gains
  `presentation: None`. One-time mechanical migration.
- Rename verb deferred to B.3 specifically because adding it later does
  not invalidate any v1 round-trip work. The fallback (delete + add) is
  always available.

**Demeter Compliance:**
- The flagged risk chain `screen.fields_per_tab[idx].iter().find(...).widget.label()`
  is already long (Demeter violation) but inherited from existing
  sync code. This spec does not deepen it; new dynamic widgets expose
  their own `entries()` accessor instead of chaining into private state.

**Calisthenics Score:** 6/9
- ✅ One level of indentation per method (renderer arms are flat).
- ✅ No `else` (early returns / `match`).
- ✅ Wrap primitives (`AgentId(String)` would be ideal; current code uses
  bare `String` — flagged as a tech-debt note, not fixed under this issue).
- ✅ First-class collections (DynamicMapWidget owns its sub-state).
- ⚠ One dot per line — see Demeter note above.
- ✅ Don't abbreviate.
- ✅ Keep entities small — every new widget file ≤ 400 LOC per guardrail §7.
- ⚠ No more than 2 instance variables — `SettingsScreen` already has 17;
  spec adds 0 (dynamic state lives inside widgets).
- ⚠ No getters/setters — `WidgetKind` exposes `.label()` which is a
  getter. Inherited from existing code; not in scope to fix.

**Trade-off Triangle:**
- **Architecture triangle:** we choose **Simplicity + Flexibility** over
  raw Performance.
- **Rationale:** the schema renderer round-trips through `toml::Value`
  on every field write. Dynamic sections amplify this — adding an agent
  with 11 fields = 11 writes = 11 round-trips. Performance acceptable
  because settings is not a hot path (one human-paced edit at a time);
  the value of one declaration driving renderer + docs + validation is
  worth the cost. Faster path (mutating `toml_edit::DocumentMut`
  directly) was rejected to keep typed-`Config` as the source of truth.

## 12. Out of scope (v0.29.0)

- Editing built-in `.claude/skills/*` or `.claude/agents/*` markdown
  files via the settings TUI — orthogonal.
- Multi-row selection / bulk operations on dynamic-section rows.
- Importing / exporting individual dynamic sections (e.g., "export
  this agent as a snippet"). Could be a CLI verb in v0.30.0+.
- Live syntax validation of `run` commands in completion gates (e.g.,
  shell-syntax check). Out of scope; existing runtime check on dispatch
  is the source of truth.
- Real-time sync with `maestro doctor` health output (badging unhealthy
  agents in the sub-tab strip). Deferred until doctor exposes a
  library-call surface for the TUI to consume cheaply.

## 13. Acceptance criteria (for this spec)

- [x] Mental model decided (§3 Q1, §4).
- [x] Add flow specified with mockup (§5.3).
- [x] Remove flow specified with mockup (§5.4).
- [x] Rename flow decision recorded (§3 Q4; "not in v1, fallback documented").
- [x] Reorder flow specified with mockup (§5.5).
- [x] Schema extensions defined (§6.1, §6.2).
- [x] Decision matrix — in-scope vs deferred (§7).
- [x] Follow-up issues listed with milestones and dependencies (§10).
- [x] At least 3 TUI mockups (§5 has 5).
- [x] Risks and open questions captured (§8).
- [x] Testing strategy (§9).

## 14. Approval

- [ ] Carlos — design lead
- [ ] Reviewer 2 (TBD)

Once both checkboxes are ticked, the orchestrator files A.1 through A.5
as GitHub issues against milestone **v0.29.0** with this spec linked
from each issue body.
