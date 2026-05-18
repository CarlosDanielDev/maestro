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
   - Run `bash .maestro/hooks/implement-gates.sh <issue-number>`
   - Abort on any non-zero exit (see exit-code table in `/implement`)
3. **Delegate to Gatekeeper (MANDATORY):**
   - `subagent-gatekeeper` → structured JSON report (DOR, blockers, contracts, task_type)
   - Parse via `.maestro/hooks/parse_gatekeeper_report.py`
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
