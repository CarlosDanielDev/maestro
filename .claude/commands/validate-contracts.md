# Validate API Contracts

Validate client-side models against API contract schemas to catch mismatches before runtime.

**Usage:** `/validate-contracts` or `/validate-contracts items-list`

---

## Arguments

`$ARGUMENTS` contains an optional contract name filter.

- No arguments: validate ALL contracts in `docs/api-contracts/`
- With argument: validate only matching contracts

---

## Instructions

### Step 1: Find Contract Schemas

```bash
ls docs/api-contracts/*.json
```

If `$ARGUMENTS` is provided, filter to matching files.

If no contract files found:
> "No API contract schemas found in `docs/api-contracts/`. Create schemas first using the format defined in `.claude/skills/api-contract-validation/SKILL.md`."

### Step 2: For Each Contract Schema

Read the contract JSON file and extract:
1. **Endpoint** — The API endpoint path
2. **Response fields** — Each field with its type and required/optional status
3. **Issue reference** — The linked issue number

### Step 3: Find Corresponding Client Models

For each contract, find the client model that decodes its response:
1. Search for struct/class/type names that match the response shape
2. Use `Grep` to find model definitions in the project
3. Match by field names

### Step 4: Validate Field-by-Field

For each field in the contract schema:

| Check | Pass | Fail |
|-------|------|------|
| Field exists in model | Found | `MISSING: field 'x' in contract but not in model` |
| Type matches | Types align | `TYPE MISMATCH: 'x' is integer in contract but float in model` |
| Optionality matches | `required: false` → nullable | `OPTIONALITY MISMATCH: 'x' is optional in contract but required in model` |

For each field in the model NOT in the contract:
- `GHOST FIELD: 'x' exists in model but not in contract schema`

### Step 5: Generate Report

```
API Contract Validation Report
==================================

docs/api-contracts/items-list.json → ItemListResponse
   All 6 fields match. No issues found.

docs/api-contracts/items-detail.json → ItemDetailResponse
   ISSUES:
   - OPTIONALITY MISMATCH: 'total' is optional in contract but required in model
   - GHOST FIELD: 'success' exists in model but not in contract

Summary: 1 passed, 1 failed
```

### Step 6: Suggest Fixes

For each failing contract, provide the exact code fix with before/after examples.

---

## Safety

- This command is READ-ONLY — reports mismatches but does NOT modify files
- The orchestrator decides whether to apply suggested fixes
