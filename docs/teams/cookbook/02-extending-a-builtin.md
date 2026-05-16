# Extending a built-in preset

The fastest path to a custom team is to inherit from a built-in and override the bindings you care about. `extends` is the load-bearing keyword.

## What `extends` means

In any preset TOML:

- `extends = ""` — root preset. Defines its own `primitive`, `min_agents`, and role bindings.
- `extends = "<parent>"` — inherits from the named parent. The merge walks root → leaf, **leaf wins per binding** (see `src/orchestration/loader.rs::Loader::resolve`).
- Inheritance chains are walked with cycle detection — a circular `extends` is rejected at load time with a `cycle detected: a → b → a` error.

Tier precedence is independent of `extends`: when the same preset name appears at multiple tiers, **project (`<repo>/.maestro/teams/`) > user (`~/.config/maestro/...`) > built-in**.

## Recipe 1 — Swap one role onto a cheaper provider

Goal: take `default-coder`, point the implementer at a cheaper provider, leave reviewer and docs on `claude`.

```sh
maestro team new cheap-coder \
  --extends default-coder \
  --implementer opencode \
  --tier user
```

Written file at `~/.config/maestro/maestro/teams/cheap-coder.toml`:

```toml
extends = "default-coder"
implementer = "opencode"
```

That's the entire file. Reviewer and docs aren't mentioned — they resolve to `claude` through the inherited `default-coder` bindings.

Verify with `maestro team explain cheap-coder`:

```
cheap-coder (user)
  primitive:   pipeline
  implementer: opencode   (override)
  reviewer:    claude     (inherited from default-coder)
  docs:        claude     (inherited from default-coder)
```

Fixture: [`tests/fixtures/teams_cookbook/cheap-coder.toml`](../../../tests/fixtures/teams_cookbook/cheap-coder.toml) — validated by `cargo test --test teams_cookbook_fixtures` on every CI run.

## Recipe 2 — Add a prompt_addendum without changing the agent

Goal: keep `default-coder`'s reviewer on `claude` but tighten its prompt with a project-specific addendum. The minimal form (`reviewer = "..."`) only takes an agent ID; for richer overrides use the `[role_overrides.<role>]` table.

```toml
extends = "default-coder"

[role_overrides.reviewer]
prompt_addendum = "Apply strict style enforcement: no magic numbers, all public items documented."
```

`role_overrides` keys (from `src/orchestration/team.rs::RoleOverride`):

- `agent` — replace the bound agent (same effect as `reviewer = "..."` at top level).
- `mode` — override the mode the role runs under.
- `model_override` — pin the role to a specific model on the bound provider.
- `prompt_addendum` — extra text appended to the role's system prompt.
- `fallback_agent` — agent to use if the primary fails the L1 dispatch.

Fixture: [`tests/fixtures/teams_cookbook/strict-reviewer.toml`](../../../tests/fixtures/teams_cookbook/strict-reviewer.toml).

## Recipe 3 — Project-tier preset checked into the repo

Goal: the team config lives next to the code so every contributor on this repo uses it. Use `--tier project`:

```sh
maestro team new repo-policy-coder \
  --extends default-coder \
  --tier project
```

The file lands at `<repo>/.maestro/teams/repo-policy-coder.toml`. Add a docs prompt addendum by editing the TOML directly (the CLI `new` flags don't cover addenda):

```toml
extends = "default-coder"
docs = "claude"

[role_overrides.docs]
prompt_addendum = "Enforce project ADR conventions. Reject undocumented public APIs."
```

If both `~/.config/maestro/maestro/teams/repo-policy-coder.toml` and `<repo>/.maestro/teams/repo-policy-coder.toml` exist, the project tier wins. `maestro team explain` prints the resolved tier in the header.

Fixture: [`tests/fixtures/teams_cookbook/repo-policy-coder.toml`](../../../tests/fixtures/teams_cookbook/repo-policy-coder.toml).

## What the CLI cannot do for you

`maestro team new` only exposes `--implementer`, `--reviewer`, and `--docs` overrides (see `src/cli_team.rs::TeamSubcommand::New`). To bind the other four roles — `devops`, `orchestrator`, `triager`, `researcher` — edit the TOML directly. The schema accepts any of the seven role keys from `TeamRole`, the CLI is just a convenience for the three common ones.

Same applies to `role_overrides`: the CLI only emits the minimal form. Use a text editor for anything richer.

## See also

- [`default-coder`](../default-coder.md) — the parent for every recipe on this page.
- [Composing from scratch](03-composing-from-scratch.md) — for primitives other than pipeline.
- [Recipes](04-recipes.md) — the end-to-end `team new` → `team launch --yes` walkthrough.
