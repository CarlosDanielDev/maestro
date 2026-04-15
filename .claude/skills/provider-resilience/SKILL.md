---
name: provider-resilience
version: "1.0.0"
description: Defensive patterns for GitHub (gh) and Azure DevOps (az) CLI interactions — error handling, idempotency, rate limits, missing resources.
allowed-tools: Read, Grep, Glob, WebSearch
---

# Provider Resilience Patterns

> Knowledge base for robust `gh` and `az` CLI interactions. Consulted by `subagent-architect` and `subagent-qa` when designing features that touch GitHub or Azure DevOps APIs.

## When to Consult This Skill

- Creating milestones, issues, PRs, or labels programmatically
- Reading/updating GitHub or Azure DevOps resources
- Handling CLI subprocess errors from `gh` or `az`
- Designing idempotent operations (adapt, materializer, CI polling)
- Rate limit management for batch operations

## Quick Reference

| Topic | File |
|-------|------|
| Error taxonomy and recovery | [errors.md](errors.md) |
| Idempotency patterns | [idempotency.md](idempotency.md) |
| GitHub CLI (`gh`) patterns | [github-cli.md](github-cli.md) |
| Azure DevOps CLI (`az`) patterns | [azure-devops-cli.md](azure-devops-cli.md) |
| Rate limiting and batching | [rate-limits.md](rate-limits.md) |

## Core Principle

**Never assume remote state.** Always verify-then-act or act-then-recover. Maestro runs on user machines against repos we don't control — labels, milestones, permissions, and API limits vary per project.
