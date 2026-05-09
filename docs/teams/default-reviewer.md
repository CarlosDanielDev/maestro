# `default-reviewer` — single-pass review team

A one-shot reviewer dispatch. Use against an existing PR, branch, or staged diff when you want a quick `Approved` / `RequestChanges` verdict without spinning up the full coding pipeline.

## Canonical TOML

```toml
extends = ""
primitive = "single-pass"
min_agents = ["claude"]
reviewer = "claude"
```

## Bindings

| Role | Agent | Notes |
|---|---|---|
| reviewer | `claude` | inspects the supplied diff, returns `ReviewFindings { verdict, findings }` |

## When to use

- Sanity-check a PR before merge.
- Audit a long-running branch for drift.
- Validate a release candidate's diff vs `main`.

## Output shape

`PrimitiveOutput::SinglePass { role: Reviewer, result: ReviewFindings { verdict, findings } }`. The verdict is one of `Approved`, `RequestChanges`, or `Reject`. Each finding has a `note` and an optional code location.

## Customise — second-opinion review

```sh
maestro team new dual-review --extends default-reviewer --reviewer opencode
```

Then chain a project-tier override that runs `default-reviewer` for the primary then `dual-review` for the second pass:

```sh
maestro team launch default-reviewer --issue 547 --yes
maestro team launch dual-review     --issue 547 --yes
```

For now this is two separate launches; cross-team chaining is on the roadmap.

## Inspect

```sh
maestro team explain default-reviewer --json
```

## See also

- [`default-coder.md`](default-coder.md) — uses a reviewer step inside its pipeline
- [`README.md`](README.md) — preset overview
