---
name: example-backend-patterns
version: "1.0.0"
description: TEMPLATE - Backend API development patterns. Copy and customize for your backend framework (Express, Django, Spring Boot, FastAPI, etc.)
allowed-tools: Read, Grep, Glob, WebSearch
---

# Backend API Patterns (TEMPLATE)

**This is a TEMPLATE skill. Copy this directory and customize it for your backend framework.**

Quick reference for backend development patterns. For detailed examples, see linked guides.

## Skill Usage

| Aspect | Details |
|--------|---------|
| **Consumer** | `subagent-backend-architect` |
| **Purpose** | Code patterns and examples for backend implementation |
| **Invocation** | Subagents read this skill; NOT directly invocable by users |
| **How to Customize** | Replace examples below with your framework's patterns |

---

## Step 1: Choose Your Framework

Replace this section with your framework-specific requirements:

### Option A: Express.js (Node.js)
```typescript
// Example: Express with TypeScript
import { Router, Request, Response } from 'express'

const router = Router()

router.get('/api/resource', async (req: Request, res: Response) => {
  try {
    const data = await service.getData()
    res.json(data)
  } catch (error) {
    res.status(500).json({ error: error.message })
  }
})
```

### Option B: Django (Python)
```python
# Example: Django REST Framework
from rest_framework import viewsets
from rest_framework.response import Response

class ResourceViewSet(viewsets.ViewSet):
    def list(self, request):
        data = service.get_data()
        serializer = ResourceSerializer(data, many=True)
        return Response(serializer.data)
```

### Option C: Spring Boot (Java)
```java
// Example: Spring Boot REST Controller
@RestController
@RequestMapping("/api/resource")
public class ResourceController {

    @Autowired
    private ResourceService service;

    @GetMapping
    public ResponseEntity<List<Resource>> getResources() {
        List<Resource> resources = service.getAll();
        return ResponseEntity.ok(resources);
    }
}
```

### Option D: FastAPI (Python)
```python
# Example: FastAPI with Pydantic
from fastapi import APIRouter, Depends
from pydantic import BaseModel

router = APIRouter()

class Resource(BaseModel):
    id: int
    name: str

@router.get("/api/resource", response_model=List[Resource])
async def get_resources(service: ResourceService = Depends()):
    return await service.get_all()
```

---

## Critical Stack Requirements (CUSTOMIZE THIS)

| Feature | Your Pattern | Not Allowed |
|---------|--------------|-------------|
| **Framework** | [Your backend framework] | [What to avoid] |
| **Database** | [Your database + ORM] | [What to avoid] |
| **Validation** | [Your validation library] | [What to avoid] |
| **Auth** | [Your auth approach] | [What to avoid] |
| **Testing** | [Your testing framework] | [What to avoid] |

---

## Quick Patterns Reference (CUSTOMIZE THIS)

### Controller Pattern

```
[Your framework's controller/route handler example]
```

### Service Layer Pattern

```
[Your service layer pattern example]
```

### Repository/Data Access Pattern

```
[Your data access pattern example]
```

### Validation Pattern

```
[Your validation pattern example]
```

### Error Handling Pattern

```
[Your error handling pattern example]
```

---

## Detailed Guides

When you need specific implementation details, read:

- **[controller-patterns.md](controller-patterns.md)** - API endpoint patterns
- **[service-patterns.md](service-patterns.md)** - Business logic layer
- **[repository-patterns.md](repository-patterns.md)** - Data access layer
- **[validation-patterns.md](validation-patterns.md)** - Input validation
- **[auth-patterns.md](auth-patterns.md)** - Authentication/authorization
- **[optimization.md](optimization.md)** - Database and API optimization

---

## Common Anti-Patterns to Avoid (CUSTOMIZE THIS)

Add framework-specific anti-patterns here:

1. ❌ [Anti-pattern 1 for your framework]
2. ❌ [Anti-pattern 2 for your framework]
3. ❌ [Anti-pattern 3 for your framework]
4. ❌ N+1 database queries
5. ❌ Business logic in controllers
6. ❌ Not validating input
7. ❌ SQL injection vulnerabilities

---

## Architecture Layers (CUSTOMIZE THIS)

```
[Your architecture pattern]

Example:
Controller → Service → Repository → Database
    ↓           ↓          ↓
  Routes    Business    Data Access
           Logic
```

---

## Dependencies Reference (CUSTOMIZE THIS)

```json
{
  "your-framework": "Core framework",
  "your-database-driver": "Database client",
  "your-orm": "ORM/Query builder",
  "your-validation-library": "Validation",
  "your-auth-library": "Authentication",
  "your-testing-framework": "Testing"
}
```

---

## When to Consult This Skill

- Designing API endpoints
- Implementing business logic
- Optimizing database queries
- Adding validation
- Setting up authentication/authorization
- Implementing error handling

---

## Related Skills

| Skill | When to Consult |
|-------|-----------------|
| `provider-resilience` | Any feature that creates GitHub/Azure DevOps resources (issues, PRs, milestones, labels) |
| `security-patterns` | Authentication, authorization, input validation |
| `api-contract-validation` | Client-server model alignment |

## Customization Instructions

1. **Copy this directory** to a new skill (e.g., `backend-express-patterns`)
2. **Update frontmatter** with your skill name and description
3. **Replace all examples** with your framework's patterns
4. **Create detailed guides** in separate .md files
5. **Update the architect subagent** to reference this skill
6. **Delete this template** or move it to `drafts/skills/`
