# Implement Issue

Fetch a GitHub issue and implement it following the full orchestrated workflow.

**Usage:** `/implement #123` or `/implement 123 --english --orchestrator`

---

## Arguments

`$ARGUMENTS` contains the issue number and optional flags.

### Supported Flags

| Flag | Short | Purpose |
|------|-------|---------|
| `--english` | `-e` | Set language to English |
| `--portuguese` | `-pt` | Set language to Português do Brasil |
| `--spanish` | `-s` | Set language to Español |
| `--orchestrator` | `-o` | Use Subagents Orchestrator mode |
| `--vibe-coding` | `-vc` | Use Vibe Coding mode |

---

## Instructions

### Step 0: Parse Arguments

Extract from `$ARGUMENTS`:
1. **Issue number**: The first number found (with or without `#` prefix)
2. **Language flag** (if present)
3. **Mode flag** (if present)

If no issue number found, ask: "Which issue should I implement?"

### Step 1: Language Selection

If a language flag was provided, use it. Otherwise, ask the user.

### Step 2: Mode Selection

If a mode flag was provided, use it. Otherwise, ask the user.

### Step 3: Fetch Issue from GitHub

```bash
gh issue view <issue-number> --json title,body,labels,assignees,milestone,state,comments
```

### Step 4: Analyze Issue

Present a brief summary:
```
Issue #<number>: <title>
Labels: <labels>
State: <state>

Summary: <1-3 sentence summary>

Proceeding with <selected mode>...
```

### Step 4.5: DOR Contract Check (if API endpoints involved)

Scan the issue body for API endpoint references. If endpoints found:
- Check `docs/api-contracts/` for existing schema
- If no schema exists and issue body has one, save it
- If no schema at all, **STOP** — DOR failure

### Step 5: Create Feature Branch (if needed)

If on `main`/`master`, create: `feat/issue-<number>-<short-description>`

### Step 6: Execute Based on Selected Mode

**Orchestrator Mode:**
1. `subagent-architect` → Architecture Blueprint
2. `/validate-contracts` → Contract Validation (if API)
3. `subagent-qa` → Test Blueprint
4. Write tests FIRST (RED)
5. Implement (GREEN)
6. Refactor
7. `subagent-security-analyst` → Security review
8. `subagent-docs-analyst` → Documentation

**Vibe Coding Mode:**
1. `/validate-contracts` (if API)
2. Write tests FIRST (RED)
3. Implement (GREEN)
4. Refactor
5. `subagent-docs-analyst` → Documentation

### Step 7: Post-Implementation

```
Implementation complete for Issue #<number>: <title>

Next steps:
- Review changes: `git diff`
- Run /pushup to commit, push, create PR, and close the issue
```

---

## Error Handling

- If `gh` CLI not installed → suggest `brew install gh`
- If not authenticated → suggest `gh auth login`
- If issue closed → warn and ask to proceed
- If dirty working tree → ask to stash or commit first
