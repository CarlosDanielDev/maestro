# Project Directory Tree

> Last updated: 2026-03-20 18:00 (UTC)
>
> This is the SINGLE SOURCE OF TRUTH for project structure.
> All documentation files should reference this file instead of duplicating the tree.

## Structure

```
maestro/
├── .claude/
│   ├── CLAUDE.md                          # Orchestrator agent instructions
│   ├── CUSTOMIZATION-GUIDE.md             # Guide for customizing the agent system
│   ├── agents/
│   │   ├── subagent-architect.md          # Architecture design subagent
│   │   ├── subagent-docs-analyst.md       # Documentation management subagent
│   │   ├── subagent-master-planner.md     # System-level planning subagent
│   │   ├── subagent-qa.md                 # QA and test design subagent
│   │   └── subagent-security-analyst.md   # Security review subagent
│   ├── commands/
│   │   ├── create-subagent.md             # Slash command: scaffold a new subagent
│   │   ├── implement.md                   # Slash command: run full TDD implementation flow
│   │   ├── plan-feature.md                # Slash command: invoke master planner
│   │   ├── pushup.md                      # Slash command: git push workflow
│   │   ├── setup-notifications.md         # Slash command: configure hook notifications
│   │   ├── setup-project.md               # Slash command: initialize project config
│   │   ├── update-from-template.md        # Slash command: sync from template directory
│   │   ├── validate-contracts.md          # Slash command: validate API contracts
│   │   └── video-frames.md                # Slash command: extract video frames
│   ├── hooks/
│   │   ├── README.md                      # Hook usage documentation
│   │   ├── notify.ps1                     # Windows notification hook
│   │   └── notify.sh                      # Unix notification hook
│   ├── settings.json                      # Claude Code project settings
│   ├── settings.local.json                # Local overrides (not committed)
│   └── skills/
│       ├── README.md                      # Skills system documentation
│       ├── api-contract-validation/
│       │   └── SKILL.md                   # API contract enforcement patterns
│       ├── example-backend-patterns/
│       │   └── SKILL.md                   # Example backend skill template
│       ├── example-frontend-patterns/
│       │   └── SKILL.md                   # Example frontend skill template
│       ├── example-mobile-patterns/
│       │   └── SKILL.md                   # Example mobile skill template
│       ├── project-patterns/
│       │   └── SKILL.md                   # Maestro-specific patterns and conventions
│       ├── security-patterns/
│       │   └── SKILL.md                   # OWASP Top 10 and security best practices
│       └── video-frame-extractor/
│           └── SKILL.md                   # Video frame extraction patterns
├── src/
│   ├── main.rs                            # CLI entry point (clap); Run, Queue, Add, Status, Cost, Init; --milestone flag
│   ├── config.rs                          # maestro.toml parsing; GithubConfig.cache_ttl_secs
│   ├── github/                            # GitHub API integration  [Phase 2]
│   │   ├── mod.rs                         # Module exports
│   │   ├── types.rs                       # GhIssue, Priority, MaestroLabel, SessionMode; label/body blocker parsing
│   │   ├── client.rs                      # GitHubClient trait, GhCliClient (gh CLI), MockGitHubClient; parse_issues_json
│   │   ├── labels.rs                      # LabelManager: ready→in-progress→done/failed lifecycle transitions
│   │   └── pr.rs                          # PrCreator: build_pr_body, create_for_issue auto-PR creation
│   ├── session/
│   │   ├── mod.rs                         # Module exports (includes pool, worktree)
│   │   ├── manager.rs                     # Claude CLI process management; worktree_path, system_prompt_appendix
│   │   ├── parser.rs                      # stream-json output parser; extracts file_path from tool input
│   │   ├── pool.rs                        # Session pool: max_concurrent, queue, auto-promote  [Phase 1]
│   │   ├── types.rs                       # Session state machine; ToolUse with file_path; issue_title field  [Phase 2]
│   │   └── worktree.rs                    # Git worktree isolation: WorktreeManager trait, GitWorktreeManager, MockWorktreeManager  [Phase 1]
│   ├── state/
│   │   ├── mod.rs                         # Module exports (includes file_claims)
│   │   ├── file_claims.rs                 # File claim system: FileClaimManager, conflict prevention  [Phase 1]
│   │   ├── store.rs                       # JSON state persistence
│   │   └── types.rs                       # State types; issue_cache, issue_cache_updated fields  [Phase 2]
│   ├── tui/
│   │   ├── mod.rs                         # Event loop with scroll keys and check_completions
│   │   ├── app.rs                         # App state; WorkAssigner, GhCliClient, Config fields  [Phase 2]
│   │   ├── activity_log.rs                # Scrollable activity log widget with LogLevel color coding  [Phase 1]
│   │   ├── panels.rs                      # Split-pane panel view for multiple agent sessions  [Phase 1]
│   │   └── ui.rs                          # ratatui rendering; delegates to panels and activity_log
│   └── work/                              # Work queue and scheduling  [Phase 2]
│       ├── mod.rs                         # Module exports
│       ├── types.rs                       # WorkItem, WorkStatus; from_issue, is_ready
│       ├── dependencies.rs               # DependencyGraph: topological sort, cycle detection
│       └── assigner.rs                    # WorkAssigner: priority ordering, next_ready, mark_done/failed/in_progress
├── template/
│   ├── README-TEMPLATE.md                 # Template usage instructions
│   └── .claude/                           # Reproducible template for new projects
│       ├── CLAUDE.md
│       ├── agents/                        # Template copies of all subagents
│       ├── commands/
│       │   ├── implement.md
│       │   └── validate-contracts.md
│       ├── hooks/
│       │   └── README.md
│       ├── settings.json
│       └── skills/                        # Template copies of core skills
│           ├── api-contract-validation/
│           ├── project-patterns/
│           └── security-patterns/
├── .gitignore                             # Includes .maestro/worktrees/
├── Cargo.lock                             # Dependency lock file
├── Cargo.toml                             # Rust package manifest and dependencies
├── LICENSE
├── README.md                              # Project front door
├── directory-tree.md                      # This file — SINGLE SOURCE OF TRUTH for structure
├── maestro-state.json                     # Runtime state persistence file
└── maestro.toml                           # Runtime configuration
```

## Quick Reference

| Path | Description |
|------|-------------|
| `.claude/` | Claude Code agent configuration |
| `.claude/agents/` | Subagent definitions |
| `.claude/commands/` | Slash command definitions |
| `.claude/hooks/` | Pre/post command notification hooks |
| `.claude/skills/` | Reusable knowledge bases for subagents |
| `src/` | Rust source code |
| `src/github/` | GitHub API integration (Phase 2) |
| `src/github/types.rs` | GhIssue, Priority, MaestroLabel, SessionMode |
| `src/github/client.rs` | GitHubClient trait, GhCliClient, MockGitHubClient |
| `src/github/labels.rs` | Issue label lifecycle transitions |
| `src/github/pr.rs` | Automated PR creation |
| `src/session/` | Claude CLI process and session lifecycle management |
| `src/session/pool.rs` | Concurrent session pool with queue and auto-promote |
| `src/session/worktree.rs` | Git worktree isolation per session |
| `src/state/` | State persistence and file conflict management |
| `src/state/file_claims.rs` | Per-session file claim registry |
| `src/tui/` | Terminal UI (ratatui) |
| `src/tui/activity_log.rs` | Scrollable log widget |
| `src/tui/panels.rs` | Split-pane multi-session view |
| `src/work/` | Work queue and dependency scheduling (Phase 2) |
| `src/work/types.rs` | WorkItem and WorkStatus types |
| `src/work/dependencies.rs` | Dependency graph, topological sort |
| `src/work/assigner.rs` | Priority-ordered work assignment |
| `template/` | Reproducible project template |
| `directory-tree.md` | This file |
| `maestro.toml` | Runtime configuration |
| `maestro-state.json` | Persisted session state |
