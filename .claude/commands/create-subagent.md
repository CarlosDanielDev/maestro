# Create Subagent

Create a new subagent with the name: $ARGUMENTS

## Instructions

1. The file name MUST follow the pattern `subagent-{name}.md`
2. The file MUST be created in `.claude/agents/`
3. Use the template below as a base
4. **MANDATORY**: After creating the subagent, update CLAUDE.md

## Subagent Template

```markdown
---
name: subagent-{name}
description: [Describe when this subagent should be invoked]
tools: Read, Glob, Grep, WebFetch, WebSearch
model: sonnet
---

# CRITICAL RULES - MANDATORY COMPLIANCE

## Language Behavior
- **Detect user language**: Always detect and respond in the same language the user is using
- **Artifacts in English**: ALL generated artifacts (.md files, documentation, reports) MUST be written in English
- **File locations**: All .md files MUST be saved in `docs/` directory

## Role Restrictions - EXTREMELY IMPORTANT

**YOU ARE A CONSULTIVE AGENT ONLY.**

### ABSOLUTE PROHIBITION - NO CODE WRITING
- You CANNOT write, modify, or create code files
- You CANNOT use Write, Edit, or Bash tools
- You CANNOT create scripts, functions, or any executable code
- You CAN ONLY: analyze, research, plan, recommend, and document

### Your Role
1. **Research**: Investigate [area of expertise]
2. **Analyze**: Examine [what to analyze]
3. **Plan**: Design [what to plan]
4. **Document**: Generate [type of documentation]
5. **Advise**: Provide detailed guidance for the ORCHESTRATOR to implement

### Output Behavior - CRITICAL
When you complete your analysis, you MUST provide:
1. **Exact file paths** where changes should be made
2. **Exact line numbers** for edits
3. **Complete code examples** ready for the orchestrator to copy
4. **Step-by-step instructions** for the orchestrator to execute

**The ORCHESTRATOR is the ONLY agent that writes code. You provide the blueprint.**

---

# [Agent Name] - Core Expertise

## Responsibilities

- [List main responsibilities]

## Best Practices

- ALWAYS search the web for updated development best practices
- ALWAYS search for security best practices (OWASP, etc.)
- Use Context7 MCP server to query updated library documentation

## Workflow

1. Analyze the request context
2. Search for updated practices on the web if needed
3. Query Context7 for library documentation
4. Execute the task following the guidelines
5. Validate security and quality before finalizing
```

## Mandatory Actions

### 1. Ask the user
Before creating, ask:
- What is the subagent's specialization?
- What are the main responsibilities?
- Are any additional tools needed?

### 2. Create the subagent file
Create the file `.claude/agents/subagent-$ARGUMENTS.md` following the template above, adapting:
- The subagent name
- The description
- The area of expertise
- The specific responsibilities
- The tools needed for the function

### 3. Update CLAUDE.md
**MANDATORY**: After creating the subagent, update the `CLAUDE.md` file at the project root.

Locate the section between the markers:
```
<!-- SUBAGENTS_LIST_START -->
<!-- SUBAGENTS_LIST_END -->
```

Add a new line with the created subagent in the format:
```
- **subagent-{name}**: [brief description of the subagent]
```

Example of how it should look after adding:
```markdown
<!-- SUBAGENTS_LIST_START -->
- **subagent-code-reviewer**: Reviews code and suggests improvements
- **subagent-security**: Analyzes security vulnerabilities
<!-- SUBAGENTS_LIST_END -->
```

---

## Skills Consideration

After creating a subagent, evaluate if a dedicated **skill** should be created.

### When to Create an Associated Skill

Create a skill for the subagent if **ALL** of these apply:

1. ✅ **Extensive knowledge base** (>500 lines of patterns/examples)
2. ✅ **Multiple topics** that could be split into guides
3. ✅ **Reusable by other subagents** (or will be in the future)

### When NOT to Create a Skill

Keep knowledge in the subagent if:

- ❌ **Small knowledge base** (<200 lines total)
- ❌ **Single topic** with no need for splitting
- ❌ **Unique to this subagent** (won't be reused)

### Decision Matrix

| Subagent Type | Create Skill? | Reasoning |
|---------------|--------------|-----------|
| Architecture (frontend, backend, mobile) | ✅ Yes | Large pattern libraries, multiple guides needed |
| QA (testing) | ✅ Yes | Test templates reusable, device matrices, etc. |
| Security | ✅ Yes | OWASP patterns, vulnerability detection guides |
| Documentation | ❌ No | Straightforward, no complex patterns |
| Simple reviewer | ❌ No | Small scope, no extensive knowledge |

### How to Create Associated Skill

If you decide to create a skill:

#### 1. Create skill directory structure:
```bash
mkdir -p .claude/skills/{skill-name}
```

#### 2. Create SKILL.md (< 300 lines):
```markdown
---
name: {skill-name}
version: "1.0.0"
description: Brief description when to use this skill
allowed-tools: Read, Grep, Glob, WebSearch
---

# {Skill Name}

Quick reference for {topic}.

## Skill Usage

| Aspect | Details |
|--------|---------|
| **Consumer** | `subagent-{name}` |
| **Purpose** | {purpose} |
| **Invocation** | Subagent reads this skill; NOT directly invocable by users |

## Quick Reference

[Quick patterns and examples - keep under 100 lines]

## Detailed Guides

For comprehensive patterns, see:
- **[guide-1.md](guide-1.md)** - {Description}
- **[guide-2.md](guide-2.md)** - {Description}

## When to Consult This Skill

- {Scenario 1}
- {Scenario 2}
```

#### 3. Create detailed guides (200-500 lines each):
```
.claude/skills/{skill-name}/
├── SKILL.md
├── guide-1.md     # Complete examples for topic 1
├── guide-2.md     # Complete examples for topic 2
└── guide-3.md     # Complete examples for topic 3
```

#### 4. Update subagent to reference the skill:

Add this section to the subagent file **BEFORE** the main content:

```markdown
---

# MANDATORY: Consult Pattern Skills

**BEFORE providing recommendations, read the pattern skills:**

## Step 1: Read {Skill Name} Skill
```
Use Read tool to access:
.claude/skills/{skill-name}/SKILL.md
```
This contains quick reference for {topic}.

## Step 2: Consult Specific Guides Based on Task
Depending on the task, read the relevant detailed guides:
- `.claude/skills/{skill-name}/guide-1.md` - {Description}
- `.claude/skills/{skill-name}/guide-2.md` - {Description}
- `.claude/skills/{skill-name}/guide-3.md` - {Description}

## Step 3: Apply Patterns in Recommendations
- Include complete code examples from the skills
- Reference specific pattern files consulted
- Flag anti-patterns observed in the codebase

---
```

#### 5. Update CLAUDE.md Skills Table:

Add entry to the Available Skills table in CLAUDE.md:

```markdown
| `{skill-name}` | 1.0.0 | `subagent-{name}` | {Brief description} |
```

### Progressive Disclosure Benefits

By creating skills with progressive disclosure:
- **Token savings**: 300-900 tokens per subagent invocation
- **Faster responses**: Less content to process
- **Better maintainability**: Update skills independently of subagents
- **Reusability**: Other subagents can reference the same skill

---

## Final Checklist

### Subagent Creation
- [ ] File created at `.claude/agents/subagent-{name}.md`
- [ ] Name follows pattern `subagent-{name}`
- [ ] Template applied with user's specializations
- [ ] CLAUDE.md updated with new subagent in the list

### Skill Creation (if applicable)
- [ ] Evaluated need for associated skill (use decision matrix)
- [ ] Created skill directory `.claude/skills/{skill-name}/`
- [ ] Created SKILL.md with frontmatter (< 300 lines)
- [ ] Created detailed guides (200-500 lines each)
- [ ] Updated subagent to reference the skill
- [ ] Updated CLAUDE.md Skills table with new skill
