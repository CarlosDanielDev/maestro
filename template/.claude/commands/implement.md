# Implement Issue

Fetch a GitHub issue and implement it following the orchestrated workflow.

**Usage:** `/implement #123` or `/implement 123 -e -o`

## Flags
- `--english`/`-e`, `--portuguese`/`-pt`, `--spanish`/`-s` — Language
- `--orchestrator`/`-o`, `--vibe-coding`/`-vc` — Mode

## Flow
1. Parse issue number from `$ARGUMENTS`
2. Ask language/mode (if not flagged)
3. Fetch issue via `gh issue view`
4. DOR contract check (if API endpoints)
5. Create feature branch
6. Execute in selected mode (following CLAUDE.md workflow)
7. Report completion
