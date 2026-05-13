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
