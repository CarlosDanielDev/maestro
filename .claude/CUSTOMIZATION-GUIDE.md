# Customization Guide

How to adapt the Maestro Claude Code template for your project.

## Quick Start

1. **Copy template**: `maestro init` copies `template/.claude/` into your project
2. **Customize CLAUDE.md**: Edit the **Project Technology Stack** section
3. **Customize project-patterns**: Edit `.claude/skills/project-patterns/SKILL.md`
4. **Optionally add MCP servers**: Edit `.claude/settings.json`

## Structure

```
.claude/
├── CLAUDE.md              # Main orchestrator config
├── settings.json          # MCP servers, hooks
├── agents/                # Subagent definitions (consultive only)
│   ├── subagent-architect.md
│   ├── subagent-qa.md
│   ├── subagent-security-analyst.md
│   ├── subagent-docs-analyst.md
│   └── subagent-master-planner.md
├── commands/              # Slash commands
│   ├── implement.md
│   └── validate-contracts.md
└── skills/                # Knowledge bases
    ├── project-patterns/  # YOUR patterns (customize!)
    ├── api-contract-validation/
    └── security-patterns/
```

## What to Customize

### 1. CLAUDE.md — Technology Stack

Replace the placeholder tech stack with your actual stack:

```markdown
### my-project

| Technology | Details |
|-----------|---------|
| **Language** | TypeScript |
| **Framework** | Next.js 14 |
| **Database** | PostgreSQL + Prisma |
| **Testing** | Vitest |
| **Build** | `npm run build` |
```

### 2. project-patterns/SKILL.md

This is the most important file to customize. Document:
- Your architecture patterns (MVC, MVVM, Clean Architecture)
- State management approach
- Data access patterns
- Error handling conventions
- Testing conventions (naming, mocking)
- Anti-patterns specific to your project

### 3. Adding Custom Agents

Create a new file in `.claude/agents/`:

```markdown
---
name: subagent-your-agent
description: What this agent does
model: sonnet
tools: Read, Glob, Grep, WebSearch
---

# Your agent instructions here
```

### 4. Adding Custom Skills

Create a directory in `.claude/skills/your-skill/`:

```markdown
---
name: your-skill
version: "1.0.0"
description: What patterns this covers
allowed-tools: Read, Grep, Glob
---

# Your skill content
```

### 5. Adding Custom Commands

Create a file in `.claude/commands/your-command.md` — it becomes `/your-command`.

## Example Stacks

### React + TypeScript
- Agent: Keep `subagent-architect` (it's stack-agnostic)
- Skill: Create `react-patterns/SKILL.md` with component patterns, hooks, state management

### Python + Django
- Agent: Keep `subagent-architect`
- Skill: Create `django-patterns/SKILL.md` with model/view/serializer patterns

### Swift + SwiftUI
- Agent: Keep `subagent-architect`
- Skill: Create `ios-patterns/SKILL.md` with SwiftUI, CoreData, MVVM patterns

### Rust
- Agent: Keep `subagent-architect`
- Skill: Create `rust-patterns/SKILL.md` with error handling, async, trait patterns
