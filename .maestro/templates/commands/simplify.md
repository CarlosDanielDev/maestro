---
command: simplify
version: 1.0.0
description: Review changed code for reuse, quality, and efficiency; remove duplication, dead code, and over-abstraction without breaking tests.
placeholders:
  - INCLUDE
  - INVOKE_SUBAGENT
  - SKILL
includes:
  - core/premises.md
  - core/tdd-cycle.md
source_provenance:
  ported_from: new
  ported_at: 2026-05-13
---

# Simplify

Review the working-tree diff against the project's design philosophy and remove what does not earn its place.

**Usage:** `/simplify` (defaults to `main..HEAD`) or `/simplify <base-ref>`

{{INCLUDE path="core/premises.md"}}

---

## When to Use

- After a feature is GREEN but before `/pushup`.
- After a large refactor, to confirm no new coupling or duplication slipped in.
- When the diff "feels long" — the heuristic is wrong more often than right, and a structured pass catches what intuition misses.

## Non-Goals

- This is NOT a security review (that is handled by `security-analyst` in `/implement` step 6i).
- This is NOT a test-coverage review (that is handled by `qa`).
- This does NOT introduce new abstractions; it removes premature ones.

## Design Principles (the rubric)

Three lenses, applied in order:

1. **ETC (Easy To Change)** — would this be easy to change if the next requirement shifted? Each binding to a concrete implementation is a coupling point; flag it.
2. **Law of Demeter** — count the dots. Method chains like `app.pool.sessions[0].status.label()` are a code smell. Push behaviour into the owner, not the caller.
3. **Object Calisthenics (5-7 of 9)** — aspirational compass, not dogma. Score the diff:
   - One level of indentation per method
   - No `else` keyword (early returns / match)
   - Wrap primitives that carry domain meaning
   - First-class collections (`SessionPool` wraps `Vec<Session>`)
   - One dot per line
   - No abbreviations
   - Keep entities small (< 100 lines per impl, < 500 lines per file)
   - Two instance variables or fewer
   - No getters/setters — expose behaviour

   Flag any score below 5/9 as a design smell worth fixing now.

## Workflow

### Step 1: Scope the diff

`git diff --stat <base>..HEAD` to see what changed. If the diff touches more than ~15 files or ~800 lines, recommend splitting the review by directory.

### Step 2: Mechanical rot checks

Run, in order:

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --quiet`

If any of these fail, **STOP** — simplify is for clean diffs, not broken ones. Fix the basics first.

### Step 3: Quality skills pass

Apply the {{SKILL name="project-patterns"}} skill to the changed files. Look for:

- Naming inconsistent with the existing module
- New `unwrap()`/`expect()` introduced (forbidden per Rust guardrails §2)
- New `println!`/`dbg!` (use `tracing` — Rust guardrails §11)
- Files crossing 500 LOC
- Functions crossing one indent level

### Step 4: Architect review

{{INVOKE_SUBAGENT name="architect" prompt="Review the diff <base>..HEAD against ETC, Demeter, and Object Calisthenics 5-7/9. Flag duplications, dead code, over-abstraction, and Demeter violations. Return a blueprint, not code."}}

The architect's response is a list of recommendations, not edits. You (the orchestrator) decide which to apply.

### Step 5: Apply edits, tests green

Each recommendation is a small TDD cycle:

{{INCLUDE path="core/tdd-cycle.md"}}

After each edit, re-run `cargo test --quiet`. If a simplification breaks a test, **the test is the spec** — either the simplification was wrong, or the test was over-fitted to the old shape. Decide explicitly which.

### Step 6: Document the trade-off

For every non-trivial change (renaming a type, collapsing two functions, removing a layer), append a one-line entry to the PR body under `## Simplifications`:

```
- Collapsed `SessionStatus::label()` + `SessionStatus::short_label()` into one method (Demeter, ETC).
```

This makes the review's intent visible to PR reviewers.

## Error Handling

- If `cargo test` fails before starting → STOP, fix baseline (same Gate 2 as `/implement`).
- If the architect flags more than ~5 issues → split into a fresh issue and run `/implement` on it. `/simplify` is for low-risk passes, not full refactors.
- If a simplification cannot keep tests green → revert it and document the constraint in the PR body.

## Do Not

- Introduce new abstractions during simplify.
- Touch files outside the diff under review.
- Skip the mechanical rot checks (Step 2) — they're cheap and catch 80% of issues.
