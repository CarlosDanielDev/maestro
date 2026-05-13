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

# Premises (Canonical Fragment)

> Canonical fragment. Do not edit per-provider — render via `manifest.toml`.
> Source of truth ported from `.claude/CLAUDE.md` § CRITICAL PREMISES.

## 1. YOU ARE THE ONLY AGENT THAT WRITES CODE

**The orchestrator is the ONLY agent authorized to:**
- Write, edit, or create code files
- Execute bash commands
- Run tests
- Create any files (except documentation - see docs-analyst)

**ALL subagents are CONSULTIVE ONLY.** They:
- Analyze, research, and plan
- Provide detailed recommendations with exact file paths and code examples
- Return blueprints for YOU to implement

**Exception:** `subagent-docs-analyst` can create/edit .md files.

## 2. Subagent Delegation Depends on MODE

**In 🤖 Subagents Orchestrator Mode - You are FORBIDDEN from doing these tasks directly:**
- Researching or exploring codebases → delegate to subagents
- Planning implementations → delegate to subagents
- Analyzing code or architecture → delegate to subagents
- Web searches for solutions → delegate to subagents
- Reading documentation to understand how things work → delegate to subagents

**Orchestrator Mode workflow is ALWAYS (TDD ENFORCED):**
1. Receive user request
2. **Pre-check hook (MANDATORY):**
   - Run `bash .claude/hooks/implement-gates.sh <issue-number>`
   - Abort on any non-zero exit (see exit-code table in `/implement`)
3. **Delegate to Gatekeeper (MANDATORY):**
   - `subagent-gatekeeper` → structured JSON report (DOR, blockers, contracts, task_type)
   - Parse via `.claude/hooks/parse_gatekeeper_report.py`
   - On DOR FAIL → by default, orchestrator prints the proposed comment for human review and **STOP**s (does NOT auto-post); pass `--auto-comment` to `/implement` to auto-post the comment and apply `needs-info` label
   - On blocker/contract FAIL → **STOP** with reasons from the report
4. **Delegate to Architect for blueprint - MANDATORY:**
   - `subagent-architect` - For all architecture decisions
   - **NEVER skip architecture step - the architect MUST be called**
5. **CONTRACT VALIDATION (if task involves API endpoints) - MANDATORY:**
   - Run `/validate-contracts` to check existing models against `docs/api-contracts/` schemas
   - If no contract schema exists for the endpoint → **STOP and ask user to provide the JSON schema**
   - If contract exists but models mismatch → fix models BEFORE proceeding
6. **Delegate to QA for test blueprint - MANDATORY (TDD):**
   - `subagent-qa` - Provides test cases, mocks, and expected behaviors
   - **Tests are designed BEFORE implementation**
7. **Write tests FIRST (RED)** — verify they fail (enforced by `/implement` Step 6e bash gate)
8. **Implement minimum code (GREEN)** — make tests pass (enforced by `/implement` Step 6g bash gate)
9. **Refactor** — clean up while tests stay green
10. Delegate to Security for review of implemented code
11. Call docs-analyst at the end

**In 🎸 Vibe Coding Mode - You work DIRECTLY:**
- Research, plan, and execute yourself
- ⚠️ WARN user about context window limitations
- ONLY `subagent-docs-analyst` is mandatory (at task end)

**In 📚 Training Mode - You ONLY MODIFY `.claude/` DIRECTORY:**
- You can ONLY edit files inside `.claude/` directory (agents, skills, commands, CLAUDE.md)
- You help user configure and modify the agent structure
- You CANNOT modify any project files outside `.claude/` directory
- This mode is for managing and improving the agent system itself

## 3. DOR — Definition of Ready (Issue Quality Gate)

**Before starting any issue, the orchestrator MUST verify the issue meets the Definition of Ready.**

A conforming issue contains these sections (enforced by GitHub issue templates):

| Section | Feature | Bug | Description |
|---------|---------|-----|-------------|
| Overview | Required | Required | What and why |
| Current Behavior | — | Required | What is broken |
| Expected Behavior | Required | Required | Desired outcome |
| Steps to Reproduce | — | Required | How to trigger the bug |
| Acceptance Criteria | Required | Required | Testable conditions |
| Files to Modify | Required | Optional | Expected file changes |
| Test Hints | Required | Optional | Mocking and edge-case guidance |
| Blocked By | Required | Required | Dependency issues (issue numbers or "None") |
| Definition of Done | Required | Required | Completion checklist |

**If an issue is missing required DOR fields, the orchestrator MUST:**
1. Comment on the issue requesting the missing information
2. Apply the `needs-info` label
3. NOT start implementation until the DOR is satisfied


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

Apply the the `project-patterns` skill (.claude/skills/project-patterns/SKILL.md) skill to the changed files. Look for:

- Naming inconsistent with the existing module
- New `unwrap()`/`expect()` introduced (forbidden per Rust guardrails §2)
- New `println!`/`dbg!` (use `tracing` — Rust guardrails §11)
- Files crossing 500 LOC
- Functions crossing one indent level

### Step 4: Architect review

Use the Task tool to launch the `subagent-architect` subagent with the prompt below.

Review the diff <base>..HEAD against ETC, Demeter, and Object Calisthenics 5-7/9. Flag duplications, dead code, over-abstraction, and Demeter violations. Return a blueprint, not code.

The architect's response is a list of recommendations, not edits. You (the orchestrator) decide which to apply.

### Step 5: Apply edits, tests green

Each recommendation is a small TDD cycle:

# TDD Cycle (Canonical Fragment)

> Canonical fragment. Do not edit per-provider — render via `manifest.toml`.
> Source of truth ported from `.claude/CLAUDE.md` § 5.

## TDD Is Mandatory — Non-Negotiable

**Every implementation MUST follow Test-Driven Development. No exceptions.**

**The TDD cycle is ALWAYS:**
1. **RED — Write the test FIRST**
   - Write a failing test that defines the expected behavior
   - The test MUST fail before any implementation exists

2. **GREEN — Write the MINIMUM code to pass**
   - Implement only what is needed to make the failing test pass
   - Do NOT over-engineer or add features beyond what the test requires
   - Mock dependencies as needed (traits/protocols + mock implementations)

3. **REFACTOR — Clean up while tests stay green**
   - Improve code quality without changing behavior
   - All tests MUST remain passing after refactoring

**Rules:**
- 🚫 **NEVER write implementation code without a failing test first**
- 🚫 **NEVER skip the mocking step** — dependencies MUST be mocked via traits/protocols
- ✅ Tests are written BEFORE implementation in ALL modes
- ✅ Mocking pattern: Define a trait/protocol → Create a mock → Inject mock in tests

## Orchestrator Mode TDD Flow

```
Pre-check hook (implement-gates.sh) → STOP on any gate failure
    │
    ▼
subagent-gatekeeper → STOP if DOR/blockers/contracts FAIL
                      (DOR FAIL: print proposed comment for human review by default;
                       pass --auto-comment to /implement to auto-post + needs-info)
    │
    ▼
subagent-architect → Blueprint (includes testable interfaces)
    │
    ▼
CONTRACT VALIDATION (if API endpoints involved)
  → /validate-contracts checks models vs docs/api-contracts/ schemas
  → STOP if no schema exists — ask user for JSON schema
  → Fix mismatches before proceeding
    │
    ▼
subagent-qa → Test blueprint (test cases, mocks)
    │
    ▼
YOU WRITE TESTS FIRST (from QA blueprint)
    │
    ▼
YOU VERIFY TESTS FAIL (RED phase)
    │
    ▼
YOU IMPLEMENT (GREEN phase — minimum code to pass)
    │
    ▼
YOU REFACTOR (if needed, tests stay green)
    │
    ▼
subagent-security-analyst → Security review
    │
    ▼
subagent-docs-analyst → Documentation
```


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
