# Project Directory Tree

> Last updated: 2026-04-04 19:00 (UTC)
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
│   ├── ISSUE_TEMPLATE/
│   │   ├── config.yml                     # Template chooser config (blank issues disabled)
│   │   ├── feature.yml                    # Feature request issue form with DOR
│   │   └── bug.yml                        # Bug report issue form with DOR
│   └── workflows/
│       ├── ci.yml                         # GitHub Actions CI pipeline
│       └── release.yml                    # Release workflow: cross-platform builds, GitHub Release, Homebrew tap trigger
├── src/
│   ├── main.rs                            # CLI entry point (clap); Run, Queue, Add, Status, Cost, Init, Doctor; --skip-doctor flag on Run subcommand bypasses preflight; cmd_run() runs validate_preflight() before session launch and uses PromptBuilder::build_issue_prompt() for issue sessions; setup_app_from_config() shared helper wires budget, model router, notifications, plugins, and permission_mode/allowed_tools from config; cmd_dashboard() performs orphan worktree cleanup, log cleanup, fetches username from doctor report, delegates App construction to setup_app_from_config(), and queues FetchSuggestionData on startup; declares #[cfg(test)] mod integration_tests  [Issue #15, #29, #49, #34, #36, #35, #52]
│   ├── config.rs                          # maestro.toml parsing; ModelsConfig, GatesConfig, ReviewConfig; ContextOverflowConfig; ProviderConfig (kind, organization, az_project); guardrail_prompt in SessionsConfig; CompletionGatesConfig and CompletionGateEntry; CiAutoFixConfig (enabled, max_retries, poll_interval_secs) under GatesConfig.ci_auto_fix; TuiConfig struct with optional theme field; Config gains tui field  [Issue #29, #40, #41, #43, #38]
│   ├── budget.rs                          # BudgetEnforcer: per-session and global budget checks  [Phase 3]
│   ├── doctor.rs                          # Preflight checks: CheckSeverity, CheckResult, DoctorReport, run_all_checks(), print_report(); validate_preflight() (public, fails fast on required check failures); build_claude_cli_result() (pub(crate), pure/testable); check_claude_cli() elevated to Required severity; build_gh_auth_result() (pure, testable); check_az_identity(); 10 check functions  [Issue #49, #34, #52]
│   ├── git.rs                             # GitOps trait, CliGitOps: commit and push operations  [Phase 3]
│   ├── models.rs                          # ModelRouter: label-based model routing  [Phase 3]
│   ├── prompts.rs                         # PromptBuilder: structured issue prompts with task-type detection; ProjectLanguage enum; detect_project_language(); default_guardrail(); resolve_guardrail()  [Phase 3, Issue #43]
│   ├── util.rs                            # Shared utilities (truncate, etc.)
│   ├── gates/                             # Completion gates framework  [Phase 3, Issue #40]
│   │   ├── mod.rs                         # Module exports
│   │   ├── types.rs                       # Gate types: TestsPass, FileExists, FileContains, PrCreated, Command; is_required(), display_name(), from_config_entry()
│   │   └── runner.rs                      # Gate evaluation runner; all_required_gates_passed(); Command match arm
│   ├── provider/                          # Multi-provider abstraction layer  [Issue #29]
│   │   ├── mod.rs                         # create_provider factory, detect_provider_from_remote
│   │   ├── types.rs                       # ProviderKind enum (Github, AzureDevops); re-exports Issue/Priority/MaestroLabel/SessionMode/Milestone  [Issue #31-33]
│   │   └── azure_devops.rs               # AzDevOpsClient implementing GitHubClient trait; parse_work_items_json; stub list_milestones()  [Issue #31-33, #47]
│   ├── github/                            # GitHub API integration  [Phase 2]
│   │   ├── mod.rs                         # Module exports
│   │   ├── types.rs                       # GhIssue (+ milestone/assignees fields), GhMilestone, Priority, MaestroLabel, SessionMode; label/body blocker parsing  [Issue #31-33]
│   │   ├── client.rs                      # GitHubClient trait + list_milestones(); GhCliClient; MockGitHubClient (set_milestones()); parse_issues_json; parse_milestones_json  [Issue #31-33, #46-48]
│   │   ├── ci.rs                          # CiChecker: check_pr_status(), fetch_failure_log(); CiStatus enum (Pending, Passed, Failed, NoneConfigured); CiPollAction enum (Wait, SpawnFix, Abandon); PendingPrCheck (fix_attempt, awaiting_fix_ci); build_ci_fix_prompt(); truncate_log(); parse_ci_json(); decide_ci_action()  [Phase 3, Issue #41]
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
│   │   ├── pool.rs                        # Session pool: max_concurrent, queue, auto-promote; branch tracking; guardrail_prompt field; set_guardrail_prompt(); merged into system prompt in try_promote(); find_by_issue_mut()  [Phase 3, Issue #40, #43]
│   │   ├── types.rs                       # Session state machine; fork fields (parent_session_id, child_session_ids, fork_depth); ContextUpdate StreamEvent; GatesRunning and NeedsReview status variants; CiFix variant; CiFixContext struct (pr_number, issue_number, branch, attempt); ci_fix_context field on Session  [Phase 3, Issue #40, #41]
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
│   │   ├── mod.rs                         # Event loop; keybindings; handle_screen_action() rewritten; command processing loop; launch_session_from_config(); FetchSuggestionData async handler spawns background GitHub fetch for ready/failed counts and milestone progress; pub mod theme  [Phase 3, Issue #31-33, #46-48, #35, #38]
│   │   ├── app.rs                         # App state; TuiMode; TuiCommand enum (FetchIssues, FetchMilestones, FetchSuggestionData, LaunchSession, LaunchSessions); TuiDataEvent enum (Issues, Milestones, Issue, SuggestionData); SuggestionDataPayload; handle_data_event(); data_tx/data_rx channel; pending_commands; check_completions() uses config-driven gates with per-gate activity logging; poll_ci_status() with CI auto-fix loop; spawn_ci_fix_session(); on_issue_session_completed() skips PR creation for CI-fix sessions; issue launch path uses PromptBuilder::build_issue_prompt(); theme: Theme field built from config in configure()  [Issue #12, #31-33, #35, #38, #40, #41, #43, #46-48, #52]
│   │   ├── theme.rs                       # Theme module: Theme struct (resolved color fields), ThemeConfig (preset + overrides), ThemePreset (Dark, Light), ThemeOverrides (per-field optional color overrides), SerializableColor (named/hex/indexed), ColorCapability; builds ratatui Color values from maestro.toml [tui.theme] block  [Issue #38]
│   │   ├── activity_log.rs                # Scrollable activity log widget with LogLevel color coding  [Phase 1]
│   │   ├── cost_dashboard.rs              # Cost dashboard widget: per-session and aggregate cost display  [Phase 3]
│   │   ├── dep_graph.rs                   # ASCII dependency graph visualization  [Phase 3]
│   │   ├── detail.rs                      # Session detail view  [Phase 3]
│   │   ├── fullscreen.rs                  # Fullscreen session view with phase progress overlay  [Phase 3]
│   │   ├── help.rs                        # Help overlay widget with keybinding reference  [Phase 3]
│   │   ├── panels.rs                      # Split-pane panel view; fork depth indicator in title; overflow warning in context gauge; GatesRunning (Cyan), NeedsReview (LightYellow), and CiFix (LightMagenta) status colors  [Issue #12, #40, #41]
│   │   ├── ui.rs                          # ratatui rendering; budget display, TUI mode switching, notification banners, screen rendering branches  [Phase 3, Issue #31-33]
│   │   ├── navigation/                    # Keyboard navigation and focus management  [Issue #37]
│   │   │   ├── mod.rs                     # Module exports for navigation subsystem
│   │   │   ├── focus.rs                   # Focus management: FocusManager, focus ring, widget focus state
│   │   │   └── keymap.rs                  # Keymap definitions: action-to-key bindings, context-sensitive keymaps
│   │   └── screens/                       # Interactive screen components  [Issue #31-33]
│   │       ├── mod.rs                     # Screen types: ScreenAction enum, SessionConfig; re-exports HomeScreen, IssueBrowserScreen, MilestoneScreen
│   │       ├── home.rs                    # HomeScreen: idle dashboard, logo, quick-actions menu, suggestions panel, recent activity panel; SuggestionKind enum, Suggestion struct, HomeSection enum; build_suggestions() derives contextual hints from GitHub data; draw_suggestions() renders Suggestions panel; Tab-based focus navigation between QuickActions and Suggestions; ProjectInfo gains username field  [Issue #31, #49, #34, #35]
│   │       ├── issue_browser.rs           # IssueBrowserScreen: navigable issue list, multi-select, label/milestone filters, preview pane; set_issues() for async data delivery  [Issue #32, #46]
│   │       └── milestone.rs               # MilestoneScreen: milestone list, progress gauge, issue detail pane, run-all action  [Issue #33]
│   ├── integration_tests/                 # End-to-end integration test suite (no external deps, all mocked)  [Issue #15]
│   │   ├── mod.rs                         # Module declarations; shared helpers: make_pool(), make_pool_with_worktree(), make_session(), make_session_with_issue(), make_gh_issue()
│   │   ├── session_lifecycle.rs           # 11 tests: enqueue/promote/complete lifecycle via handle_event()
│   │   ├── stream_parsing.rs              # 22 tests: stream event parsing and parser round-trips
│   │   ├── completion_pipeline.rs         # 9 tests: label transitions and PR creation
│   │   ├── concurrent_sessions.rs         # 6 tests: max_concurrent enforcement
│   │   └── worktree_lifecycle.rs          # 8 tests: worktree create/cleanup and health monitoring
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
└── maestro.toml                           # Runtime configuration; [sessions.context_overflow] section; guardrail_prompt option (commented); [sessions.completion_gates] with fmt, clippy, test defaults  [Issue #12, #40, #43]
```

## Quick Reference

| Path | Description |
|------|-------------|
| `.github/ISSUE_TEMPLATE/config.yml` | Template chooser config — blank issues disabled |
| `.github/ISSUE_TEMPLATE/feature.yml` | Feature request issue form with Definition of Ready fields |
| `.github/ISSUE_TEMPLATE/bug.yml` | Bug report issue form with Definition of Ready fields |
| `.github/workflows/ci.yml` | GitHub Actions CI pipeline |
| `.github/workflows/release.yml` | Release automation: build binaries, create GitHub Release, update Homebrew tap |
| `.claude/` | Claude Code agent configuration |
| `.claude/agents/` | Subagent definitions |
| `.claude/commands/` | Slash command definitions |
| `.claude/hooks/` | Pre/post command notification hooks |
| `.claude/skills/` | Reusable knowledge bases for subagents |
| `.claude/worktrees/` | Worktree checkouts managed by maestro |
| `src/` | Rust source code |
| `src/main.rs` | CLI entry point; `--skip-doctor` flag on `run` subcommand; `cmd_run()` calls `validate_preflight()` before launch and uses `PromptBuilder::build_issue_prompt()` for issue sessions; `setup_app_from_config()` shared App setup helper; `cmd_dashboard()` with startup cleanup, config-driven wiring, and `FetchSuggestionData` queued on startup (Issues #29, #34, #35, #36, #49, #52) |
| `src/budget.rs` | Per-session and global budget enforcement (Phase 3) |
| `src/doctor.rs` | Preflight check system: `CheckSeverity`, `CheckResult`, `DoctorReport`, `run_all_checks()`, `print_report()`; `validate_preflight()` fails fast if any required check fails; `build_claude_cli_result()` (pub(crate), pure/testable); `check_claude_cli()` is Required severity; `build_gh_auth_result()` (pure/testable); `check_az_identity()` for Azure DevOps (Issues #49, #34, #52) |
| `src/git.rs` | GitOps trait and CLI-backed commit+push (Phase 3) |
| `src/models.rs` | Label-based model routing (Phase 3) |
| `src/prompts.rs` | Structured issue prompt builder with task-type detection; ProjectLanguage detection; guardrail resolution (Phase 3, Issue #43) |
| `src/gates/` | Completion gates: TestsPass, FileExists, FileContains, PrCreated, Command (Phase 3, Issue #40) |
| `src/provider/` | Multi-provider abstraction layer (Issue #29) |
| `src/provider/mod.rs` | create_provider factory; detect_provider_from_remote |
| `src/provider/types.rs` | ProviderKind enum; re-exports Issue/Priority/MaestroLabel/SessionMode/Milestone |
| `src/provider/azure_devops.rs` | AzDevOpsClient (`az` CLI); parse_work_items_json; stub `list_milestones()` |
| `src/github/` | GitHub API integration (Phase 2) |
| `src/github/types.rs` | GhIssue (milestone, assignees fields added), GhMilestone, Priority, MaestroLabel, SessionMode |
| `src/github/client.rs` | GitHubClient trait + `list_milestones()`; GhCliClient; MockGitHubClient; `parse_issues_json`; `parse_milestones_json` |
| `src/github/ci.rs` | `CiChecker` (`check_pr_status`, `fetch_failure_log`); `CiStatus`; `CiPollAction`; `PendingPrCheck` (with `fix_attempt`, `awaiting_fix_ci`); `build_ci_fix_prompt`; `truncate_log`; `parse_ci_json`; `decide_ci_action` (Issue #41) |
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
| `src/session/pool.rs` | Concurrent session pool with queue and auto-promote; guardrail_prompt merged into system prompt; `find_by_issue_mut()` (Issue #40, #43) |
| `src/session/worktree.rs` | Git worktree isolation per session |
| `src/session/cleanup.rs` | Orphaned worktree detection and removal (Phase 3) |
| `src/session/logger.rs` | Per-session file logging to .maestro/logs/ (Phase 3) |
| `src/session/context_monitor.rs` | ContextMonitor trait + ProductionContextMonitor: per-session context tracking (Issue #12) |
| `src/session/fork.rs` | SessionForker trait + ForkPolicy: auto-fork on overflow, continuation prompt builder (Issue #12) |
| `src/state/` | State persistence and file conflict management |
| `src/state/file_claims.rs` | Per-session file claim registry |
| `src/state/progress.rs` | Session phase tracking (Phase 3) |
| `src/tui/` | Terminal UI (ratatui) |
| `src/tui/mod.rs` | Event loop; `handle_screen_action()`; command processing; `launch_session_from_config()`; `FetchSuggestionData` async handler for GitHub ready/failed counts and milestone progress; `pub mod theme` (Issues #31-33, #35, #38, #46-48) |
| `src/tui/app.rs` | `App` struct with `theme: Theme` field built in `configure()`; `TuiMode`; `TuiCommand` (adds `FetchSuggestionData`); `TuiDataEvent` (adds `SuggestionData`); `SuggestionDataPayload`; `handle_data_event()`; `data_tx`/`data_rx` channel; `check_completions()` config-driven gates with per-gate logging; `poll_ci_status()` with CI auto-fix loop; `spawn_ci_fix_session()`; `on_issue_session_completed()` skips PR creation for CI-fix sessions; issue launch uses `PromptBuilder::build_issue_prompt()` (Issues #12, #31-33, #35, #38, #40, #41, #43, #46-48, #52) |
| `src/tui/theme.rs` | `Theme` (resolved ratatui `Color` fields); `ThemeConfig` (`preset` + `overrides`); `ThemePreset` (`Dark`, `Light`); `ThemeOverrides` (per-field optional overrides); `SerializableColor` (named string / `#rrggbb` hex / 256-color index); `ColorCapability`; all 14 TUI rendering files consume theme fields instead of hardcoded `Color::` constants (Issue #38) |
| `src/tui/activity_log.rs` | Scrollable log widget |
| `src/tui/cost_dashboard.rs` | Per-session and aggregate cost display (Phase 3) |
| `src/tui/dep_graph.rs` | ASCII dependency graph visualization (Phase 3) |
| `src/tui/detail.rs` | Session detail view (Phase 3) |
| `src/tui/fullscreen.rs` | Fullscreen session view with phase progress overlay (Phase 3) |
| `src/tui/help.rs` | Help overlay widget with keybinding reference (Phase 3) |
| `src/tui/navigation/` | Keyboard navigation system and focus management (Issue #37) |
| `src/tui/navigation/mod.rs` | Module exports for navigation subsystem |
| `src/tui/navigation/focus.rs` | `FocusManager`: focus ring, widget focus state tracking |
| `src/tui/navigation/keymap.rs` | Keymap definitions: action-to-key bindings, context-sensitive keymaps |
| `src/tui/panels.rs` | Split-pane multi-session view; `GatesRunning` (Cyan), `NeedsReview` (LightYellow), and `CiFix` (LightMagenta) status colors (Issues #40, #41) |
| `src/tui/screens/` | Interactive TUI screen components (Issues #31-33) |
| `src/tui/screens/mod.rs` | `ScreenAction` enum, `SessionConfig`; re-exports all screen types |
| `src/tui/screens/home.rs` | `HomeScreen`: idle dashboard with 3-column layout (Quick Actions 30% / Suggestions 35% / Recent Activity 35%); `SuggestionKind` enum (`ReadyIssues`, `MilestoneProgress`, `IdleSessions`, `FailedIssues`); `Suggestion` struct with `build_suggestions()` factory; `HomeSection` enum for Tab-based focus toggle; `draw_suggestions()` renderer; `@username` display in project info bar (Issues #31, #34, #35, #49) |
| `src/tui/screens/issue_browser.rs` | `IssueBrowserScreen`: navigable issue list with multi-select, label/milestone filters; `set_issues()` (Issues #32, #46) |
| `src/tui/screens/milestone.rs` | `MilestoneScreen`: milestone list with progress gauge and run-all action (Issue #33) |
| `src/integration_tests/` | End-to-end integration test suite; 55 tests; MockGitHubClient and MockWorktreeManager; no external process dependencies (Issue #15) |
| `src/integration_tests/mod.rs` | Module declarations and shared helpers: `make_pool()`, `make_pool_with_worktree()`, `make_session()`, `make_session_with_issue()`, `make_gh_issue()` |
| `src/integration_tests/session_lifecycle.rs` | 11 tests covering enqueue, promote, and complete session lifecycle via `handle_event()` |
| `src/integration_tests/stream_parsing.rs` | 22 tests covering stream event parsing and parser round-trips |
| `src/integration_tests/completion_pipeline.rs` | 9 tests covering label transitions and PR creation |
| `src/integration_tests/concurrent_sessions.rs` | 6 tests covering `max_concurrent` enforcement |
| `src/integration_tests/worktree_lifecycle.rs` | 8 tests covering worktree create/cleanup and health monitoring |
| `src/work/` | Work queue and dependency scheduling (Phase 2) |
| `src/work/dependencies.rs` | Dependency graph, topological sort |
| `src/work/assigner.rs` | Priority-ordered work assignment |
| `template/` | Reproducible project template |
| `CHANGELOG.md` | Release history |
| `ROADMAP.md` | Project milestones and implementation order |
| `directory-tree.md` | This file |
| `maestro.toml` | Runtime configuration |
| `maestro-state.json` | Persisted session state |
