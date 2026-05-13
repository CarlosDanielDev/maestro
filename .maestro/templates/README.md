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
└── commands/              ← canonical command specs (populated in #C)
    └── .gitkeep
```

> Authoritative project layout lives in `directory-tree.md` at the repo root.
> The snippet above is illustrative for this subtree only.

## Forward-reference legend

Letter codes (`#A`, `#B`, `#C`, `#G`) reference work items in the approved
plan `we-need-to-standardize-zippy-wave.md`. They are placeholders until the
follow-up GitHub issues are filed; replace with concrete `#NNN` numbers as
each lands.

| Code | Scope |
|------|-------|
| #A   | This issue (#700) — L0 scaffold |
| #B   | Render engine — resolves placeholders into per-provider output |
| #C   | Canonical command specs — populates `commands/` |
| #G   | `maestro sync-templates` CLI — re-renders from canonical sources |

