# `default-coder` — pipeline coding team

The default end-to-end coding team. Drives an issue from implementation through review through documentation in three sequential subagent dispatches.

## Canonical TOML

```toml
extends = ""
primitive = "pipeline"
min_agents = ["claude"]
implementer = "claude"
reviewer = "claude"
docs = "claude"
```

## Bindings

| Role | Agent | Notes |
|---|---|---|
| implementer | `claude` | writes the code |
| reviewer | `claude` | inspects the diff, returns `ReviewVerdict::Approved` / `RequestChanges` |
| docs | `claude` | updates README/CHANGELOG/inline docs |

`min_agents = ["claude"]` is the pre-flight gate — if `[agents.claude]` is absent from your `maestro.toml`, the launch fails before any subagent runs.

## When to use

- A normal feature or bugfix on a single issue.
- The issue has clear `Acceptance Criteria` and `Definition of Done`; the pipeline doesn't replan or scope the work.

## Customise — cheap iteration on a non-Claude implementer

```sh
maestro team new cheap-coder --extends default-coder --implementer opencode
```

The reviewer and docs bindings still inherit `claude` from the parent; only the implementer is overridden. Run with:

```sh
maestro team launch cheap-coder --issue 547 --yes
```

## Customise — a stricter reviewer mode

Drop a richer override in `~/.config/maestro/maestro/teams/strict-coder.toml`:

```toml
extends = "default-coder"

[role_overrides.reviewer]
agent = "claude"
mode = "review-strict"
prompt_addendum = "Reject any PR with a TODO or FIXME in changed lines."
fallback_agent = "opencode"
```

`mode` looks up `[modes.review-strict]` from your `maestro.toml`; `prompt_addendum` is appended to the reviewer's system prompt; `fallback_agent` is used if the primary fails to return a valid `ReviewFindings`.

## Inspect

```sh
maestro team explain default-coder
```

prints the resolved bindings and primitive. With `--json`, the same data is machine-readable for diffing across tiers.

## See also

- [`README.md`](README.md) — preset overview
- Spec §4 — comparison table
