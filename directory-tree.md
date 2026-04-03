# Project Directory Tree

> Last updated: 2026-04-02 12:00 (UTC)
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
│   ├── worktrees/
│   │   └── bugfix                         # Worktree checkout for bugfix branch
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
├── .github/
│   └── workflows/
│       ├── ci.yml                         # GitHub Actions CI pipeline
│       └── release.yml                    # Release workflow: cross-platform builds, GitHub Release, Homebrew tap trigger
├── src/
│   ├── main.rs                            # CLI entry point (clap); Run, Queue, Add, Status, Cost, Init; module declarations (includes provider)  [Issue #29]
│   ├── config.rs                          # maestro.toml parsing; ModelsConfig, GatesConfig, ReviewConfig; ContextOverflowConfig; ProviderConfig (kind, organization, az_project); guardrail_prompt in SessionsConfig  [Issue #29, #43]
│   ├── budget.rs                          # BudgetEnforcer: per-session and global budget checks  [Phase 3]
│   ├── git.rs                             # GitOps trait, CliGitOps: commit and push operations  [Phase 3]
│   ├── models.rs                          # ModelRouter: label-based model routing  [Phase 3]
│   ├── prompts.rs                         # PromptBuilder: structured issue prompts with task-type detection; ProjectLanguage enum; detect_project_language(); default_guardrail(); resolve_guardrail()  [Phase 3, Issue #43]
│   ├── util.rs                            # Shared utilities (truncate, etc.)
│   ├── gates/                             # Completion gates framework  [Phase 3]
│   │   ├── mod.rs                         # Module exports
│   │   ├── types.rs                       # Gate types: TestsPass, FileExists, FileContains, PrCreated
│   │   └── runner.rs                      # Gate evaluation runner
│   ├── provider/                          # Multi-provider abstraction layer  [Issue #29]
│   │   ├── mod.rs                         # create_provider factory, detect_provider_from_remote
│   │   ├── types.rs                       # ProviderKind enum (Github, AzureDevops); re-exports Issue/Priority/MaestroLabel/SessionMode/Milestone  [Issue #31-33]
│   │   └── azure_devops.rs               # AzDevOpsClient implementing GitHubClient trait; parse_work_items_json; updated for new GhIssue fields  [Issue #31-33]
│   ├── github/                            # GitHub API integration  [Phase 2]
│   │   ├── mod.rs                         # Module exports
│   │   ├── types.rs                       # GhIssue (+ milestone/assignees fields), GhMilestone, Priority, MaestroLabel, SessionMode; label/body blocker parsing  [Issue #31-33]
│   │   ├── client.rs                      # GitHubClient trait, GhCliClient (gh CLI), MockGitHubClient; parse_issues_json updated for milestone/assignees  [Issue #31-33]
│   │   ├── labels.rs                      # LabelManager: ready→in-progress→done/failed lifecycle transitions
│   │   └── pr.rs                          # PrCreator: build_pr_body, create_for_issue auto-PR creation
│   ├── modes/                             # Session mode definitions and resolution  [Phase 3]
│   │   └── mod.rs                         # builtin_modes, resolve_mode, mode_from_labels
│   ├── notifications/                     # Interruption and notification system  [Phase 3]
│   │   ├── mod.rs                         # Module exports
│   │   ├── types.rs                       # Notification levels: Info, Warning, Critical, Blocker
│   │   └── dispatcher.rs                  # Notification dispatcher
│   ├── plugins/                           # Plugin and hook execution system  [Phase 3]
│   │   ├── mod.rs                         # Module exports
│   │   ├── hooks.rs                       # HookPoint enum: SessionStarted, SessionCompleted, TestsPassed, ContextOverflow, etc.  [Issue #12]
│   │   └── runner.rs                      # PluginRunner: executes external plugin commands per hook point
│   ├── review/                            # Review pipeline  [Phase 3]
│   │   ├── mod.rs                         # Module exports; re-exports ReviewConfig, ReviewDispatcher
│   │   ├── council.rs                     # ReviewCouncil: parallel multi-reviewer orchestration
│   │   └── dispatch.rs                    # ReviewDispatcher: single reviewer execution and config
│   ├── session/
│   │   ├── mod.rs                         # Module exports (includes pool, worktree, health, retry, context_monitor, fork)
│   │   ├── manager.rs                     # Claude CLI process management; handles ContextUpdate events  [Phase 3]
│   │   ├── parser.rs                      # stream-json output parser; parses system events for context usage  [Phase 3]
│   │   ├── pool.rs                        # Session pool: max_concurrent, queue, auto-promote; branch tracking; guardrail_prompt field; set_guardrail_prompt(); merged into system prompt in try_promote()  [Phase 3, Issue #43]
│   │   ├── types.rs                       # Session state machine; fork fields (parent_session_id, child_session_ids, fork_depth); ContextUpdate StreamEvent  [Phase 3]
│   │   ├── worktree.rs                    # Git worktree isolation: WorktreeManager trait, GitWorktreeManager, MockWorktreeManager  [Phase 1]
│   │   ├── health.rs                      # HealthMonitor: stall detection, HealthCheck trait  [Phase 3]
│   │   ├── retry.rs                       # RetryPolicy: configurable max retries and cooldown  [Phase 3]
│   │   ├── cleanup.rs                     # CleanupManager: orphaned worktree detection and removal  [Phase 3]
│   │   ├── logger.rs                      # SessionLogger: logs ContextUpdate events; per-session timestamped file logging  [Phase 3]
│   │   ├── context_monitor.rs             # ContextMonitor trait + ProductionContextMonitor: tracks per-session context usage, overflow and commit-prompt thresholds  [Issue #12]
│   │   └── fork.rs                        # SessionForker trait + ForkPolicy: auto-fork on overflow, continuation prompt builder, max depth enforcement  [Issue #12]
│   ├── state/
│   │   ├── mod.rs                         # Module exports (includes file_claims, progress)
│   │   ├── file_claims.rs                 # File claim system: FileClaimManager, conflict prevention  [Phase 1]
│   │   ├── progress.rs                    # SessionProgress: phase tracking (Analyzing, Implementing, Testing, CreatingPR)  [Phase 3]
│   │   ├── store.rs                       # JSON state persistence
│   │   └── types.rs                       # State types; fork_lineage HashMap; record_fork, fork_chain, fork_depth methods  [Issue #12]
│   ├── tui/
│   │   ├── mod.rs                         # Event loop; keybindings: Tab, Esc, Enter, 1-9, d; screen event delegation and handle_screen_action  [Phase 3, Issue #31-33]
│   │   ├── app.rs                         # App state; TuiMode enum: Dashboard, IssueBrowser, MilestoneView added; screen fields; configure() resolves guardrail prompt  [Issue #12, #31-33, #43]
│   │   ├── activity_log.rs                # Scrollable activity log widget with LogLevel color coding  [Phase 1]
│   │   ├── cost_dashboard.rs              # Cost dashboard widget: per-session and aggregate cost display  [Phase 3]
│   │   ├── dep_graph.rs                   # ASCII dependency graph visualization  [Phase 3]
│   │   ├── detail.rs                      # Session detail view  [Phase 3]
│   │   ├── fullscreen.rs                  # Fullscreen session view with phase progress overlay  [Phase 3]
│   │   ├── help.rs                        # Help overlay widget with keybinding reference  [Phase 3]
│   │   ├── panels.rs                      # Split-pane panel view; fork depth indicator in title; overflow warning in context gauge  [Issue #12]
│   │   ├── ui.rs                          # ratatui rendering; budget display, TUI mode switching, notification banners, screen rendering branches  [Phase 3, Issue #31-33]
│   │   └── screens/                       # Interactive screen components  [Issue #31-33]
│   │       ├── mod.rs                     # Screen types: ScreenAction enum, SessionConfig; re-exports HomeScreen, IssueBrowserScreen, MilestoneScreen
│   │       ├── home.rs                    # HomeScreen: idle dashboard, logo, quick-actions menu, recent sessions panel  [Issue #31]
│   │       ├── issue_browser.rs           # IssueBrowserScreen: navigable issue list, multi-select, label/milestone filters, preview pane  [Issue #32]
│   │       └── milestone.rs               # MilestoneScreen: milestone list, progress gauge, issue detail pane, run-all action  [Issue #33]
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
├── CHANGELOG.md                           # Release history following Keep a Changelog format
├── LICENSE
├── README.md                              # Project front door
├── ROADMAP.md                             # Project milestones and implementation order
├── directory-tree.md                      # This file — SINGLE SOURCE OF TRUTH for structure
├── maestro-state.json                     # Runtime state persistence file
└── maestro.toml                           # Runtime configuration; [sessions.context_overflow] section; guardrail_prompt option (commented)  [Issue #12, #43]
```

## Quick Reference

| Path | Description |
|------|-------------|
| `.github/workflows/ci.yml` | GitHub Actions CI pipeline |
| `.github/workflows/release.yml` | Release automation: build binaries, create GitHub Release, update Homebrew tap |
| `.claude/` | Claude Code agent configuration |
| `.claude/agents/` | Subagent definitions |
| `.claude/commands/` | Slash command definitions |
| `.claude/hooks/` | Pre/post command notification hooks |
| `.claude/skills/` | Reusable knowledge bases for subagents |
| `.claude/worktrees/` | Worktree checkouts managed by maestro |
| `src/` | Rust source code |
| `src/budget.rs` | Per-session and global budget enforcement (Phase 3) |
| `src/git.rs` | GitOps trait and CLI-backed commit+push (Phase 3) |
| `src/models.rs` | Label-based model routing (Phase 3) |
| `src/prompts.rs` | Structured issue prompt builder with task-type detection; ProjectLanguage detection; guardrail resolution (Phase 3, Issue #43) |
| `src/gates/` | Completion gates: TestsPass, FileExists, FileContains, PrCreated (Phase 3) |
| `src/provider/` | Multi-provider abstraction layer (Issue #29) |
| `src/provider/mod.rs` | create_provider factory; detect_provider_from_remote |
| `src/provider/types.rs` | ProviderKind enum; re-exports Issue/Priority/MaestroLabel/SessionMode/Milestone |
| `src/provider/azure_devops.rs` | AzDevOpsClient (`az` CLI); parse_work_items_json; extended for milestone/assignees fields |
| `src/github/` | GitHub API integration (Phase 2) |
| `src/github/types.rs` | GhIssue (milestone, assignees fields added), GhMilestone, Priority, MaestroLabel, SessionMode |
| `src/github/client.rs` | GitHubClient trait, GhCliClient, MockGitHubClient; parse_issues_json updated |
| `src/github/labels.rs` | Issue label lifecycle transitions |
| `src/github/pr.rs` | Automated PR creation |
| `src/modes/` | Session mode definitions: orchestrator, vibe, review (Phase 3) |
| `src/notifications/` | Interruption system with Info/Warning/Critical/Blocker levels (Phase 3) |
| `src/plugins/` | Plugin and hook execution system (Phase 3) |
| `src/plugins/hooks.rs` | HookPoint enum for plugin attachment points |
| `src/plugins/runner.rs` | External plugin command execution per hook point |
| `src/review/` | Review pipeline: single dispatcher and council orchestration (Phase 3) |
| `src/review/council.rs` | Parallel multi-reviewer council |
| `src/review/dispatch.rs` | Single reviewer execution and config |
| `src/session/` | Claude CLI process and session lifecycle management |
| `src/session/health.rs` | Stall detection and HealthCheck trait (Phase 3) |
| `src/session/retry.rs` | Configurable retry policy (Phase 3) |
| `src/session/pool.rs` | Concurrent session pool with queue and auto-promote; guardrail_prompt merged into system prompt (Issue #43) |
| `src/session/worktree.rs` | Git worktree isolation per session |
| `src/session/cleanup.rs` | Orphaned worktree detection and removal (Phase 3) |
| `src/session/logger.rs` | Per-session file logging to .maestro/logs/ (Phase 3) |
| `src/session/context_monitor.rs` | ContextMonitor trait + ProductionContextMonitor: per-session context tracking (Issue #12) |
| `src/session/fork.rs` | SessionForker trait + ForkPolicy: auto-fork on overflow, continuation prompt builder (Issue #12) |
| `src/state/` | State persistence and file conflict management |
| `src/state/file_claims.rs` | Per-session file claim registry |
| `src/state/progress.rs` | Session phase tracking (Phase 3) |
| `src/tui/` | Terminal UI (ratatui) |
| `src/tui/activity_log.rs` | Scrollable log widget |
| `src/tui/cost_dashboard.rs` | Per-session and aggregate cost display (Phase 3) |
| `src/tui/dep_graph.rs` | ASCII dependency graph visualization (Phase 3) |
| `src/tui/detail.rs` | Session detail view (Phase 3) |
| `src/tui/fullscreen.rs` | Fullscreen session view with phase progress overlay (Phase 3) |
| `src/tui/help.rs` | Help overlay widget with keybinding reference (Phase 3) |
| `src/tui/panels.rs` | Split-pane multi-session view |
| `src/tui/screens/` | Interactive TUI screen components (Issues #31-33) |
| `src/tui/screens/mod.rs` | `ScreenAction` enum, `SessionConfig`; re-exports all screen types |
| `src/tui/screens/home.rs` | `HomeScreen`: idle dashboard with logo, quick-actions, and recent activity (Issue #31) |
| `src/tui/screens/issue_browser.rs` | `IssueBrowserScreen`: navigable issue list with multi-select, label/milestone filters (Issue #32) |
| `src/tui/screens/milestone.rs` | `MilestoneScreen`: milestone list with progress gauge and run-all action (Issue #33) |
| `src/work/` | Work queue and dependency scheduling (Phase 2) |
| `src/work/dependencies.rs` | Dependency graph, topological sort |
| `src/work/assigner.rs` | Priority-ordered work assignment |
| `template/` | Reproducible project template |
| `CHANGELOG.md` | Release history |
| `ROADMAP.md` | Project milestones and implementation order |
| `directory-tree.md` | This file |
| `maestro.toml` | Runtime configuration |
| `maestro-state.json` | Persisted session state |
