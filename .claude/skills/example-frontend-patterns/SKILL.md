---
name: example-frontend-patterns
version: "1.0.0"
description: TEMPLATE - Frontend web development patterns. Copy and customize for your frontend framework (React, Vue, Angular, Svelte, etc.)
allowed-tools: Read, Grep, Glob, WebSearch
---

# Frontend Web Patterns (TEMPLATE)

**This is a TEMPLATE skill. Copy this directory and customize it for your frontend framework.**

Quick reference for frontend development patterns. For detailed examples, see linked guides.

## Skill Usage

| Aspect | Details |
|--------|---------|
| **Consumer** | `subagent-frontend-architect` |
| **Purpose** | Code patterns and examples for frontend implementation |
| **Invocation** | Subagents read this skill; NOT directly invocable by users |
| **How to Customize** | Replace examples below with your framework's patterns |

---

## Step 1: Choose Your Framework

Replace this section with your framework-specific requirements:

### Option A: React
```typescript
// Example: React with hooks and TypeScript
import React, { useState, useEffect } from 'react'

interface Props {
  data: DataType
}

export const MyComponent: React.FC<Props> = ({ data }) => {
  const [state, setState] = useState<StateType>(initialState)

  useEffect(() => {
    // side effects
  }, [dependencies])

  return <div>{/* JSX */}</div>
}
```

### Option B: Vue 3
```vue
<template>
  <div>{{ data }}</div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'

const data = ref<DataType>(initialValue)
const computedValue = computed(() => data.value * 2)

onMounted(() => {
  // lifecycle hook
})
</script>
```

### Option C: Angular
```typescript
// Example: Angular component with TypeScript
import { Component, OnInit } from '@angular/core'

@Component({
  selector: 'app-my-component',
  templateUrl: './my-component.component.html',
  styleUrls: ['./my-component.component.css']
})
export class MyComponent implements OnInit {
  data: DataType

  constructor(private service: MyService) {}

  ngOnInit(): void {
    // initialization
  }
}
```

### Option D: Svelte
```svelte
<script lang="ts">
  import { onMount } from 'svelte'

  let data: DataType = initialValue

  $: computedValue = data * 2

  onMount(() => {
    // lifecycle
  })
</script>

<div>{data}</div>
```

---

## Critical Stack Requirements (CUSTOMIZE THIS)

| Feature | Your Pattern | Not Allowed |
|---------|--------------|-------------|
| **State** | [Your state management] | [What to avoid] |
| **Routing** | [Your router library] | [What to avoid] |
| **Styling** | [Your styling approach] | [What to avoid] |
| **Testing** | [Your testing framework] | [What to avoid] |

---

## Quick Patterns Reference (CUSTOMIZE THIS)

### Component Structure

```
[Your framework's component structure example]
```

### State Management

```
[Your state management pattern example]
```

### Routing

```
[Your routing pattern example]
```

### Forms

```
[Your form handling pattern example]
```

### API Calls

```
[Your API integration pattern example]
```

---

## Detailed Guides

When you need specific implementation details, read:

- **[component-patterns.md](component-patterns.md)** - Component templates
- **[state-management.md](state-management.md)** - State patterns
- **[routing-patterns.md](routing-patterns.md)** - Routing setup
- **[forms-patterns.md](forms-patterns.md)** - Form handling
- **[optimization.md](optimization.md)** - Performance optimization

---

## Common Anti-Patterns to Avoid (CUSTOMIZE THIS)

Add framework-specific anti-patterns here:

1. ❌ [Anti-pattern 1 for your framework]
2. ❌ [Anti-pattern 2 for your framework]
3. ❌ [Anti-pattern 3 for your framework]
4. ❌ Not code-splitting routes
5. ❌ Inline anonymous functions in render
6. ❌ Not memoizing expensive computations

---

## Dependencies Reference (CUSTOMIZE THIS)

```json
{
  "your-framework": "Core framework",
  "your-state-library": "State management",
  "your-router": "Routing",
  "your-form-library": "Form handling",
  "your-testing-framework": "Testing"
}
```

---

## When to Consult This Skill

- Designing component architecture
- Implementing state management
- Creating routing structures
- Handling forms and validation
- Optimizing bundle size and performance

---

## Related Skills

| Skill | When to Consult |
|-------|-----------------|
| `provider-resilience` | Any feature that calls GitHub/Azure DevOps APIs |
| `security-patterns` | XSS, CSRF, input sanitization |
| `api-contract-validation` | Frontend models vs backend JSON |

## Customization Instructions

1. **Copy this directory** to a new skill (e.g., `frontend-react-patterns`)
2. **Update frontmatter** with your skill name and description
3. **Replace all examples** with your framework's patterns
4. **Create detailed guides** in separate .md files
5. **Update the architect subagent** to reference this skill
6. **Delete this template** or move it to `drafts/skills/`
