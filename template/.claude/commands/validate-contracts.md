# Validate API Contracts

Validate client-side models against `docs/api-contracts/` schemas.

**Usage:** `/validate-contracts` or `/validate-contracts feature-name`

## Flow
1. Find contracts in `docs/api-contracts/*.json`
2. Find corresponding client models
3. Validate field-by-field (type, optionality, ghost fields)
4. Report mismatches with suggested fixes

**Read-only** — reports but does not modify files.
