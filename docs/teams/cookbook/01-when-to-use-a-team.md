# When to use a team (and when not to)

A team preset is overkill for a quick one-shot bugfix and underkill for a hand-driven scoping session. This page is the decision guide before you spend money launching one.

## TL;DR decision matrix

The four coordination primitives shipped in v1, with their required role bindings and matching built-in (where one exists):

| Primitive | Required roles | Built-in | Use when | Avoid when |
|---|---|---|---|---|
| `pipeline` | implementer → reviewer → docs | [`default-coder`](../default-coder.md) | implementing a feature or bugfix end-to-end with review and docs in one issue lifecycle | the issue body is a question, not a change request |
| `verdict-only` | reviewer | [`default-researcher`](../default-researcher.md), [`default-triager`](../default-triager.md) | producing a structured verdict (recommendation, classification) with no code change | you actually need a PR — there will be no commit |
| `single-pass` | _(none required)_ | [`default-reviewer`](../default-reviewer.md), [`default-docs`](../default-docs.md) | a one-shot pass — review the existing branch, refresh the docs, no orchestration loop | you need multi-step recovery (review → fix → re-review) |
| `fan-out` | reviewer | _(no built-in — compose from scratch, see [Recipe 4](03-composing-from-scratch.md))_ | running N reviewers in parallel against the same input and merging the verdicts | a single reviewer would already answer the question |

Required-role enforcement lives in `src/orchestration/types.rs::Primitive::required_roles` — if a primitive's required role is unbound the preset fails pre-flight, no launch.

## Single-agent vs team

Use a plain `claude` invocation (no team) when:

- The work is a one-shot pass and you'd discard a multi-step orchestration loop anyway.
- You're iterating in the TUI and a single subagent is doing the whole job.
- You don't need the cost of a separate reviewer for this issue.

Reach for a team when:

- You want enforced separation between implementer, reviewer, and docs — the pipeline gives you that for free.
- You want a verdict you can attach to an issue without churning a PR — `verdict-only` is the cheaper shape.
- You want to swap one role onto a cheaper provider without losing the rest of the loop — `extends` plus an override does that in two lines of TOML (see [Recipe 1](02-extending-a-builtin.md)).

## Cost intuition

The spec locks the cost-formula constants in `docs/superpowers/specs/2026-05-05-orchestration-wizard-design.md` §7. The formula:

```
estimate_tokens(team, primitive, num_issues) =
    num_issues * (
        L2_system_prompt_tokens                    # ~200, computed at team resolution
      + sum_over_roles(role_system_prompt_tokens)  # from [modes.*]
      + AVG_ISSUE_CONTEXT_TOKENS_PER_PROVIDER      # static const, default 800
      + 300 * num_required_roles * RECOVERY_BUDGET # RECOVERY_BUDGET = 3 (max attempts)
    )
```

The plan preview renders the dollar figure as **"≈ $X.XX (rough estimate, ±50%)"** — explicitly approximate. Treat it as a sanity check, not a quote.

What the constants mean in practice:

- A single-issue `pipeline` run multiplies the recovery budget by 3 required roles (implementer + reviewer + docs) → `300 * 3 * 3 = 2700` extra tokens budgeted against retries. That's the price of being able to recover.
- A single-issue `verdict-only` run has 1 required role → `300 * 1 * 3 = 900` retry tokens. Roughly a third of pipeline's recovery cost.
- A three-issue `fan-out` (composed; no built-in) with one reviewer multiplies by `num_issues`, so 3× the per-issue cost.

The dollar value depends on which provider each role binds to — `COST_PER_TOKEN[Ollama] = 0.0`, so a cheap-stack preset that pins everything to Ollama produces a near-zero estimate.

## See also

- [`default-coder`](../default-coder.md) — pipeline pre-bound to `claude` for every role.
- [`default-researcher`](../default-researcher.md) — verdict-only with implementer + reviewer for scoping work.
- [`default-triager`](../default-triager.md) — verdict-only with a triager role for idea-inbox classification.
- [`default-reviewer`](../default-reviewer.md) — single-pass reviewer for one-shot branch reviews.
- [`default-docs`](../default-docs.md) — single-pass docs for refresh-only runs.
- [Extending a built-in](02-extending-a-builtin.md) — once you've picked a shape, swap roles to taste.
