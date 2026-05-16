# End-to-end recipes

Stitch together the pieces from [Extending](02-extending-a-builtin.md) and [Composing](03-composing-from-scratch.md) into runnable flows. Expected outputs are **sketches** — the framing is stable, but exact strings may evolve. Pin to behavior, not to byte-for-byte stdout.

## Recipe 6 — From zero to launched

The headline walkthrough: install fresh, build one preset, launch it on an issue.

### Step 1 — Inspect what's already shipped

```sh
maestro team list
```

Expected sketch:

```
NAME                TIER      PRIMITIVE     ROLES
default-coder       built-in  pipeline      implementer, reviewer, docs
default-researcher  built-in  verdict-only  implementer, reviewer
default-triager     built-in  verdict-only  triager
default-reviewer    built-in  single-pass   reviewer
default-docs        built-in  single-pass   docs
```

Five built-ins, all on `claude`. No user or project presets yet.

### Step 2 — Create the custom preset

```sh
maestro team new ship-it \
  --extends default-coder \
  --implementer opencode \
  --tier user
```

Expected:

```
wrote ~/.config/maestro/maestro/teams/ship-it.toml
```

The file's full contents:

```toml
extends = "default-coder"
implementer = "opencode"
```

That's the entire TOML — the inherited reviewer and docs bindings come from `default-coder` via the resolver, not from this file.

### Step 3 — Confirm the resolved bindings

```sh
maestro team explain ship-it
```

Expected sketch:

```
ship-it (user)
  primitive:   pipeline
  implementer: opencode   (override)
  reviewer:    claude     (inherited from default-coder)
  docs:        claude     (inherited from default-coder)
```

JSON form for scripts:

```sh
maestro team explain ship-it --json
```

Expected JSON sketch:

```json
{
  "name": "ship-it",
  "source_tier": "User",
  "primitive": "pipeline",
  "bindings": {
    "Implementer": { "agent": "opencode" },
    "Reviewer":    { "agent": "claude" },
    "Docs":        { "agent": "claude" }
  }
}
```

### Step 4 — Launch headless on a single issue

```sh
maestro team launch ship-it --issue 123 --yes
```

Expected sketch:

```
plan: 1 issue, 1 level
  level 0: #123
launching #123 with ship-it (pipeline)
  implementer: opencode
  reviewer:    claude
  docs:        claude
#123 → Succeeded (PR #SYNTHETIC-1)
summary: 1/1 succeeded, 0 failed
```

Exit code is `0` only if every issue reaches `Succeeded`. Per-issue failures print on stdout with the failure reason and total counts after the plan summary.

> **v1 caveat.** `team launch --yes` returns **synthetic PR identifiers** (see [`docs/teams/README.md`](../README.md#headless-launch---yes)). The scheduler-to-dispatch wiring is exercised end-to-end, but real worktree + PR creation is still TUI-coupled and lands in a v0.27.x follow-up. Don't script around the literal PR numbers in the output until that lands.

The exact TOML file from Step 2 is mirrored by [`tests/fixtures/teams_cookbook/ship-it.toml`](../../../tests/fixtures/teams_cookbook/ship-it.toml) and validated on every CI run, so the snippet documented here is the snippet the loader accepts.

## Recipe 7 — Multi-issue launch with parallelism

Goal: launch a team on three issues, cap concurrency at 2, respect `## Blocked By` ordering parsed from each issue body.

```sh
maestro team launch default-coder \
  --issues 1,2,3 \
  --yes \
  --max-parallel 2
```

The scheduler builds a DAG from `## Blocked By` fields in each issue, packs the levels under the concurrency cap, and dispatches per level. Issues with no parseable `## Blocked By` (or `None`) are level 0.

Expected sketch with `#2` blocked by `#1`:

```
plan: 3 issues, 2 levels
  level 0: #1, #3       (parallel, max 2)
  level 1: #2           (depends on #1)
```

Exit code is non-zero on any failure. Failed issues print with the reason; subsequent levels don't get to run if dependencies fail.

## Recipe 8 — Tier precedence in practice

Goal: show that a project-tier preset shadows the same name at the user tier.

1. Create the preset at the user tier with one binding shape:

   ```sh
   maestro team new repo-policy-coder --extends default-coder --tier user
   ```

2. Create the same name at the project tier with a different shape (this requires editing the TOML — `team new` would refuse to overwrite at the same tier):

   ```toml
   # <repo>/.maestro/teams/repo-policy-coder.toml
   extends = "default-coder"
   docs = "claude"

   [role_overrides.docs]
   prompt_addendum = "Enforce project ADR conventions. Reject undocumented public APIs."
   ```

3. Verify the project tier wins:

   ```sh
   maestro team explain repo-policy-coder
   ```

   Expected: `repo-policy-coder (project)` — the project file shadows the user file. Resolved bindings include the docs prompt_addendum from the project file.

If you delete the project file, the user file becomes visible again on the next `team list` — no caching.

Fixture: [`tests/fixtures/teams_cookbook/repo-policy-coder.toml`](../../../tests/fixtures/teams_cookbook/repo-policy-coder.toml) (validated as a project-shape preset; the test loads it from a user-tier dir for the smoke run, but the schema is identical).

## See also

- [When to use a team](01-when-to-use-a-team.md) — pick the shape before paying for a launch.
- [Troubleshooting](05-troubleshooting.md) — what to do when `team launch` rejects your team or a role goes unhealthy mid-run.
