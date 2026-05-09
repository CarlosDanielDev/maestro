# `default-triager` — idea-inbox triage team

Single-role verdict-only team that classifies ideas in the inbox. The `triager` role consults `subagent-idea-triager` and returns a `decision` of **promote / park / archive** with rationale.

## Canonical TOML

```toml
extends = ""
primitive = "verdict-only"
min_agents = ["claude"]
triager = "claude"
```

## Bindings

| Role | Agent | Notes |
|---|---|---|
| triager | `claude` | runs the 5-question honesty check, emits a structured promote/park/archive verdict |

## When to use

- Bulk triage of `idea` issues created via the `idea.yml` template.
- Pre-DOR funnel — triaged ideas either earn DOR (promoted to features) or are explicitly parked/archived.

## Output shape

`PrimitiveOutput::Verdict { decision, rationale, new_issues }`. For triage:

- `decision` is one of `"promote"`, `"park"`, `"archive"`.
- `rationale` summarises which honesty-check questions failed.
- `new_issues` is empty for `park` / `archive`; for `promote`, it can contain a draft of the upgraded feature/bug issue.

## Interop

- Always launches with `min_agents = ["claude"]` — the triager subagent is Claude-shaped (consults its skill bundle).
- Headless `team launch default-triager --issues 700,701,702 --yes` runs the three triage decisions in parallel up to `--max-parallel`.

## Inspect

```sh
maestro team explain default-triager
```

## See also

- `subagent-idea-triager` (`.claude/agents/`) — the underlying triage subagent
- [`default-researcher.md`](default-researcher.md) — verdict-only sibling for research
- [`README.md`](README.md) — preset overview
