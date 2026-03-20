# Update From Template

Sync template updates from Maestro into a downstream project's `.claude/` directory.

**Usage:** `/update-from-template` or `/update-from-template /path/to/project`

---

## Arguments

`$ARGUMENTS` optionally contains the path to the target project.

If no path provided, use the current working directory.

---

## Instructions

### Step 1: Locate Template

Find the Maestro template at `template/.claude/` in the maestro project.

### Step 2: Compare Files

For each file in `template/.claude/`:
1. Check if it exists in the target project's `.claude/`
2. If it exists, compare content
3. Categorize as: NEW, UPDATED, IDENTICAL, or CUSTOMIZED

**CUSTOMIZED** = file exists but has user modifications beyond the template. These should NOT be overwritten.

### Step 3: Present Diff Report

```
Template Sync Report
====================

NEW (will be added):
  .claude/commands/new-command.md

UPDATED (template changed, your version is default):
  .claude/agents/subagent-architect.md

IDENTICAL (no changes needed):
  .claude/agents/subagent-docs-analyst.md

CUSTOMIZED (will NOT overwrite — manual merge needed):
  .claude/CLAUDE.md (you customized the tech stack)
  .claude/skills/project-patterns/SKILL.md (your project patterns)

Apply updates? (NEW + UPDATED only) [y/n]
```

### Step 4: Apply Updates

Only apply NEW and UPDATED files. Never overwrite CUSTOMIZED files.

### Step 5: Report Completion

```
Template sync complete.

Applied:
  + .claude/commands/new-command.md (NEW)
  ~ .claude/agents/subagent-architect.md (UPDATED)

Skipped (manual merge needed):
  .claude/CLAUDE.md — compare with template/.claude/CLAUDE.md
```
