---
name: api-contract-validation
version: "2.0.0"
description: API contract validation patterns for ensuring client-side models match backend JSON responses. Tech-stack agnostic.
allowed-tools: Read, Grep, Glob, WebSearch
---

# API Contract Validation

## Purpose

Prevent bugs where client-side models don't match backend API responses.

## Contract Location

`docs/api-contracts/{feature}-{endpoint}.json`

## Schema Format

```json
{
  "$schema": "api-contract-v1",
  "endpoint": "GET /api/items",
  "description": "What this endpoint does",
  "response": {
    "items": {
      "type": "array",
      "items": {
        "id": { "type": "string", "required": true },
        "name": { "type": "string", "required": true }
      }
    }
  }
}
```

## Validation: required fields = non-nullable, optional = nullable. No ghost fields.
