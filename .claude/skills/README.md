# Skills Directory

This directory contains **skill templates** that subagents consult for best practices, patterns, and optimization techniques.

## What Are Skills?

Skills are reusable knowledge bases that provide:
- **Patterns**: Code templates and architecture patterns for your tech stack
- **Optimization**: Performance and best practice guidelines
- **Testing**: Testing strategies and examples
- **Security**: Security patterns and OWASP compliance

## How Skills Work

1. **Subagents consult skills** during their analysis
2. **Skills provide examples and patterns** that match your project's stack
3. **Orchestrator implements** the recommendations with skill-provided code

**Important:** Skills are NOT directly invocable by users. They are internal resources for subagents.

## Creating Skills for Your Tech Stack

### Step 1: Create Skill Directory

```bash
mkdir -p .claude/skills/{your-skill-name}
```

### Step 2: Create SKILL.md with Frontmatter

Every skill MUST have a `SKILL.md` file with this frontmatter:

```yaml
---
name: your-skill-name
version: "1.0.0"
description: Brief description of what this skill provides
allowed-tools: Read, Grep, Glob, WebSearch
---
```

### Step 3: Add Quick Reference

The `SKILL.md` should contain:
- **Critical requirements** (what MUST be followed)
- **Quick patterns** (common code snippets)
- **Links to detailed guides** (other .md files in the skill directory)

### Step 4: Add Detailed Guides

Create topic-specific .md files for detailed examples:

```
.claude/skills/your-skill-name/
├── SKILL.md                    # Quick reference (required)
├── component-patterns.md       # Detailed guide
├── state-management.md         # Detailed guide
└── optimization.md             # Detailed guide
```

### Step 5: Update Subagent References

Update the relevant subagent to reference your skill in their "Consult Skills" section.

## Skill Structure Best Practices

### Progressive Disclosure

Use the **progressive disclosure pattern**:
- `SKILL.md` contains **quick reference** (loaded always)
- Detailed guides contain **complete examples** (loaded when needed)

This reduces token consumption while maintaining depth when required.

### Example Structure

```markdown
# SKILL.md (Quick Reference)

## Critical Stack Requirements
- Framework: [Your framework]
- State Management: [Your approach]
- Testing: [Your approach]

## Quick Patterns Reference

### Component Pattern
\```typescript
// Quick example
\```

## Detailed Guides

For complete implementation details, read:
- [component-patterns.md](component-patterns.md)
- [state-management.md](state-management.md)
```

## Example Skills to Create

### For Mobile Development
**Skill Name:** `mobile-patterns`
**Topics:**
- Component structure
- State management (Redux, MobX, Context, etc.)
- Navigation patterns
- Platform-specific code
- Testing strategies (Detox, Jest, etc.)

### For Frontend Web Development
**Skill Name:** `frontend-patterns`
**Topics:**
- Component architecture (React, Vue, Angular, etc.)
- State management
- Routing patterns
- Form handling
- Performance optimization

### For Backend Development
**Skill Name:** `backend-patterns`
**Topics:**
- API design (REST, GraphQL, gRPC)
- Controller → Service → Repository pattern
- Database optimization
- Validation patterns
- Authentication/authorization

### For Testing
**Skill Name:** `testing-patterns`
**Topics:**
- Unit testing
- Integration testing
- E2E testing frameworks
- Test organization
- Mocking strategies

### For Security
**Skill Name:** `security-patterns`
**Topics:**
- OWASP Top 10 prevention
- Authentication patterns
- Authorization patterns
- Input validation
- Secure headers

### For Optimization
**Skill Name:** `performance-patterns`
**Topics:**
- Bundle optimization
- Database query optimization
- Caching strategies
- Memory management
- Profiling techniques

## Referencing Skills in Subagents

In your subagent files, add skill consultation in the analysis workflow:

```markdown
## Step 1: Discover Available Skills
\```
Use Glob to list available skills:
.claude/skills/*{relevant}*/SKILL.md
\```

## Step 2: Read Relevant Skills
Depending on what skills exist and the task at hand:
- [Your skill name] skill - [What it provides]

## Step 3: Apply Patterns in Recommendations
- Include complete code examples from the skills
- Reference specific pattern files consulted
- Flag anti-patterns observed in the codebase
```

## Versioning Skills

When you update a skill:
1. Increment the version number in the frontmatter
2. Document changes in skill's CHANGELOG.md (optional)
3. Update any subagents that reference the skill if needed

## Multi-Framework Skills

If your project uses multiple frameworks, create separate skills:

```
.claude/skills/
├── mobile-react-native-patterns/   # React Native specific
├── mobile-flutter-patterns/        # Flutter specific
├── frontend-react-patterns/        # React Web specific
├── frontend-vue-patterns/          # Vue specific
├── backend-express-patterns/       # Express.js specific
└── backend-django-patterns/        # Django specific
```

## Skill Examples

See the example template skills provided in this directory to get started:
- `example-mobile-patterns/` - Template for mobile development
- `example-frontend-patterns/` - Template for frontend development
- `example-backend-patterns/` - Template for backend development

Copy these templates and customize them for your specific tech stack.

## Tips

1. **Keep skills focused**: Each skill should cover one domain (mobile, frontend, backend, testing, security)
2. **Use your project's conventions**: Include actual code from your codebase as examples
3. **Update regularly**: As your patterns evolve, update the skills
4. **Reference external docs**: Link to framework documentation and best practices
5. **Include anti-patterns**: Show what NOT to do, not just what to do

## Need Help?

Run `/create-agent` to create new custom subagents that can consume your skills.

For more details, see `.claude/CUSTOMIZATION-GUIDE.md`
