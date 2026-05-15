# Canonical Templates

This directory holds **canonical**, provider-agnostic command and fragment
sources. The render engine (issue #B) projects them into provider-specific
outputs such as `.claude/commands/*.md` (Claude Code) or `.cursor/commands/*.md`
(Cursor). Edit canonical sources here and re-render with `maestro sync-templates`
(lands in #G) — never edit the rendered files under `.claude/commands/*.md`
directly. The canonical-vs-rendered split exists so cross-provider rules
(premises, TDD cycle, dependency-graph mandate) live in exactly one place.

## Layout

```
.maestro/templates/
├── README.md              ← this file
├── manifest.toml          ← placeholder vocabulary + subagent registry (canonical)
├── core/                  ← shared fragments included by every spec
│   ├── premises.md
│   ├── tdd-cycle.md
│   └── dependency-graph.md
└── commands/              ← canonical command specs (Issue #702)
    ├── implement.md
    ├── pushup.md
    ├── plan-feature.md
    └── simplify.md
```

> Authoritative project layout lives in `directory-tree.md` at the repo root.
> The snippet above is illustrative for this subtree only.

## manifest.toml schema

`manifest.toml` is the single source of truth for all placeholder metadata used
by the render engine.

### `[[subagents]]` — subagent registry (Issue #728)

Each entry in the `[[subagents]]` TOML array declares one subagent that appears
in the rendered `{{SUBAGENT_LIST}}` placeholder:

```toml
[[subagents]]
slug    = "subagent-gatekeeper"
purpose = "DOR, blockers, and API-contract gate for `/implement`"
```

| Key | Type | Description |
|-----|------|-------------|
| `slug` | string | Exact filename stem of the agent file (e.g. `subagent-gatekeeper` matches `.claude/agents/subagent-gatekeeper.md`) |
| `purpose` | string | One-line description; rendered verbatim as the "Purpose" column in the Markdown table |

**Order matters.** Rows in the rendered table follow the order of entries in
this file (pipeline order, not alphabetical).

**Drift detection.** The set of slugs in `[[subagents]]` must match the set of
`subagent-*.md` files on disk under `.claude/agents/`. Any mismatch is caught
at test time by `tests/subagent_manifest_drift.rs`. When you add or remove an
agent file you must update this array.

**Render path.** `src/templates/provider_rules/subagent_list.rs` reads this
array via `Manifest::subagents()` and converts it into the Markdown table.
The three provider rule files (`claude.rs`, `codex.rs`, `http_generic.rs`) all
delegate their `subagent_list()` method to that shared helper.

## Cutover policy (post-#703)

`.maestro/templates/` is the **single source of truth** for slash-command specs.
`.claude/commands/*.md` files that are listed as `[generated]` in `directory-tree.md`
(`implement.md`, `pushup.md`, `plan-feature.md`, `simplify.md`) are **rendered artifacts**.

- **Never edit generated files directly.** Edit the canonical source in
  `.maestro/templates/commands/` and re-render.
- **CI enforces drift.** `tests/templates_render.rs` contains byte-identical regression
  tests. Any mismatch between a canonical source and its rendered output is caught at
  compile/test time.
- Commands without a canonical spec (`create-subagent.md`, `release.md`, etc.) remain
  hand-maintained for now.

### Regenerating Claude baselines

After editing a canonical spec under `.maestro/templates/commands/` or a shared fragment
under `.maestro/templates/core/`, regenerate the rendered `.claude/commands/*.md`
artifacts with:

```sh
cargo run --example regen_templates
```

The example writes the four canonical commands (`implement`, `pushup`, `plan-feature`,
`simplify`) into the directory reported by `ClaudeRules::target_dir()` (`.claude/commands/`).
Confirm the regeneration with `cargo test --test templates_render` — the byte-identical
regression tests pass when the rendered output matches the canonical render.

> The dedicated `maestro sync-templates` CLI (forward-reference `#G`) will eventually
> replace this example with a provider-aware command. Until that lands, the example is
> the regen path.

## Forward-reference legend

Letter codes (`#A`, `#B`, `#G`) reference work items in the approved
plan `we-need-to-standardize-zippy-wave.md`. `#A`, `#B`, and `#C` have
landed; replace `#G` with a concrete `#NNN` number when that issue is filed.

| Code | Issue | Scope |
|------|-------|-------|
| #A   | #700  | L0 scaffold — canonical templates directory |
| #B   | #701  | Render engine — resolves placeholders into per-provider output |
| #C   | #702  | Canonical command specs — populated `commands/` with implement, pushup, plan-feature, simplify |
| #G   | TBD   | `maestro sync-templates` CLI — re-renders from canonical sources |

