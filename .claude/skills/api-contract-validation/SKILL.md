---
name: api-contract-validation
version: "2.0.0"
description: API contract validation patterns for ensuring client-side models match backend JSON responses. Prevents decoding failures from schema mismatches. Tech-stack agnostic.
allowed-tools: Read, Grep, Glob, WebSearch
---

# API Contract Validation Skill

## Purpose

Prevent bugs where client-side models don't match backend API responses — causing silent decoding failures at runtime.

## Contract Schema Location

All API contracts live in `docs/api-contracts/` as JSON files.

### Naming Convention
```
docs/api-contracts/{feature-area}-{endpoint-name}.json
```

## Contract Schema Format

```json
{
  "$schema": "api-contract-v1",
  "endpoint": "GET /api/items/list",
  "description": "Fetch items for display",
  "issue": "#38",
  "lastVerified": "2026-03-10",
  "request": {
    "query": {
      "count": { "type": "integer", "required": false, "default": 10 }
    }
  },
  "response": {
    "items": {
      "type": "array",
      "items": {
        "id": { "type": "string", "required": true },
        "name": { "type": "string", "required": true },
        "score": { "type": "integer", "required": false }
      }
    },
    "total": { "type": "integer", "required": true }
  }
}
```

## DOR (Definition of Ready)

An issue is NOT ready for implementation unless:
- API contract schema exists in `docs/api-contracts/`
- All response fields documented with types and required/optional
- Schema verified against actual backend response

## DOD (Definition of Done)

Implementation is NOT done unless:
- Client models match contract schema field-by-field
- Required fields are non-optional in client code
- Optional fields are properly nullable
- A decode test exists using example JSON from the contract
- No extra fields in client model that don't exist in contract

## Common Mismatch Patterns

1. **Ghost fields**: Client model has a field that doesn't exist in backend response
2. **Required mismatch**: Backend returns field optionally, client declares it as required
3. **Type mismatch**: Backend returns `number` (float) but client declares integer
4. **Naming mismatch**: Backend uses `snake_case`, client uses `camelCase` without mapping
5. **Nested structure mismatch**: Backend nests data differently than client expects

## Validation Process

1. Read contract schema from `docs/api-contracts/`
2. Read the corresponding client model file
3. For each field in schema: verify exists, type matches, optionality matches
4. For each field in client model: verify exists in schema (catch ghost fields)
5. Report mismatches

## Integration with Orchestrator Workflow

```
subagent-architect → Architecture Blueprint
    │
    ▼
CONTRACT VALIDATION ← This skill
    │
    ▼
subagent-qa → Test Blueprint (includes decode tests)
```
