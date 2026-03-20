# Plan Feature

Plan a feature across your project, creating API contracts first, then milestones and issues with dependency tracking.

**Usage:** `/plan-feature <description>` or `/plan-feature`

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

---

## Error Handling

- If `gh` CLI fails → suggest `gh auth login`
- If description is too vague → ask clarifying questions
- If contract already exists → reuse it
- If milestone exists → reuse it
