# Team orchestration presets

A **team preset** binds a set of role agents (implementer, reviewer, docs, …) to a coordination **primitive** (pipeline, single-pass, fan-out, verdict-only). The preset is the unit of reuse and the unit of CLI launch.

> Spec: [`docs/superpowers/specs/2026-05-05-orchestration-wizard-design.md`](../superpowers/specs/2026-05-05-orchestration-wizard-design.md)
> Plan: [`docs/superpowers/plans/2026-05-05-orchestration-wizard.md`](../superpowers/plans/2026-05-05-orchestration-wizard.md)

## Three tiers

Presets resolve from three layers; the highest layer that defines a name wins.

| Tier | Location | Purpose |
|---|---|---|
| **built-in** | embedded in the binary (`src/orchestration/builtins/*.toml`) | ships with maestro; users never edit |
| **user** | `~/.config/maestro/maestro/teams/` (platform-specific) | personal presets shared across projects |
| **project** | `<repo>/.maestro/teams/` | per-repo overrides; checked into source control |

The `extends` key in any TOML lets a preset inherit from another by name. The merge walks the chain root → leaf, with leaf values overriding parent values per role binding.

## Five built-ins

| Name | Primitive | Roles | Use when |
|---|---|---|---|
| [`default-coder`](default-coder.md) | pipeline | implementer → reviewer → docs | implementing a feature or bugfix end-to-end |
| [`default-researcher`](default-researcher.md) | verdict-only | implementer + reviewer | scoping work or producing a recommendation without code changes |
| [`default-triager`](default-triager.md) | verdict-only | triager | classifying ideas in the inbox before they earn DOR |
| [`default-reviewer`](default-reviewer.md) | single-pass | reviewer | running a one-shot review against an existing PR or branch |
| [`default-docs`](default-docs.md) | single-pass | docs | refreshing documentation without touching code |

All five run on `claude` out of the box. To swap an agent (e.g. point the implementer at `opencode` for a cheaper iteration), copy the preset to your user tier and override:

```sh
maestro team new cheap-coder --extends default-coder --implementer opencode
```

That writes `~/.config/maestro/maestro/teams/cheap-coder.toml` with `extends = "default-coder"` plus the override, leaving the built-in untouched.

## User Guide

New to team presets? Walk through the cookbook in order:

1. [When to use a team](cookbook/01-when-to-use-a-team.md) — decision matrix, cost intuition.
2. [Extending a built-in](cookbook/02-extending-a-builtin.md) — swap an agent, add a prompt addendum, pin to project tier.
3. [Composing from scratch](cookbook/03-composing-from-scratch.md) — schema cheat sheet, naming rules.
4. [Recipes](cookbook/04-recipes.md) — end-to-end `team new` → `team launch --yes` walkthrough plus multi-issue and tier-precedence flows.
5. [Troubleshooting](cookbook/05-troubleshooting.md) — every loader error message and the honest v1 scope notes.

Every TOML snippet in the cookbook is mirrored by a fixture under `tests/fixtures/teams_cookbook/` and validated on every CI run.

## CLI surface

| Command | Purpose |
|---|---|
| `maestro team list` | list all resolved teams across the three tiers |
| `maestro team list --json` | machine-readable form for scripts |
| `maestro team new <name> --extends <parent> [--implementer …] [--reviewer …] [--docs …] [--tier user\|project]` | create a new preset |
| `maestro team explain <name>` | show resolved bindings (primitive, agent per role, mode, fallback) |
| `maestro team explain <name> --json` | JSON form of the same |
| `maestro team manage --list` | list user-tier presets with on-disk paths |
| `maestro team launch <preset> --issue N --yes` | headless run — drives the scheduler to completion, exits non-zero on any issue failure |
| `maestro team launch <preset> --issues 1,2,3 --yes [--max-parallel 3]` | same for a set of issues, respecting `Blocked By` dependencies |

The interactive `compose / launch / manage` flows live in the TUI wizard (`maestro` then `[t]`); the CLI surface above is the headless equivalent for scripting and CI.

## Headless launch (`--yes`)

`team launch <preset> --yes` skips every interactive step. It resolves the preset, builds a dependency-aware plan from the supplied issues, and drives each issue's primitive machine through real subagent dispatch.

- Exit code `0` only if every issue reaches `Succeeded`.
- Per-issue failures are listed on stdout with the failure reason; total counts print after the plan summary.
- v1 returns synthetic PR identifiers — the scheduler-to-dispatch wiring is exercised end-to-end, but real worktree + PR creation is still TUI-coupled and lands in a v0.27.x follow-up.

## State migration

The on-disk state file (`maestro-state.json`) carries an explicit `version: u32` field starting with v0.26.0. Files written by older maestro versions (no `version` key) load with `version = 0` and migrate to `CURRENT_STATE_VERSION = 1` on first read. The migration is structural: every later-added field already defaulted via `#[serde(default)]`, so the bump is the only mutation required for `0 → 1`.

## See also

- [`directory-tree.md`](../../directory-tree.md) — full project layout
- [`docs/releases/v0.26.0.md`](../releases/v0.26.0.md) — v0.26.0 release notes
