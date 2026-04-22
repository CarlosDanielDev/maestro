# Implement Issue

Fetch a GitHub issue and implement it following the enforced TDD harness.

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

`--training` / `-t` is explicitly rejected — Training mode is for `.claude/` configuration, not implementation.

---

## Instructions

### Step 0: Parse arguments

Extract from `$ARGUMENTS`:
1. **Issue number**: first `\d+` with optional `#` prefix. Export as `$ISSUE_NUMBER` for downstream steps.
2. **Language flag** (if present).
3. **Mode flag** (if present).

```bash
export ISSUE_NUMBER="<n>"  # substitute the parsed number
```

All subsequent gate commands in this file reference `$ISSUE_NUMBER`, not bare `<n>`.

If `--training` or `-t` is detected, output:

```
Training mode is for configuring .claude/ agents, skills, and commands.
/implement is for building features from GitHub issues.
Drop --training, or use a free-form prompt for .claude/ configuration.
```

and exit 0 (not an error).

If no issue number found, ask: "Which issue should I implement?"

### Step 1: Language and mode selection

If flags provided, honor them. Otherwise, ask the user.
