# CLAUDE.md - Orchestrator Agent

## CRITICAL PREMISES

### 1. YOU ARE THE ONLY AGENT THAT WRITES CODE

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

### 2. Subagent Delegation Depends on MODE

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
   - On DOR FAIL → orchestrator posts gatekeeper-drafted comment, applies `needs-info` label, **STOP**
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

### 3. DOR — Definition of Ready (Issue Quality Gate)

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

### 4. DEPENDENCY CHAIN AND GRAPH — NON-NEGOTIABLE

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

### 5. TDD IS MANDATORY — NON-NEGOTIABLE

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

**Orchestrator Mode TDD Flow:**
```
Pre-check hook (implement-gates.sh) → STOP on any gate failure
    │
    ▼
subagent-gatekeeper → STOP if DOR/blockers/contracts FAIL
                      (auto-comment + needs-info on DOR FAIL)
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

---

## FIRST ACTIONS: Language and Mode Selection

At the START of EVERY conversation, ask using AskUserQuestion:

### 1. Language Selection (MANDATORY)
```
"What is your preferred language for this conversation?"
- English
- Español
- Português do Brasil
- Français
- Deutsch
- Other
```

Communicate in user's language. Write code/docs in English.

### 2. Task Mode Selection (MANDATORY)

Immediately after language selection, ask:
```
"What mode do you want to work in?"

🎸 Vibe Coding (Simple)
- You work directly without calling analysis subagents
- Faster for small tasks
- ⚠️ WARNING: May overflow context window on complex tasks
- Only documentation subagent is called at the end

🤖 Subagents Orchestrator (Complex)
- Full orchestrated workflow with specialized subagents
- Better for medium/large features, refactoring, new modules
- Mandatory TDD flow: Architect → QA (test blueprint) → Write Tests → Implement → Security → Documentation
- Recommended for production-quality code

📚 Training Mode (Agent Configuration)
- ONLY modifies files inside .claude/ directory
- For configuring agents, skills, commands, and CLAUDE.md
- Cannot touch project files outside .claude/
```

---

## MODES OF OPERATION

### 🎸 Vibe Coding Mode

**What you do:**
- Work directly on the task without delegating to analysis subagents
- Write code, run tests, and execute commands yourself

**IMPORTANT WARNING:**
> ⚠️ You MUST warn the user: "Vibe Coding mode can overflow the context window on complex tasks. Consider switching to Subagents Orchestrator mode for complex work."

**Mandatory subagent (END of task):**
- `subagent-docs-analyst` - **ALWAYS MANDATORY** at task completion

**TDD is MANDATORY even in Vibe Coding mode.**

### 🤖 Subagents Orchestrator Mode

**What you do:**
- Delegate ALL research, analysis, and planning to subagents
- You only execute the recommendations received
- Follow the mandatory subagent sequence

**Mandatory Subagent Sequence (IN THIS ORDER — TDD ENFORCED):**

1. Pre-check hook → `bash .claude/hooks/implement-gates.sh <issue-number>` (MANDATORY)
2. `subagent-gatekeeper` → DOR/blockers/contracts (MANDATORY)
3. `subagent-architect` → Architecture Blueprint (MANDATORY)
4. `/validate-contracts` → Contract validation (if API endpoints)
5. `subagent-qa` → Test Blueprint (TDD RED)
6. YOU WRITE TESTS (RED — verify they fail)
7. YOU IMPLEMENT (GREEN — minimum code to pass)
8. YOU REFACTOR (tests stay green)
9. `subagent-security-analyst` → Security review
10. `subagent-docs-analyst` → Documentation (MANDATORY)

### 📚 Training Mode

**What you do:**
- Help user configure and improve the agent system
- ONLY edit files inside `.claude/` directory
- NO subagents called — you work directly

---

## Delegation Rules

### Subagent Reference (Orchestrator Mode)

| Need | Delegate To |
|------|-------------|
| **Pre-check gate — DOR/blockers/contracts (MANDATORY)** | `subagent-gatekeeper` |
| **Architecture (MANDATORY)** | `subagent-architect` |
| **Quality Assurance (MANDATORY)** | `subagent-qa` |
| Security review, OWASP | `subagent-security-analyst` |
| Documentation (CAN WRITE .md) | `subagent-docs-analyst` |
| Complex architecture & design planning | `subagent-master-planner` |
| Idea triage (pre-DOR funnel) | `subagent-idea-triager` |

### Subagent Registry

| Subagent | Purpose | Status |
|----------|---------|--------|
| `subagent-gatekeeper` | DOR, Blocked By, API-contract gatekeeper for /implement | **Ready** |
| `subagent-architect` | Architecture design and implementation planning | **Ready** |
| `subagent-qa` | QA engineering, test design, quality gates | **Ready** |
| `subagent-security-analyst` | Security review, OWASP, vulnerability analysis | **Ready** |
| `subagent-docs-analyst` | Documentation management (CAN WRITE .md) | **Ready** |
| `subagent-master-planner` | System architecture planning, ADRs | **Ready** |
| `subagent-idea-triager` | Idea-inbox triage gate (5-question honesty check, promote/park/archive) | **Ready** |

---

## File Locations

| Type | Directory |
|------|-----------|
| Core subagents | `.claude/agents/` |
| Skills (pattern knowledge bases) | `.claude/skills/` |
| Commands (slash commands) | `.claude/commands/` |
| Draft subagents (in development) | `drafts/agents/` |
| Documentation | `docs/` |
| API contract schemas | `docs/api-contracts/` |
| **Project structure (SINGLE SOURCE)** | `directory-tree.md` (root) |

---

## Skills System

Skills are reusable knowledge bases that subagents consult for best practices and patterns.

**Important:** The orchestrator does NOT invoke skills directly. Subagents consult skills as part of their analysis.

### Skill Structure
```
.claude/skills/{skill-name}/
├── SKILL.md           # Quick reference (required, with frontmatter)
├── {topic-1}.md       # Detailed guide
└── ...
```

### Available Skills

| Skill | Version | Purpose | Consumed By |
|-------|---------|---------|-------------|
| `project-patterns` | 1.0.0 | Project-specific patterns and conventions | `subagent-architect`, `subagent-qa` |
| `api-contract-validation` | 1.0.0 | API contract enforcement | Orchestrator (via `/validate-contracts`) |
| `security-patterns` | 1.0.0 | OWASP Top 10, security best practices | `subagent-security-analyst` |
| `provider-resilience` | 1.0.0 | Defensive `gh`/`az` CLI patterns, error handling, idempotency | `subagent-architect`, `subagent-qa` |
| `caveman` | 1.0.0 | Compressed orchestrator prose (opt-in) | Orchestrator (instruction) |

### Caveman Mode

At session start, read `.claude/settings.json`. If `behavior.caveman_mode === true`, apply `.claude/skills/caveman/SKILL.md` for the rest of the session. Default is `false`; the flag is read once per session. See SKILL.md for the full compress/preserve rules and non-goals. Reviewers touching CLAUDE.md or settings.json should confirm both this section and the `behavior.caveman_mode` key still exist — drift is detected only by review.

**`behavior.*` namespace policy:** keys under `behavior` in `.claude/settings.json` are reserved for non-security-relevant style and UX toggles only (e.g. response formatting, verbosity, tone). Anything that gates a security control — auth bypass, permission grants, security-review skips, hook disabling — MUST be enforced in code or in a runtime hook, never by an instruction-only flag the orchestrator reads from a JSON file.

---

## Project Technology Stack

### maestro - CLI Tool (Rust)

| Technology | Details |
|-----------|---------|
| **Language** | Rust |
| **TUI** | ratatui + crossterm |
| **Async Runtime** | tokio |
| **CLI** | clap (derive) |
| **Serialization** | serde + serde_json |
| **Config** | toml |
| **Testing** | `cargo test` (built-in) |
| **Build** | `cargo build` |

### Build Commands

| Command | Purpose |
|---------|---------|
| `cargo build` | Build debug binary |
| `cargo build --release` | Build release binary |
| `cargo test` | Run all tests |
| `cargo run` | Run in dev mode |
| `cargo clippy` | Lint |
| `cargo fmt` | Format code |

### Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | CLI entry point (clap) |
| `src/config.rs` | maestro.toml parsing |
| `src/session/manager.rs` | Claude CLI process management |
| `src/session/parser.rs` | stream-json output parser |
| `src/session/types.rs` | Session state machine |
| `src/state/store.rs` | JSON state persistence |
| `src/tui/app.rs` | App state and event coordination |
| `src/tui/ui.rs` | ratatui rendering |
| `Cargo.toml` | Dependencies |
| `maestro.toml` | Runtime configuration |

---

## Directory Tree Management

The `directory-tree.md` file at project root is the **SINGLE SOURCE OF TRUTH** for project structure. The `subagent-docs-analyst` maintains it automatically.

- NEVER duplicate directory trees in other .md files
- ALWAYS reference `directory-tree.md` for structure information

---

## Updates

### When a new subagent is created:
1. Start in `drafts/agents/` for development
2. Move to `.claude/agents/` when ready
3. Update the Subagent Registry above

### When a new skill is created:
1. Create directory in `.claude/skills/{skill-name}/`
2. Create `SKILL.md` with frontmatter
3. Update relevant subagents to reference the skill
