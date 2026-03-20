# Setup Project

Initialize a project with the Maestro Claude Code template.

**Usage:** `/setup-project` or `/setup-project /path/to/project`

---

## Arguments

`$ARGUMENTS` optionally contains the path to the target project.

If no path provided, use the current working directory.

---

## Instructions

### Step 1: Check Prerequisites

Verify the target project exists and has a `.git` directory.

### Step 2: Copy Template

Copy the Maestro template into the project:
```bash
cp -r template/.claude/ <target-project>/.claude/
```

If `.claude/` already exists:
- Ask user: "A .claude/ directory already exists. Overwrite? (y/n)"
- If no, abort

### Step 3: Detect Technology Stack

Analyze the project to detect its stack:

| File | Technology |
|------|-----------|
| `Cargo.toml` | Rust |
| `package.json` | Node.js / JavaScript / TypeScript |
| `pyproject.toml` / `setup.py` | Python |
| `go.mod` | Go |
| `*.xcodeproj` / `Package.swift` | Swift / iOS |
| `build.gradle` | Java / Kotlin / Android |

### Step 4: Customize CLAUDE.md

Update the **Project Technology Stack** section in `.claude/CLAUDE.md` with detected info:
- Language and framework
- Build commands
- Test commands
- Key file paths

### Step 5: Create project-patterns Skill

Pre-fill `.claude/skills/project-patterns/SKILL.md` with:
- Detected technology stack
- Module structure from directory tree
- Common patterns found in existing code

### Step 6: Report

```
Project setup complete!

Files created:
  .claude/CLAUDE.md                    (customized for your stack)
  .claude/settings.json
  .claude/agents/                      (5 agents)
  .claude/commands/                    (2 commands)
  .claude/skills/project-patterns/     (pre-filled for your stack)
  .claude/skills/api-contract-validation/
  .claude/skills/security-patterns/

Next steps:
1. Review .claude/CLAUDE.md — customize the tech stack section
2. Edit .claude/skills/project-patterns/SKILL.md — add your conventions
3. Start coding with: /implement #<issue-number>
```
