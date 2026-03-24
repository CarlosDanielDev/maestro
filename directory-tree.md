# Project Directory Tree

> Last updated: 2026-03-23 00:00 (UTC)
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
│   ├── main.rs                            # CLI entry point (clap); Run, Queue, Add, Status, Cost, Init; module declarations
│   ├── config.rs                          # maestro.toml parsing; ModelsConfig, GatesConfig, ReviewConfig; max_retries, retry_cooldown_secs
│   ├── budget.rs                          # BudgetEnforcer: per-session and global budget checks  [Phase 3]
│   ├── git.rs                             # GitOps trait, CliGitOps: commit and push operations  [Phase 3]
│   ├── models.rs                          # ModelRouter: label-based model routing  [Phase 3]
│   ├── review.rs                          # Review pipeline dispatcher  [Phase 3]
│   ├── util.rs                            # Shared utilities (truncate, etc.)
│   ├── gates/                             # Completion gates framework  [Phase 3]
│   │   ├── mod.rs                         # Module exports
│   │   ├── types.rs                       # Gate types: TestsPass, FileExists, FileContains, PrCreated
│   │   └── runner.rs                      # Gate evaluation runner
│   ├── github/                            # GitHub API integration  [Phase 2]
│   │   ├── mod.rs                         # Module exports
│   │   ├── types.rs                       # GhIssue, Priority, MaestroLabel, SessionMode; label/body blocker parsing
│   │   ├── client.rs                      # GitHubClient trait, GhCliClient (gh CLI), MockGitHubClient; parse_issues_json
│   │   ├── labels.rs                      # LabelManager: ready→in-progress→done/failed lifecycle transitions
│   │   └── pr.rs                          # PrCreator: build_pr_body, create_for_issue auto-PR creation
│   ├── notifications/                     # Interruption and notification system  [Phase 3]
│   │   ├── mod.rs                         # Module exports
│   │   ├── types.rs                       # Notification levels: Info, Warning, Critical, Blocker
│   │   └── dispatcher.rs                  # Notification dispatcher
│   ├── session/
│   │   ├── mod.rs                         # Module exports (includes pool, worktree, health, retry)
│   │   ├── manager.rs                     # Claude CLI process management; branch_name field  [Phase 3]
│   │   ├── parser.rs                      # stream-json output parser; extracts file_path from tool input
│   │   ├── pool.rs                        # Session pool: max_concurrent, queue, auto-promote; branch tracking  [Phase 3]
│   │   ├── types.rs                       # Session state machine; Stalled, Retrying variants; retry_count, last_retry_at  [Phase 3]
│   │   ├── worktree.rs                    # Git worktree isolation: WorktreeManager trait, GitWorktreeManager, MockWorktreeManager  [Phase 1]
│   │   ├── health.rs                      # HealthMonitor: stall detection, HealthCheck trait  [Phase 3]
│   │   └── retry.rs                       # RetryPolicy: configurable max retries and cooldown  [Phase 3]
│   ├── state/
│   │   ├── mod.rs                         # Module exports (includes file_claims, progress)
│   │   ├── file_claims.rs                 # File claim system: FileClaimManager, conflict prevention  [Phase 1]
│   │   ├── progress.rs                    # SessionProgress: phase tracking (Analyzing, Implementing, Testing, CreatingPR)  [Phase 3]
│   │   ├── store.rs                       # JSON state persistence
│   │   └── types.rs                       # State types; issue_cache, issue_cache_updated fields  [Phase 2]
│   ├── tui/
│   │   ├── mod.rs                         # Event loop; keybindings: Tab, Esc, Enter, 1-9, d  [Phase 3]
│   │   ├── app.rs                         # App state; health monitor, budget enforcer, model router, notifications, TuiMode, retry logic, gates  [Phase 3]
│   │   ├── activity_log.rs                # Scrollable activity log widget with LogLevel color coding  [Phase 1]
│   │   ├── dep_graph.rs                   # ASCII dependency graph visualization  [Phase 3]
│   │   ├── detail.rs                      # Session detail view  [Phase 3]
│   │   ├── panels.rs                      # Split-pane panel view; Stalled/Retrying colors, selected_index()  [Phase 3]
│   │   └── ui.rs                          # ratatui rendering; budget display, TUI mode switching, notification banners  [Phase 3]
│   └── work/                              # Work queue and scheduling  [Phase 2]
│       ├── mod.rs                         # Module exports
│       ├── types.rs                       # WorkItem, WorkStatus; from_issue, is_ready
│       ├── dependencies.rs               # DependencyGraph: topological sort, cycle detection
│       └── assigner.rs                    # WorkAssigner: topo sort tiebreaker, cycle detection  [Phase 3]
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
├── Cargo.toml                             # Rust package manifest; tempfile dev-dependency added
├── LICENSE
├── README.md                              # Project front door
├── directory-tree.md                      # This file — SINGLE SOURCE OF TRUTH for structure
├── maestro-state.json                     # Runtime state persistence file
└── maestro.toml                           # Runtime configuration; [models], [gates], [review] sections added
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
| `src/budget.rs` | Per-session and global budget enforcement (Phase 3) |
| `src/git.rs` | GitOps trait and CLI-backed commit+push (Phase 3) |
| `src/models.rs` | Label-based model routing (Phase 3) |
| `src/review.rs` | Review pipeline dispatcher (Phase 3) |
| `src/gates/` | Completion gates: TestsPass, FileExists, FileContains, PrCreated (Phase 3) |
| `src/github/` | GitHub API integration (Phase 2) |
| `src/github/client.rs` | GitHubClient trait, GhCliClient, MockGitHubClient |
| `src/github/labels.rs` | Issue label lifecycle transitions |
| `src/github/pr.rs` | Automated PR creation |
| `src/notifications/` | Interruption system with Info/Warning/Critical/Blocker levels (Phase 3) |
| `src/session/` | Claude CLI process and session lifecycle management |
| `src/session/health.rs` | Stall detection and HealthCheck trait (Phase 3) |
| `src/session/retry.rs` | Configurable retry policy (Phase 3) |
| `src/session/pool.rs` | Concurrent session pool with queue and auto-promote |
| `src/session/worktree.rs` | Git worktree isolation per session |
| `src/state/` | State persistence and file conflict management |
| `src/state/file_claims.rs` | Per-session file claim registry |
| `src/state/progress.rs` | Session phase tracking (Phase 3) |
| `src/tui/` | Terminal UI (ratatui) |
| `src/tui/activity_log.rs` | Scrollable log widget |
| `src/tui/dep_graph.rs` | ASCII dependency graph visualization (Phase 3) |
| `src/tui/detail.rs` | Session detail view (Phase 3) |
| `src/tui/panels.rs` | Split-pane multi-session view |
| `src/work/` | Work queue and dependency scheduling (Phase 2) |
| `src/work/dependencies.rs` | Dependency graph, topological sort |
| `src/work/assigner.rs` | Priority-ordered work assignment |
| `template/` | Reproducible project template |
| `directory-tree.md` | This file |
| `maestro.toml` | Runtime configuration |
| `maestro-state.json` | Persisted session state |
