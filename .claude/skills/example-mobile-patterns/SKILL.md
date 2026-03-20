---
name: example-mobile-patterns
version: "1.0.0"
description: TEMPLATE - Mobile development patterns. Copy and customize for your mobile framework (React Native, Flutter, Swift, Kotlin, etc.)
allowed-tools: Read, Grep, Glob, WebSearch
---

# Mobile Patterns (TEMPLATE)

**This is a TEMPLATE skill. Copy this directory and customize it for your mobile framework.**

Quick reference for mobile development patterns. For detailed examples, see linked guides.

## Skill Usage

| Aspect | Details |
|--------|---------|
| **Consumer** | `subagent-mobile-architect` |
| **Purpose** | Code patterns and examples for mobile implementation |
| **Invocation** | Subagents read this skill; NOT directly invocable by users |
| **How to Customize** | Replace examples below with your framework's patterns |

---

## Step 1: Choose Your Framework

Replace this section with your framework-specific requirements:

### Option A: React Native
```typescript
// Example: Redux with hooks
import { useSelector, useDispatch } from 'react-redux'

const MyComponent = () => {
  const data = useSelector(state => state.feature.data)
  const dispatch = useDispatch()

  return <View>...</View>
}
```

### Option B: Flutter
```dart
// Example: Provider pattern
class MyWidget extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final data = Provider.of<DataModel>(context);
    return Container(...);
  }
}
```

### Option C: Native iOS (Swift)
```swift
// Example: SwiftUI with @State
struct ContentView: View {
    @State private var data: String = ""

    var body: some View {
        Text(data)
    }
}
```

### Option D: Native Android (Kotlin)
```kotlin
// Example: Jetpack Compose
@Composable
fun MyScreen(viewModel: MyViewModel = viewModel()) {
    val data by viewModel.data.collectAsState()
    Text(text = data)
}
```

---

## Critical Stack Requirements (CUSTOMIZE THIS)

| Feature | Your Pattern | Not Allowed |
|---------|--------------|-------------|
| **State** | [Your state management] | [What to avoid] |
| **Navigation** | [Your navigation library] | [What to avoid] |
| **UI** | [Your UI library] | [What to avoid] |
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

### Navigation

```
[Your navigation pattern example]
```

### Testing

```
[Your testing pattern example]
```

---

## Detailed Guides

When you need specific implementation details, read:

- **[component-patterns.md](component-patterns.md)** - Component templates and structure
- **[state-management.md](state-management.md)** - State management patterns
- **[navigation-patterns.md](navigation-patterns.md)** - Navigation flows
- **[testing-patterns.md](testing-patterns.md)** - Testing strategies

---

## Common Anti-Patterns to Avoid (CUSTOMIZE THIS)

Add framework-specific anti-patterns here:

1. ❌ [Anti-pattern 1 for your framework]
2. ❌ [Anti-pattern 2 for your framework]
3. ❌ [Anti-pattern 3 for your framework]

---

## Dependencies Reference (CUSTOMIZE THIS)

```json
{
  "your-framework": "Core framework",
  "your-state-library": "State management",
  "your-navigation-library": "Navigation",
  "your-testing-framework": "Testing"
}
```

---

## When to Consult This Skill

- Designing mobile component architecture
- Implementing state management
- Creating navigation flows
- Setting up mobile testing
- Platform-specific implementations

---

## Customization Instructions

1. **Copy this directory** to a new skill (e.g., `mobile-react-native-patterns`)
2. **Update frontmatter** with your skill name and description
3. **Replace all examples** with your framework's patterns
4. **Create detailed guides** in separate .md files
5. **Update the architect subagent** to reference this skill
6. **Delete this template** or move it to `drafts/skills/`
