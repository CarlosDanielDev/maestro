# `default-researcher` — verdict-only research team

Produces a recommendation or scoped plan **without code changes**. Verdict-only primitive ends the run with a `decision` + `rationale` (and optionally a list of `NewIssueDraft`s).

## Canonical TOML

```toml
extends = ""
primitive = "verdict-only"
min_agents = ["claude"]
implementer = "claude"
reviewer = "claude"
```

## Bindings

| Role | Agent | Notes |
|---|---|---|
| implementer | `claude` | here, the role label is misleading — implementer is the **researcher** for this primitive; it gathers evidence |
| reviewer | `claude` | adversarially evaluates the researcher's recommendation |

## When to use

- "Should we adopt X?" / "Which library fits Y?" — questions that need investigation, not implementation.
- Pre-DOR scoping: a researcher run can produce `new_issues` drafts that downstream go through `subagent-idea-triager`.
- Architecture spike before committing to an approach.

## Output shape

`PrimitiveOutput::Verdict { decision, rationale, new_issues }`. The orchestrator prints `decision` + `rationale` to stdout; any `new_issues` drafts are surfaced for follow-up triage rather than auto-created.

## Customise — heavier-context researcher

```sh
maestro team new deep-research \
  --extends default-researcher \
  --implementer claude \
  --reviewer opencode
```

Then in `~/.config/maestro/maestro/teams/deep-research.toml` add a richer override:

```toml
extends = "default-researcher"

[role_overrides.implementer]
agent = "claude"
model_override = "claude-opus-4-7"
prompt_addendum = "Cite sources as inline footnotes. Prefer primary sources."
```

## Inspect

```sh
maestro team explain default-researcher --json
```

## See also

- [`default-triager.md`](default-triager.md) — verdict-only sibling for inbox triage
- [`README.md`](README.md) — preset overview
