# Troubleshooting and honest scope

What to do when things break, plus the v1 limitations to keep in mind before you hit them.

## Loader errors

Every message below is emitted verbatim by `src/orchestration/loader.rs`. Quoting them when filing bugs makes the issue greppable.

| Error fragment | Meaning | Fix |
|---|---|---|
| `team "<name>": primitive not set anywhere in extends chain` | The walk from leaf to root never found a `primitive = "..."` line. | Set `primitive` on the root preset (the one with `extends = ""`). |
| `extends cycle detected: a → b → a` | An `extends` chain loops back on itself. | Edit one of the offending presets to break the cycle. |
| `team "<name>": unknown role binding "<role>"` | A top-level key looks like a role binding but isn't one of the seven `TeamRole` variants. | Check spelling — valid roles are `implementer`, `reviewer`, `docs`, `devops`, `orchestrator`, `triager`, `researcher`. |
| `team "<name>" extends missing parent "<parent>"` | `extends = "parent"` but no preset named `parent` resolves. | Confirm the parent exists at some tier (`maestro team list`). |
| `preset name "<name>" must not be empty` | A `--name ""` slipped through. | Pick a non-empty name. |
| `preset name "<name>" is N chars, max 64` | Name is too long. | Shorten to ≤ 64 characters. |
| `preset name "<name>" cannot start with '.' (would shadow a hidden file)` | Leading dot. | Drop the dot. |
| `preset name "<name>" cannot start with '-' (would parse as a CLI flag)` | Leading dash. | Drop the dash. |
| `preset name "<name>" contains illegal path characters` | Name contains `/`, `\`, NUL, or `..`. | Pick a name with `[a-z0-9-]` only. |
| `team file "<path>" is N bytes, exceeds the 1048576-byte cap` | TOML file is > 1 MiB. | If you wrote a real preset that big, the schema isn't your problem — start over. |

## Pre-flight failures

These fire after the loader succeeds but before any dispatch runs:

- **Missing required role binding.** Each primitive's required roles are listed in [Composing from scratch](03-composing-from-scratch.md#schema-cheat-sheet). A pipeline without a reviewer fails here, not at load time.
- **Agent in `min_agents` is unhealthy or unconfigured.** `min_agents = ["claude"]` requires `[agents.claude]` to exist in `maestro.toml` and pass health check. Pre-flight calls `run_health_check()` as a library function.
- **Issue body has unparseable `## Blocked By`.** The launcher needs a parseable `## Blocked By` (or `None`) for each issue; a missing or malformed block aborts pre-flight with the issue number and what the parser saw.

The wizard hides teams that fail pre-flight rather than showing them as launchable; the CLI prints the diagnostic and exits non-zero.

## Runtime failure modes

- **`model_override` returns "model not found" at runtime.** L1 dispatch surfaces the provider's error in the structured result; the orchestrator retries up to `RECOVERY_BUDGET` (3) attempts. After exhaustion, the issue is marked `Failed` with the provider's message preserved. Either correct the `model_override` to a model the provider actually serves, or remove the override and let the role's mode pick the default.
- **A bound agent is unhealthy.** The wizard's pre-flight hides the team, so you won't see this at launch — but if an agent goes unhealthy between pre-flight and dispatch, L1 will surface the failure with the agent ID in the message. Run `maestro doctor` to inspect agent health.

## Honest scope (v1)

What v1 explicitly does NOT do, per `docs/superpowers/specs/2026-05-05-orchestration-wizard-design.md` §13 and the surrounding sections:

- **L2 is Claude-only.** The per-issue L2 orchestrator system call always runs on `claude`. You can bind non-Claude agents to roles (implementer, reviewer, docs, etc.), but the orchestrator layer that schedules them stays on Claude in v1 — that's why `min_agents` always includes `"claude"`.
- **No auto-merge / no merge-train.** Teams produce PRs but do not coordinate their merges. PRs land via the existing per-session merge queue, which serializes them; conflicts at merge time are resolved manually. A team-aware merge-train scheduler is explicitly out of v1 scope.
- **No `FileSet` `TeamInput`.** v1 accepts `Issue { number }`, `IssueSet { primary_milestone, issues }`, and `IdeaInbox`. You cannot launch a team against a file path or glob.
- **No preset marketplace / no remote presets.** All presets live on disk in one of the three tiers (built-in, user, project). There is no `maestro team install <url>` and there will not be one in v1.
- **No team memory across runs.** Each launch starts fresh. Teams do not share state, prompts, or learned heuristics between invocations.
- **No real-time team coordination.** Subagents within a team do not talk to each other mid-run. The L2 orchestrator owns coordination; subagents are stateless from each other's point of view.
- **No cross-machine teams.** Teams run on the local machine in a single process. No SSH, no remote agents.
- **No auto-tuning bindings.** The cost estimator does not feed back into role selection. If a binding is wasteful, the user changes it.
- **Headless `launch --yes` returns synthetic PR identifiers.** Real worktree + PR creation is still TUI-coupled in v1 — see the [headless-launch note in `README.md`](../README.md#headless-launch---yes).
- **CLI `team new` only exposes `--implementer`, `--reviewer`, `--docs`.** The other four roles (`devops`, `orchestrator`, `triager`, `researcher`) require editing the TOML directly. The schema supports all seven; the CLI doesn't shortcut all of them.
- **No built-in for the `fan-out` primitive.** The primitive is supported by the engine but no preset ships with it in v1. Compose from scratch (see [Recipe 4](03-composing-from-scratch.md#recipe-4--a-fan-out-reviewer-team-no-built-in-for-this-primitive)).
- **No versioned `extends`.** `extends = "default-coder@v1"` is not supported in v1. Built-in changes that affect downstream `extends` are flagged as breaking in `CHANGELOG.md`; the loader emits a `tracing::warn!` on every load that inherits from a built-in. Versioned pinning is deferred to v2.

## See also

- [`README.md`](../README.md) — the system index, CLI surface table, state migration.
- [Spec §13](../../superpowers/specs/2026-05-05-orchestration-wizard-design.md) — the canonical out-of-scope list.
- [`default-coder`](../default-coder.md) and the other four per-built-in pages — what each preset binds and when to extend it.
