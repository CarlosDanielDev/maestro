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
