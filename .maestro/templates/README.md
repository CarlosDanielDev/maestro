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
├── manifest.toml          ← placeholder vocabulary (skeleton)
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

