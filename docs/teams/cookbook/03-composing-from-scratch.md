# Composing a preset from scratch

When `extends`-ing a built-in won't get you there — a different primitive, a role outside the coder triad, or a fresh starting point — set `extends = ""` and define everything yourself.

## When you need it

- You want a `fan-out` team. No built-in ships with the `fan-out` primitive in v1.
- You want a single-pass team in a non-default shape (e.g. researcher-only).
- The set of roles you need doesn't overlap with any built-in's bindings.

If you're swapping one or two roles in `default-coder`, stay in [Recipe 1](02-extending-a-builtin.md) — extending is shorter and survives upstream tweaks to the built-in.

## Schema cheat sheet

Every key valid in a preset TOML, from `src/orchestration/team.rs::TeamConfig`:

| Key | Type | Required | Notes |
|---|---|---|---|
| `extends` | `string` | yes (use `""` for root) | parent preset name; `""` marks the root of an `extends` chain |
| `primitive` | `string` | required somewhere in the chain | one of `pipeline`, `fan-out`, `single-pass`, `verdict-only` (kebab-case, exact) |
| `min_agents` | `[string]` | required somewhere in the chain | agent IDs that MUST be present in `[agents.*]` and healthy for pre-flight to pass; for v1 always includes `"claude"` because L2 runs on Claude |
| `<role> = "<agent>"` | `string` | optional | minimal-form binding; `<role>` is one of `implementer`, `reviewer`, `docs`, `devops`, `orchestrator`, `triager`, `researcher` |
| `[role_overrides.<role>]` | table | optional | rich-form override; keys: `agent`, `mode`, `model_override`, `prompt_addendum`, `fallback_agent` (all optional) |

Primitive variants and their required role bindings, from `src/orchestration/types.rs::Primitive::required_roles`:

| Primitive | Required roles |
|---|---|
| `pipeline` | implementer, reviewer, docs |
| `fan-out` | reviewer |
| `single-pass` | _(none)_ |
| `verdict-only` | reviewer |

A required role left unbound is a pre-flight failure with a single human-readable diagnostic; the team will not launch.

## Recipe 4 — A fan-out reviewer team (no built-in for this primitive)

Goal: run a reviewer in parallel — useful when you have several patches and want independent verdicts without coordination overhead. There is no `default-fan-out` shipped with maestro; compose from scratch.

```toml
extends = ""
primitive = "fan-out"
min_agents = ["claude"]
reviewer = "claude"
```

Save as `~/.config/maestro/maestro/teams/fanout-reviewers.toml` (or under `.maestro/teams/` for project tier). Note `min_agents = ["claude"]` is mandatory because L2 still runs on Claude in v1 — see [Troubleshooting](05-troubleshooting.md) for the rationale.

Fixture: [`tests/fixtures/teams_cookbook/fanout-reviewers.toml`](../../../tests/fixtures/teams_cookbook/fanout-reviewers.toml).

## Recipe 5 — A triager preset for the idea inbox

Goal: a verdict-only preset that classifies idea-inbox issues, with a project-specific addendum to the triager's prompt. The CLI `new` flags don't cover `triager`, so this one is TOML-by-hand:

```toml
extends = "default-triager"

[role_overrides.triager]
prompt_addendum = "Triage strictly: park anything missing acceptance criteria."
```

This still uses `extends` because [`default-triager`](../default-triager.md) already binds the right primitive (`verdict-only`) and `min_agents`. The override only tightens the prompt.

Fixture: [`tests/fixtures/teams_cookbook/inbox-triager.toml`](../../../tests/fixtures/teams_cookbook/inbox-triager.toml).

## Naming rules

`src/orchestration/loader.rs::validate_preset_name` rejects:

- empty names — `"preset name must not be empty"`
- names longer than 64 characters — `"preset name \"...\" is N chars, max 64"`
- names starting with `.` — `"preset name \"...\" cannot start with '.' (would shadow a hidden file)"`
- names starting with `-` — `"preset name \"...\" cannot start with '-' (would parse as a CLI flag)"`
- names containing `/`, `\`, NUL, or `..` — `"preset name \"...\" contains illegal path characters"`

Quote the messages directly when filing bug reports; they're the single source of truth, not this doc.

## File size and format

- Loader caps any single TOML file at **1 MiB** (`MAX_PRESET_FILE_BYTES` in `loader.rs`). A real preset is 50–100 bytes; if you hit the cap, the file isn't a preset.
- Only files with the `.toml` extension are loaded. Files are processed in deterministic sorted order, so `01-foo.toml` loads before `02-bar.toml`.
- The `[teams.*]` inline form inside `maestro.toml` is also supported for project-tier presets — see `Loader::with_project_inline`. Useful for a one-off preset that doesn't deserve its own file.

## See also

- [Extending a built-in](02-extending-a-builtin.md) — the short path for small variations.
- [Recipes](04-recipes.md) — multi-issue launch and tier-precedence walkthroughs.
- [Troubleshooting](05-troubleshooting.md) — every loader error message and the v1 scope notes.
