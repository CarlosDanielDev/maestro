---
command: plan-feature
version: 1.0.0
description: Plan a feature across the project, creating API contracts first, then milestones and issues with dependency tracking.
placeholders:
  - INCLUDE
  - SUBAGENT_LIST
includes:
  - core/premises.md
  - core/dependency-graph.md
source_provenance:
  ported_from: .claude/commands/plan-feature.md
  ported_at: 2026-05-13
---

# Plan Feature

Plan a feature across your project, creating API contracts first, then milestones and issues with dependency tracking.

**Usage:** `/plan-feature <description>` or `/plan-feature`

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

## Arguments

`$ARGUMENTS` contains the user's natural language description of the feature.

If no arguments, ask: "Describe the feature you want to build."

---

## Instructions

### Phase 1: ANALYZE — Understand the Feature

1. **Parse the description** to identify:
   - Feature goals
   - Which systems/modules are involved
   - What data flows exist (APIs, events, etc.)

2. **Explore the codebase** to find:
   - Existing models, services, and components related to the feature
   - Existing API endpoints or contracts
   - Architecture patterns already in use

   Available subagents for delegation: | Subagent | Purpose |
|----------|---------|
| `subagent-gatekeeper` | DOR, blockers, and API-contract gate for `/implement` |
| `subagent-architect` | Architecture design and implementation planning |
| `subagent-qa` | QA engineering, test design, quality gates |
| `subagent-security-analyst` | Security review (OWASP Top 10) |
| `subagent-docs-analyst` | Documentation management (only subagent allowed to write `.md`) |
| `subagent-master-planner` | System architecture planning, ADRs |
| `subagent-idea-triager` | Idea-inbox triage gate (5-question honesty check)

3. **Present analysis** to the user with what exists vs. what's needed

### Phase 2: CONTRACTS FIRST — Create API Schemas

For every API endpoint identified:
1. Check if a contract already exists in `docs/api-contracts/`
2. If not, create one following `api-contract-v1` format
3. Present contracts to user for review

### Phase 3: MILESTONES — Create if Needed

If the feature is large enough for a milestone:
```bash
gh api repos/<owner>/<repo>/milestones -f title="..." -f state=open -f description="..."
```

### Phase 4: ISSUES — Create with Traceability

Every issue MUST contain:
1. **Context** — Why this issue exists
2. **API Contract** reference (if endpoint involved)
3. **Requirements** — What to build
4. **TDD Checklist** — Tests to write
5. **Acceptance Criteria** — Checkbox list
6. **Dependencies** — What blocks this / what this blocks

Create in dependency order:
1. Contract/schema issues first
2. Foundation issues (blocked by contracts)
3. Feature issues (blocked by foundation)
4. Capstone issue (blocked by all)

### Phase 5: DEPENDENCY GRAPH — Present Final Plan

Output:
```
## Implementation Order

### Phase 1: Foundation
- #XX: <title>

### Phase 2: Core Features (parallelizable)
- #XX: <title> (blocked by #YY)
- #XX: <title> (blocked by #YY)

### Phase 3: Integration
- #XX: <title> (blocked by all above)
```

The dependency-graph format and per-issue `## Blocked By` rules are canonical:

# Dependency Graph (Canonical Fragment)

> Canonical fragment. Do not edit per-provider — render via `manifest.toml`.
> Source of truth ported from `.claude/CLAUDE.md` § 4.

## Dependency Chain and Graph — Non-Negotiable

**Every issue and milestone MUST include dependency information. No exceptions. This applies in ALL modes (Subagents Orchestrator, Vibe Coding, Training).**

**Rules for Issues:**
- Every `gh issue create` call MUST include a `## Blocked By` section with issue numbers (or "None")
- This field is REQUIRED, not optional
- Format:
  ```markdown
  ## Blocked By

  - #106 feat: sanitize module scaffolding
  - #107 feat: Phase 1 scanner
  ```
  Or if no dependencies:
  ```markdown
  ## Blocked By

  - None
  ```

**Rules for Milestones:**
- Every `gh api milestones` create/update MUST include a `## Dependency Graph` in the description
- The dependency graph MUST use ASCII visualization showing the execution order
- Required sections in milestone description:
  1. A one-line summary
  2. A `## Dependency Graph (Implementation Order)` section with levels (Level 0, Level 1, etc.)
  3. A `Sequence:` line showing the linear/parallel execution order using `→` (sequential) and `∥` (parallel)
- Format:
  ```markdown
  ## Dependency Graph (Implementation Order)

  Level 0 — no dependencies:
  • #106 feat: scaffolding and types

  Level 1 — depends on #106 (can run in parallel):
  • #107 feat: Phase 1 scanner
  • #108 feat: Phase 2 analyzer

  Level 2 — depends on #107, #108:
  • #110 feat: Wire pipeline

  Sequence: #106 → #107 ∥ #108 → #110
  ```

**Violation of this rule means the issue/milestone is malformed and MUST be corrected before proceeding.**

**Rules for Milestone Updates After Issue Closure (MANDATORY):**
- When an issue is closed, its entry in the milestone dependency graph MUST be updated with ✅
- Change `• #NNN` to `• ✅ #NNN` in the milestone description
- If ALL issues in a level are now ✅, mark the level header as `(COMPLETED ✅)`
- Update the `Sequence:` line to reflect completed levels with `✅(LN)`
- This is done via `gh api repos/<owner>/<repo>/milestones/<number> -X PATCH -f description="..."`
- **This is NON-NEGOTIABLE.** Every closed issue MUST be reflected in the milestone graph immediately after closure. Skipping this step is a violation.


---

## Error Handling

- If `gh` CLI fails → suggest `gh auth login`
- If description is too vague → ask clarifying questions
- If contract already exists → reuse it
- If milestone exists → reuse it
