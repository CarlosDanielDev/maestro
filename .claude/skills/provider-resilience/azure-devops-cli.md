# Azure DevOps CLI (`az`) Patterns

## Command Reference for Maestro

Maestro supports Azure DevOps as an alternative provider (auto-detected from git remote or configured via `[provider] kind = "azure_devops"`).

### Work Items (equivalent to GitHub Issues)

```bash
# Create
az boards work-item create --title "feat: X" --type "User Story" --description "..." --org https://dev.azure.com/ORG --project PROJECT

# Update
az boards work-item update --id 123 --state "Closed"

# Query
az boards work-item list --org https://dev.azure.com/ORG --project PROJECT

# Show
az boards work-item show --id 123
```

### Iterations (equivalent to GitHub Milestones)

```bash
# List
az boards iteration project list --org https://dev.azure.com/ORG --project PROJECT

# Create
az boards iteration project create --name "Sprint 1" --path "\\PROJECT\\Iteration" --org https://dev.azure.com/ORG --project PROJECT
```

### Labels/Tags

Azure DevOps uses **tags** on work items, not standalone label resources. Tags are auto-created on first use — no 422 equivalent. However:

```bash
# Add tag to work item
az boards work-item update --id 123 --fields "System.Tags=maestro:ready; priority:P1"
```

**Key difference from GitHub:** Tags don't need to be pre-created. This means the label 422 bug doesn't affect Azure DevOps.

### Pull Requests

```bash
# Create
az repos pr create --title "feat: X" --description "..." --source-branch feat/x --target-branch main

# List
az repos pr list --status active

# Review
az repos pr set-vote --id 123 --vote approve
```

## Azure DevOps Error Patterns

| Error Code | Meaning | Recovery |
|------------|---------|----------|
| `TF401019` | Resource already exists | Find existing, reuse |
| `TF400813` | Field validation error | Check required fields |
| `VS403403` | Rate limit / throttling | Exponential backoff |
| `TF401349` | Permission denied | Warn user |
| `TF400898` | Invalid field value | Validate before sending |
| `VS800024` | Project not found | Check org/project config |

## Provider-Agnostic Patterns in Maestro

The `GitHubClient` trait in `src/github/client.rs` abstracts both providers. When adding resilience:

1. Error classification should handle BOTH `gh` stderr patterns AND `az` error codes
2. Idempotency patterns apply to both providers
3. Rate limits differ: GitHub = 5000/hr, Azure DevOps = varies by tier
4. Authentication: `gh auth status` vs `az account show`

## Key Differences from GitHub

| Feature | GitHub (`gh`) | Azure DevOps (`az`) |
|---------|---------------|---------------------|
| Labels | Must pre-exist | Auto-created (tags) |
| Milestones | Unique by title | Unique by path |
| Auth | `gh auth login` | `az login` |
| Rate limit | 5000/hr fixed | Tier-dependent |
| Body limit | ~65k chars | ~128k chars |
| 422 risk | High (labels, milestones) | Low (tags auto-create) |
