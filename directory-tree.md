# Project Directory Tree

> Last updated: 2026-04-27 15:00 (UTC)
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
│   │   ├── subagent-gatekeeper.md         # DOR, blockers, and API-contract gatekeeper for /implement
│   │   ├── subagent-idea-triager.md       # Idea inbox triage subagent
│   │   ├── subagent-master-planner.md     # System-level planning subagent
│   │   ├── subagent-qa.md                 # QA and test design subagent
│   │   └── subagent-security-analyst.md   # Security review subagent
│   ├── commands/
│   │   ├── create-subagent.md             # Slash command: scaffold a new subagent
│   │   ├── implement.md                   # Slash command: run full TDD implementation flow
│   │   ├── plan-feature.md                # Slash command: invoke master planner
│   │   ├── pushup.md                      # Slash command: git push workflow
│   │   ├── release.md                     # Slash command: semantic version release — bump version, update changelog, tag, push, and create GitHub Release
│   │   ├── setup-notifications.md         # Slash command: configure hook notifications
│   │   ├── setup-project.md               # Slash command: initialize project config
│   │   ├── triage-idea.md                 # Slash command: non-mutating idea triage loop (fetch idea issue → dispatch subagent-idea-triager → validate report → render digest)  [Issue #485]
│   │   ├── update-from-template.md        # Slash command: sync from template directory
│   │   ├── validate-contracts.md          # Slash command: validate API contracts
│   │   └── video-frames.md                # Slash command: extract video frames
│   ├── hooks/
│   │   ├── README.md                      # Hook usage documentation
│   │   ├── fixtures/                      # Fixture inputs for smoke-testing parser hooks
│   │   │   ├── idea_triager_fail_bad_enum.txt     # Negative fixture: valid JSON but unsupported recommendation enum value
│   │   │   ├── idea_triager_fail_missing_fence.txt # Negative fixture: plain prose with no fenced block
│   │   │   └── idea_triager_pass.txt              # Golden-path fixture: full valid idea-triager report
│   │   ├── implement-gates.sh             # Pre-implementation gate: DOR / blocker / contract checks
│   │   ├── notify.ps1                     # Windows notification hook
│   │   ├── notify.sh                      # Unix notification hook
│   │   ├── parse_gatekeeper_report.py     # Validates and re-emits gatekeeper JSON reports (exit 1 on contract violation)
│   │   ├── parse_idea_triager_report.py   # Validates and re-emits idea-triager JSON reports; enforces fence, version, enums (exit 1 on violation)  [Issue #484]
│   │   └── preflight.sh                   # Preflight environment checks run before session launch
│   ├── settings.json                      # Claude Code project settings
│   ├── settings.local.json                # Local overrides (not committed)
│   ├── worktrees/
│   │   └── bugfix                         # Worktree checkout for bugfix branch
│   └── skills/
│       ├── README.md                      # Skills system documentation
│       ├── api-contract-validation/
│       │   └── SKILL.md                   # API contract enforcement patterns
│       ├── caveman/
│       │   └── SKILL.md                   # Compressed-prose response style (opt-in via behavior.caveman_mode)
│       ├── example-backend-patterns/
│       │   └── SKILL.md                   # Example backend skill template
│       ├── example-frontend-patterns/
│       │   └── SKILL.md                   # Example frontend skill template
│       ├── example-mobile-patterns/
│       │   └── SKILL.md                   # Example mobile skill template
│       ├── project-patterns/
│       │   └── SKILL.md                   # Maestro-specific patterns and conventions
│       ├── provider-resilience/
│       │   └── SKILL.md                   # Defensive gh/az CLI patterns, error handling, idempotency
│       ├── security-patterns/
│       │   └── SKILL.md                   # OWASP Top 10 and security best practices
│       └── video-frame-extractor/
│           └── SKILL.md                   # Video frame extraction patterns
├── .github/
│   ├── ISSUE_TEMPLATE/
│   │   ├── config.yml                     # Template chooser config (blank issues disabled)
│   │   ├── bug.yml                        # Bug report issue form with DOR; Blocked By required
│   │   ├── feature.yml                    # Feature request issue form with DOR; Blocked By required; Dependency Graph field
│   │   └── idea.yml                       # Idea inbox issue form — 5 required textareas (itch + Q1-Q4) + Q5 vision-alignment dropdown
│   └── workflows/
│       ├── ci.yml                         # GitHub Actions CI pipeline
│       └── release.yml                    # Release automation for cross-platform builds and Homebrew tap updates
├── build.rs                               # Build script: generates man page (maestro.1) and shell completions (bash, zsh, fish) into OUT_DIR at build time using clap_mangen and clap_complete  [Issue #18]
├── src/
│   ├── lib.rs                             # Library facade; exposes session::parser and session::types for benchmark crates; pub mod icon_mode and pub mod icons added so shared icon modules are accessible as library crate items  [Issue #307, #308]
│   ├── icon_mode.rs                       # Shared icon mode detection: AtomicBool global flag, init_from_config() reads tui.ascii_icons from Config and MAESTRO_ASCII_ICONS env var, use_nerd_font() returns current mode; extracted from tui/icons.rs so non-TUI crates can query the mode without pulling in the full TUI tree  [Issue #307]
│   ├── icons.rs                           # Shared icon registry: IconId enum (38 variants across Navigation, Status, UI Chrome, Indicators categories, plus NeedsReview variant added in #308), IconPair struct (nerd: &'static str, ascii: &'static str), icon_pair() const fn compiles to a zero-allocation jump table, get(IconId) returns the correct variant based on global mode, get_for_mode(id, nerd_font) pure testable variant; extracted from tui/icons.rs; CheckboxOn codepoint U+F14A (nf-fa-check_square) and CheckboxOff codepoint U+F0C8 (nf-fa-square) — universally present FA-core glyphs replacing the legacy nf-oct variants  [Issue #308, #433]
│   ├── main.rs                            # CLI entry point (clap); Run, Queue, Add, Status, Cost, Init, Doctor; --skip-doctor flag on Run subcommand bypasses preflight; cmd_run() runs validate_preflight() before session launch and uses PromptBuilder::build_issue_prompt() for issue sessions; setup_app_from_config() shared helper wires budget, model router, notifications, plugins, and permission_mode/allowed_tools from config; propagates once_mode from parsed CLI flag into App; forces max_concurrent=1 when --continuous is set; cmd_dashboard() performs orphan worktree cleanup, log cleanup, fetches username from doctor report, delegates App construction to setup_app_from_config(), and queues FetchSuggestionData on startup; declares #[cfg(test)] mod integration_tests; declares mod updater; declares mod flags; propagates startup gh auth check result into App.gh_auth_ok; declares #[cfg(feature = "experimental-sanitizer")] mod sanitizer; constructs FeatureFlags from --enable-flag / --disable-flag CLI args merged with [flags] config  [Issue #15, #29, #49, #34, #36, #35, #52, #83, #85, #118, #141, #142, #143, #158]
│   ├── cli.rs                             # CLI definition extracted from main.rs; Cli struct and Commands enum (clap derive); --once flag on Run subcommand (exits after all sessions complete, for CI/scripting); --continuous / -C flag on Run subcommand (auto-advance through issues, pause on failure); --enable-flag / --disable-flag repeatable args on Run subcommand for runtime feature flag overrides; --bypass-review global flag (session-only, skips review council); generate_completions() and cmd_completions() for shell tab-completion output; cmd_mangen() for roff man page generation; Completions and Mangen subcommands  [Issue #18, #83, #85, #143, #328]
│   ├── commands/                          # Command handler modules (one per CLI subcommand)
│   │   ├── mod.rs                         # Module re-exports
│   │   ├── clean.rs                       # cmd_clean(): prune orphaned worktrees and stale log files
│   │   ├── dashboard.rs                   # cmd_dashboard(): launch the TUI dashboard
│   │   ├── doctor.rs                      # cmd_doctor(): run preflight checks and print report
│   │   ├── init.rs                        # cmd_init(): scaffold maestro.toml in the project root
│   │   ├── logs.rs                        # cmd_logs(): stream or tail session log files
│   │   ├── queue.rs                       # cmd_queue(): interactive work-queue management
│   │   ├── resume.rs                      # cmd_resume(): re-attach to a paused session
│   │   ├── run.rs                         # cmd_run(): validate preflight then launch a session
│   │   ├── setup.rs                       # cmd_setup(): guided first-run configuration wizard
│   │   ├── slack.rs                       # cmd_slack(): test Slack webhook notification delivery
│   │   ├── slash.rs                       # SlashCommandRunner: executes /review and other slash commands against a PR; integrates with review::parse to extract the maestro-review JSON block  [Issue #327]
│   │   ├── status.rs                      # cmd_status(): print current session and queue state
│   │   └── turboquant.rs                  # cmd_turboquant(): run TurboQuant compression diagnostics
│   ├── config.rs                          # maestro.toml parsing; ModelsConfig, GatesConfig, ReviewConfig; ContextOverflowConfig; ProviderConfig (kind, organization, az_project); guardrail_prompt in SessionsConfig; CompletionGatesConfig and CompletionGateEntry; CiAutoFixConfig (enabled, max_retries, poll_interval_secs) under GatesConfig.ci_auto_fix; TuiConfig struct with optional theme field and mascot_style field ("sprite" | "ascii", default "sprite"); Config gains tui field; FlagsConfig (flattened HashMap<String, bool>) loaded from [flags] table; Config gains flags field; HollowRetryPolicy enum (Always/IntentAware/Never), HollowRetryConfig struct (policy, work_max_retries, consultation_max_retries), merge_legacy_hollow() for backward-compat TOML parsing, SessionsConfigRaw shadow struct for custom Deserialize; LoadedConfig { config: Config, path: PathBuf } struct returned by find_and_load_with_path() and find_and_load_in_with_path() so callers have the resolved file path; legacy find_and_load() and find_and_load_in() kept as thin shims  [Issue #29, #40, #41, #43, #38, #143, #275, #437, #473]
│   ├── continuous.rs                      # ContinuousModeState and ContinuousFailure structs; state machine for --continuous / -C flag: auto-advances to next ready issue, pauses loop on failure waiting for user decision (skip / retry / quit)  [Issue #85]
│   ├── budget.rs                          # BudgetEnforcer: per-session and global budget checks  [Phase 3]
│   ├── doctor.rs                          # Preflight checks: CheckSeverity, CheckResult, DoctorReport, run_all_checks(), print_report(); validate_preflight() (public, fails fast on required check failures); build_claude_cli_result() (pub(crate), pure/testable); check_claude_cli() elevated to Required severity; build_gh_auth_result() (pure, testable); check_az_identity(); 10 check functions  [Issue #49, #34, #52]
│   ├── git.rs                             # GitOps trait, CliGitOps: commit and push operations; list_remote_branches() on GitOps trait and CliGitOps impl — filters remote refs by prefix for orphan branch detection  [Phase 3, Issue #159]
│   ├── models.rs                          # ModelRouter: label-based model routing  [Phase 3]
│   ├── prompts.rs                         # PromptBuilder: structured issue prompts with task-type detection; ProjectLanguage enum; detect_project_language(); default_guardrail(); resolve_guardrail()  [Phase 3, Issue #43]
│   ├── util.rs                            # Shared utilities (truncate, etc.)
│   ├── sanitizer.rs                       # Placeholder sanitizer module; compiled only when `--features experimental-sanitizer` is set  [Issue #142]
│   ├── settings/                          # Settings persistence layer: reads/writes .claude/settings.json  [Issue #490]
│   │   ├── mod.rs                         # Module facade; re-exports SettingsStore, FsSettingsStore, CavemanModeState
│   │   ├── claude_settings.rs             # CavemanModeState enum (ExplicitTrue/ExplicitFalse/Default/Error); SettingsStore trait; FsSettingsStore impl with atomic writer for .claude/settings.json; toggle_caveman_mode()
│   │   └── claude_settings_tests.rs       # Sibling test module (attached via #[path]); unit tests for CavemanModeState parse/round-trip and FsSettingsStore read/write/toggle
│   ├── flags/                             # Feature flag registry and runtime store  [Issue #141, #146]
│   │   ├── mod.rs                         # Flag enum (6 variants); FlagSource enum (Default, Config, Cli); serde serialization; default_enabled(), description(), name(), all() helpers
│   │   └── store.rs                       # FeatureFlags store; source tracking per flag; HashMap-based resolution: CLI override > config file > compile-time defaults; source(), all_with_source() methods
│   ├── adapt/                             # Adapt pipeline: onboard existing projects to maestro workflow  [Issue #87-95, #371]
│   │   ├── mod.rs                         # Module exports; cmd_adapt() CLI entry point; adapt pipeline orchestration including scaffold phase  [Issue #371]
│   │   ├── types.rs                       # AdaptPlan, AdaptReport, TechDebtItem, AdaptConfig, ScaffoldFileStatus, ScaffoldedFile, ScaffoldResult type definitions  [Issue #371]
│   │   ├── scanner.rs                     # Project scanner Phase 1: detect language, framework, existing issues, CI config
│   │   ├── analyzer.rs                    # Claude-backed analyzer Phase 2: builds structured adapt plan from scan results
│   │   ├── planner.rs                     # Adaptation planner Phase 3: maps analyzer output to actionable plan steps
│   │   ├── materializer.rs               # Plan materializer Phase 4: creates GitHub issues and milestones; GhMaterializer struct; ensure_labels() auto-creates missing labels before issue creation; STANDARD_LABEL_COLORS constant defines canonical hex colors for all maestro labels  [Issue #93, #348]
│   │   ├── scaffolder.rs                  # Scaffold phase: ProjectScaffolder trait, ClaudeScaffolder impl, write_scaffold_files(); generates project files from adapt plan  [Issue #371]
│   │   ├── prompts.rs                     # Claude prompt builders for analyzer, planner, and scaffold phases  [Issue #371]
│   │   └── knowledge.rs                   # Knowledge-base compression (Phase 2.6): consumes AdaptReport + ProjectProfile; produces KnowledgeBase (six token-budgeted sections); write_knowledge_file() writes .maestro/knowledge.md; auto-loaded by SessionPool::try_promote as a system-prompt component; 1 MiB size cap, symlink rejection, TOCTOU-safe load, envelope-wrapped injection  [Issue #347]
│   ├── updater/                           # Self-upgrade subsystem  [Issue #118]
│   │   ├── mod.rs                         # UpgradeState state machine (Idle, Checking, UpdateAvailable, Downloading, Installing, Done, Failed); ReleaseInfo type (tag_name, download_url, body)
│   │   ├── checker.rs                     # UpdateChecker trait; GitHubReleaseChecker (hits GitHub Releases API); version parsing via semver comparison; asset names use Rust target triples (e.g. aarch64-apple-darwin); checksum file resolves to sha256sums.txt; check_for_update() async entry point  [Issue #118, #233]
│   │   ├── installer.rs                   # Binary replacement with pre-install backup; atomic swap via temp file; tar.gz archives extracted via flate2 + tar pipeline; restart_with_same_args() re-execs the process with original argv after upgrade completes  [Issue #118, #233]
│   │   └── restart.rs                     # RestartBuilder and RestartCommand: pure, testable command construction for post-upgrade re-exec; no side effects until .execute() is called
│   ├── gates/                             # Completion gates framework  [Phase 3, Issue #40]
│   │   ├── mod.rs                         # Module exports
│   │   ├── types.rs                       # Gate types: TestsPass, FileExists, FileContains, PrCreated, Command; is_required(), display_name(), from_config_entry(); GateResult derives Serialize/Deserialize for session persistence  [Issue #104]
│   │   └── runner.rs                      # Gate evaluation runner; all_required_gates_passed(); Command match arm
│   ├── provider/                          # Multi-provider abstraction layer  [Issue #29]
│   │   ├── mod.rs                         # create_provider factory, detect_provider_from_remote
│   │   ├── types.rs                       # ProviderKind enum (Github, AzureDevops); re-exports Issue/Priority/MaestroLabel/SessionMode/Milestone  [Issue #31-33]
│   │   └── azure_devops.rs               # AzDevOpsClient implementing GitHubClient trait; parse_work_items_json; stub list_milestones(); stub list_labels() and create_label() to satisfy GitHubClient trait  [Issue #31-33, #47, #348]
│   ├── github/                            # GitHub API integration  [Phase 2]
│   │   ├── mod.rs                         # Module exports
│   │   ├── types.rs                       # GhIssue (+ milestone/assignees fields), GhMilestone, Priority, MaestroLabel, SessionMode; label/body blocker parsing; PendingPr struct (issue_number, branch, attempt, last_error, status, retry_after); PendingPrStatus enum (RetryScheduled, Retrying, AwaitingManualRetry)  [Issue #31-33, #159]
│   │   ├── client.rs                      # GitHubClient trait + list_milestones(); GhCliClient; MockGitHubClient (set_milestones()); parse_issues_json; parse_milestones_json; is_auth_error(); is_gh_auth_error(); auth error detection in run_gh() surfaces gh CLI authentication failures; list_prs_for_branch() on GitHubClient trait — returns open PR numbers for a given head branch; MockGitHubClient gains set_list_prs_for_branch() helper; list_labels() and create_label() on GitHubClient trait — enumerate and create repo labels; MockGitHubClient gains set_list_labels_error(), set_create_label_error(), list_labels_call_count(), create_label_calls() helpers  [Issue #31-33, #46-48, #158, #159, #348]
│   │   ├── ci.rs                          # CiCheck trait (check_pr_status, check_pr_details, fetch_failure_log); CiChecker impl; CiStatus enum (Pending, Passed, Failed, NoneConfigured); CheckStatus enum (Queued, InProgress, Completed, Waiting, Pending, Requested, Unknown) with serde aliases; CheckConclusion enum (Success, Failure, Neutral, Cancelled, TimedOut, ActionRequired, Skipped, Stale, StartupFailure, None) with serde aliases; CheckRunDetail struct (name, status, conclusion, started_at, elapsed_secs); CiPollAction enum (Wait, SpawnFix, Abandon); PendingPrCheck (fix_attempt, awaiting_fix_ci); build_ci_fix_prompt(); truncate_log(); parse_ci_json(); parse_check_details(); decide_ci_action()  [Phase 3, Issue #41, #123]
│   │   ├── labels.rs                      # LabelManager: ready→in-progress→done/failed lifecycle transitions
│   │   ├── merge.rs                       # PrMergeCheck trait (mockable); PrMergeChecker impl using `gh pr view` + `git diff`; MergeState enum (Clean, Conflicting, Blocked, Unknown); PrConflictStatus struct; parse_merge_json(); parse_conflicting_files(); build_conflict_fix_prompt()
│   │   └── pr.rs                          # PrCreator: build_pr_body, create_for_issue auto-PR creation; PrRetryPolicy (max_attempts, base_delay_secs, multiplier) with exponential back-off via delay_for_attempt(); OrphanBranch struct with from_branch_name() — parses issue number from maestro/issue-N branch names  [Issue #159]
│   ├── mascot/                            # Pixel-art and ASCII mascot rendering subsystem  [Issue #473-476]
│   │   ├── mod.rs                         # Module facade; MascotStyle enum (Sprite | Ascii) re-exported; pub mod sprites
│   │   ├── animator.rs                    # Frame-advance animation timer for mascot sequences
│   │   ├── frames.rs                      # AsciiMascotFrames (renamed from MascotFrames); MASCOT_ROWS_ASCII / MASCOT_WIDTH_ASCII constants (old MASCOT_ROWS / MASCOT_WIDTH aliases removed)  [Issue #476]
│   │   ├── state.rs                       # MascotState: tracks current animation state and frame index
│   │   ├── tests.rs                       # Unit tests for mascot module
│   │   ├── widget.rs                      # MascotWidget; style: MascotStyle field; with_style() builder; render_sprite() path (128×128 pixel grid) and render_ascii() path  [Issue #473]
│   │   ├── sprites.rs                     # sprite() / pixel() accessors; embeds 128×128 RGBA byte arrays from sprites/ at compile time  [Issue #474]
│   │   └── sprites/                       # Compiled pixel-art sprite data (128×128 px each)  [Issue #474]
│   │       ├── conducting.bin
│   │       ├── error.bin
│   │       ├── happy.bin
│   │       ├── idle.bin
│   │       ├── sleeping.bin
│   │       └── thinking.bin
│   ├── modes/                             # Session mode definitions and resolution  [Phase 3]
│   │   └── mod.rs                         # builtin_modes, resolve_mode, mode_from_labels
│   ├── notifications/                     # Interruption and notification system  [Phase 3]
│   │   ├── mod.rs                         # Module exports
│   │   ├── types.rs                       # Notification levels: Info, Warning, Critical, Blocker
│   │   ├── dispatcher.rs                  # Notification dispatcher
│   │   └── slack.rs                       # Slack webhook notification sender
│   ├── plugins/                           # Plugin and hook execution system  [Phase 3]
│   │   ├── mod.rs                         # Module exports
│   │   ├── hooks.rs                       # HookPoint enum: SessionStarted, SessionCompleted, TestsPassed, ContextOverflow, etc.  [Issue #12]
│   │   └── runner.rs                      # PluginRunner: executes external plugin commands per hook point
│   ├── prd/                               # PRD model, persistence, and markdown export  [Issue #321]
│   │   ├── mod.rs                         # Module facade; re-exports Prd, PrdStore, PrdExporter
│   │   ├── model.rs                       # Prd struct and field types; serde Serialize/Deserialize
│   │   ├── store.rs                       # PrdStore: JSON persistence under .maestro/prd/
│   │   └── export.rs                      # PrdExporter: renders a Prd to a markdown document
│   ├── review/                            # Review pipeline  [Phase 3, Issue #327, #328]
│   │   ├── mod.rs                         # Module exports; re-exports ReviewConfig, ReviewDispatcher
│   │   ├── apply.rs                       # apply_review(): applies accepted concern patches to the worktree  [Issue #327]
│   │   ├── audit.rs                       # ReviewAudit: records accept/reject decisions and writes audit log  [Issue #327]
│   │   ├── bypass.rs                      # BypassGuard: enforces --bypass-review policy; logs bypass events  [Issue #328]
│   │   ├── council.rs                     # ReviewCouncil: parallel multi-reviewer orchestration
│   │   ├── dispatch.rs                    # ReviewDispatcher: single reviewer execution and config
│   │   ├── parse.rs                       # parse_review_comment(): extracts maestro-review JSON block from PR comment body  [Issue #327]
│   │   └── types.rs                       # ReviewReport, Concern, ConcernSeverity, ReviewOutcome types; schema mirrors docs/api-contracts/review-comment.json  [Issue #327]
│   ├── session/
│   │   ├── mod.rs                         # Module exports (includes pool, worktree, health, retry, context_monitor, fork)
│   │   ├── manager.rs                     # Claude CLI process management; handles ContextUpdate events; thinking_start field tracks Thinking block duration; handle_event() emits rich activity messages with file paths, elapsed times for tool calls, and thinking duration on block end; current_activity reflects "Thinking..." while a thinking block is active; emits "STATUS: OLD → NEW" activity log entries when session state changes  [Phase 3, Issue #102, #202]
│   │   ├── parser.rs                      # stream-json output parser; parses system events for context usage; parses "thinking" message type into StreamEvent::Thinking; extracts command field from Bash tool input as command_preview (truncated to 60 chars)  [Phase 3, Issue #102]
│   │   ├── pool.rs                        # Session pool: max_concurrent, queue, auto-promote; branch tracking; guardrail_prompt field; set_guardrail_prompt(); merged into system prompt in try_promote(); find_by_issue_mut(); decrements flash_counter on each session per render tick and emits STATUS activity log entries on state transitions  [Phase 3, Issue #40, #43, #202]
│   │   ├── pr_capture.rs                  # PrCapture: intercepts stream-json output to detect when a session posts a /review PR comment and stores the raw comment body for the review pipeline  [Issue #327]
│   │   ├── types.rs                       # Session state machine; fork fields (parent_session_id, child_session_ids, fork_depth); ContextUpdate StreamEvent; GatesRunning and NeedsReview status variants; CiFix variant; CiFixContext struct (pr_number, issue_number, branch, attempt); ci_fix_context field on Session; StreamEvent::Thinking { text } variant; command_preview: Option<String> field on StreamEvent::ToolUse; GateResultEntry struct (gate, passed, message); gate_results: Vec<GateResultEntry> field on Session; NeedsPr variant — non-terminal status indicating PR creation failed and is queued for retry; flash_counter: u8 field on Session — decremented each render tick to drive border-flash effect on state transition  [Phase 3, Issue #40, #41, #102, #104, #159, #202]
│   │   ├── worktree.rs                    # Git worktree isolation: WorktreeManager trait, GitWorktreeManager, MockWorktreeManager  [Phase 1]
│   │   ├── health.rs                      # HealthMonitor: stall detection, HealthCheck trait  [Phase 3]
│   │   ├── retry.rs                       # RetryPolicy: configurable max retries and cooldown; hollow field owns HollowRetryConfig (replaces flat hollow_max_retries); effective_max() dispatches by policy + session intent; 18 unit tests  [Phase 3, Issue #275]
│   │   ├── cleanup.rs                     # CleanupManager: orphaned worktree detection and removal  [Phase 3]
│   │   ├── image.rs                       # Image attachment helpers: VALID_IMAGE_EXTENSIONS constant, path validation, base64 encoding for multimodal session prompts
│   │   ├── logger.rs                      # SessionLogger: logs ContextUpdate events; logs Thinking events with "THINKING:" prefix; per-session timestamped file logging  [Phase 3, Issue #102]
│   │   ├── context_monitor.rs             # ContextMonitor trait + ProductionContextMonitor: tracks per-session context usage, overflow and commit-prompt thresholds  [Issue #12]
│   │   └── fork.rs                        # SessionForker trait + ForkPolicy: auto-fork on overflow, continuation prompt builder, max depth enforcement  [Issue #12]
│   ├── state/
│   │   ├── mod.rs                         # Module exports (includes file_claims, progress)
│   │   ├── file_claims.rs                 # File claim system: FileClaimManager, conflict prevention  [Phase 1]
│   │   ├── progress.rs                    # SessionProgress: phase tracking (Analyzing, Implementing, Testing, CreatingPR)  [Phase 3]
│   │   ├── store.rs                       # JSON state persistence
│   │   └── types.rs                       # State types; fork_lineage HashMap; record_fork, fork_chain, fork_depth methods; pending_prs: Vec<PendingPr> field on MaestroState — persisted to JSON state for PR retry recovery  [Issue #12, #159]
│   ├── tui/
│   │   ├── mod.rs                         # Event loop; keybindings; handle_screen_action() rewritten; command processing loop; launch_session_from_config(); FetchSuggestionData async handler spawns background GitHub fetch for ready/failed counts and milestone progress; spawns async version check on startup via check_for_update() — result delivered as VersionCheckResult data event; key handlers for upgrade flow (confirm/decline banner); CompletionSummary key-intercept branch: [f] collects NeedsReview sessions and calls spawn_gate_fix_session() for each then transitions to Overview, [i] opens issue browser, [r] opens prompt input, [l] switches to Overview (activity log view), [Enter]/[Esc] returns to dashboard via transition_to_dashboard(), [q] quits; ContinuousPause key-intercept overlay: [s] skip, [r] retry, [q] quit continuous loop; RefreshSuggestions branch sets loading_suggestions=true and queues FetchSuggestionData; exit path checks once_mode — exits immediately when true, otherwise shows CompletionSummary overlay; "All Issues" navigation always creates a fresh IssueBrowserScreen to prevent stale milestone filters leaking across navigation contexts; PromptInputScreen always created with injected history so Up/Down arrow recall works correctly; F-key bar actions wired (F1–F10, Alt-X); per-tick flash_counter decrement dispatched to session pool; pub mod theme; pub mod widgets; RunAdaptScaffold command dispatch  [Phase 3, Issue #31-33, #46-48, #35, #38, #83, #84, #85, #86, #104, #117, #118, #124, #202, #218, #232, #371]
│   │   ├── app/                           # App state module (split across multiple files)
│   │   │   ├── mod.rs                     # App struct; nav_stack: NavigationStack field (replaces confirm_exit_return_mode); navigate_to(), navigate_back(), navigate_back_or_dashboard(), navigate_to_root() navigation methods; gh_auth_ok: bool; theme: Theme; pending_prs: Vec<PendingPr>; config_path: Option<PathBuf> field carries the resolved maestro.toml path for settings save; set_config_path() setter; process_pending_pr_retries(); trigger_manual_pr_retry(); mascot_style: MascotStyle field hydrated in apply_config()  [Issue #12, #31-33, #35, #38, #40, #41, #43, #46-48, #52, #83, #84, #85, #86, #102, #104, #118, #123, #158, #159, #342, #437, #473]
│   │   │   ├── types.rs                   # TuiMode enum (+ CompletionSummary, ContinuousPause variants) with breadcrumb_label() method; NavigationStack struct (push/pop/peek/clear/breadcrumbs, cap 32); TuiCommand enum (+ RunAdaptScaffold); TuiDataEvent enum (+ AdaptScaffoldResult); SuggestionDataPayload; CompletionSummaryData; CompletionSessionLine; GateFailureInfo  [Issue #342, #371]
│   │   │   ├── budget.rs                  # Budget enforcement helpers within App
│   │   │   ├── ci_polling.rs              # poll_ci_status() CI auto-fix loop using CiCheck trait; decide_ci_action(); spawn_ci_fix_session()  [Issue #41, #123]
│   │   │   ├── completion_pipeline.rs     # check_completions() config-driven gate evaluation with per-gate logging  [Issue #40, #104]
│   │   │   ├── completion_summary.rs      # build_completion_summary(); transition_to_dashboard() calls navigate_to_root() to clear nav stack  [Issue #342]
│   │   │   ├── context_overflow.rs        # Context overflow detection and fork triggering
│   │   │   ├── data_handler.rs            # handle_data_event(); data_tx/data_rx channel; SuggestionData, VersionCheckResult, UpgradeResult, AdaptScaffoldResult handlers  [Issue #371]
│   │   │   ├── event_handler.rs           # Top-level event dispatch and tick handling
│   │   │   ├── helpers.rs                 # Shared App helper utilities
│   │   │   ├── issue_completion.rs        # on_issue_session_completed(); skips PR creation for CI-fix sessions
│   │   │   ├── plugins.rs                 # Hook point invocation via PluginRunner
│   │   │   ├── pr_retry.rs                # process_pending_pr_retries() exponential back-off; trigger_manual_pr_retry()  [Issue #159]
│   │   │   ├── review.rs                  # ReviewCouncil integration and gate-fix session spawning
│   │   │   ├── session_lifecycle.rs       # Session promotion, state transitions, activity log forwarding
│   │   │   ├── session_spawners.rs        # spawn_gate_fix_session(); build_gate_fix_prompt(); launch_session_from_config()
│   │   │   ├── tests.rs                   # App-level unit tests
│   │   │   └── work_assigner.rs           # WorkAssigner integration: topo-sort, issue queueing
│   │   ├── theme.rs                       # Theme module: Theme struct (resolved color fields), ThemeConfig (preset + overrides), ThemePreset (Dark, Light), ThemeOverrides (per-field optional color overrides), SerializableColor (named/hex/indexed), ColorCapability; fkey_badge_bg and fkey_badge_fg optional override fields for F-key bar badge styling; milestone_gauge_color() derives a completion-aware color (green=high, yellow=mid, red=low) with inverted semantics relative to budget gauges; builds ratatui Color values from maestro.toml [tui.theme] block  [Issue #38, #218, #299]
│   │   ├── activity_log.rs                # Scrollable activity log widget with LogLevel color coding; LogLevel::Thinking variant (green / accent_success color, distinct from Error)  [Phase 1, Issue #102]
│   │   ├── cost_dashboard.rs              # Cost dashboard widget: per-session and aggregate cost display  [Phase 3]
│   │   ├── dep_graph.rs                   # ASCII dependency graph visualization  [Phase 3]
│   │   ├── detail.rs                      # Session detail view  [Phase 3]
│   │   ├── fullscreen.rs                  # Fullscreen session view with phase progress overlay  [Phase 3]
│   │   ├── help.rs                        # Help overlay widget with keybinding reference  [Phase 3]
│   │   ├── icons.rs                       # Thin re-export shim: re-exports IconId, IconPair, icon_pair(), get(), get_for_mode() from src/icons.rs and init_from_config(), use_nerd_font() from src/icon_mode.rs so existing tui:: import paths remain valid after the registry was extracted  [Issue #307, #308]
│   │   ├── input_handler.rs               # Top-level key event dispatcher extracted from mod.rs; KeyAction enum (Consumed, Quit); handle_key() dispatches to overlay handlers, mode-specific input, global shortcuts, and screen dispatch in priority order; all Esc handlers use navigate_back_or_dashboard() via NavigationStack  [Issue #342]
│   │   ├── log_viewer.rs                  # Full-screen scrollable log viewer widget
│   │   ├── markdown.rs                    # markdown-to-ratatui rendering module; convert Markdown content to terminal-friendly widgets; wrap_and_push_text() performs width-aware word wrapping when appending text spans to a line buffer
│   │   ├── marquee.rs                     # Horizontally scrolling marquee text widget
│   │   ├── panels.rs                      # Split-pane panel view; fork depth indicator in title; overflow warning in context gauge; GatesRunning (Cyan), NeedsReview (LightYellow), and CiFix (LightMagenta) status colors; panel_border_type() returns thick borders for the focused grid panel; ▸ indicator rendered on the selected panel title; border flashes (amber) for 4 render ticks when flash_counter > 0 on state transition  [Issue #12, #40, #41, #202]
│   │   ├── ui.rs                          # ratatui rendering; budget display, TUI mode switching, notification banners, screen rendering branches; draw_upgrade_banner() renders upgrade notification states (available, downloading, installing, done, failed) as a top-of-screen banner with version info and [y]/[n] confirmation prompts; draw_gh_auth_warning() renders a persistent top-of-screen banner when gh CLI is not authenticated; CompletionSummary render branch and draw_completion_overlay() centred overlay with per-session outcome rows, PR links (underlined), error summaries, per-gate failure lines (✗ gate_name message in warning/error colors), and keybindings bar ([f] Fix when has_needs_review(), [i] [r] [l] [q] [Esc]); ContinuousPause render branch and continuous pause overlay; bottom bar split into info bar (agent count, cost, elapsed) and DOS-style F-key legend bar; draw_fkey_bar() renders amber-badged key names (F1–F10, Alt-X) with responsive width truncation; HelpBarContext struct drives context-aware keybinding dimming in the help bar; breadcrumb trail rendered in status bar from nav_stack.breadcrumbs() using TuiMode::breadcrumb_label(); should_show_dashboard_mascot_panel() / dashboard_mascot_panel_width() style-aware panel gate; passes MascotStyle through draw_mascot_block(), HomeScreen::set_mascot(), LandingScreen::set_mascot()  [Phase 3, Issue #31-33, #83, #84, #85, #104, #118, #158, #218, #342, #473]
│   │   ├── navigation/                    # Keyboard navigation and focus management  [Issue #37]
│   │   │   ├── mod.rs                     # Module exports for navigation subsystem
│   │   │   ├── focus.rs                   # Focus management: FocusManager, focus ring, widget focus state
│   │   │   ├── keymap.rs                  # Keymap definitions: action-to-key bindings, context-sensitive keymaps; F-key bar actions registered (F1 Help, F2 Summary, F3 Full, F4 Costs, F5 Tokens, F6 Deps, F9 Pause, F10 Kill, Alt-X Exit); KeyBindingGroup, InlineHint, FKeyRelevance, ModeKeyMap, global_keybindings() LazyLock  [Issue #218]
│   │   │   └── mode_hints.rs              # mode_keymap() builds ModeKeyMap for a given TuiMode + optional session status; maps TuiMode variants to mode labels, F-key visibility rules, and context-sensitive inline hints; consumes screen_bindings from KeymapProvider::keybindings()
│   │   ├── session_summary.rs             # Session summary widget rendered in the completion overlay and detail pane
│   │   ├── session_switcher.rs            # Session switcher overlay for jumping between active sessions
│   │   ├── splash.rs                      # Startup splash screen rendered before the TUI loop begins
│   │   ├── spinner.rs                     # Braille spinner animation helpers: spinner_frame(), format_thinking_elapsed(), spinner activity string builder
│   │   ├── summary.rs                     # Compact per-session summary row widget used in panel and list views
│   │   ├── token_dashboard.rs             # Token usage dashboard widget: per-session and aggregate token counts; TQ Ratio column removed (#346)
│   │   ├── turboquant_dashboard.rs        # TurboQuant savings dashboard: classify_savings(), aggregate_savings(), AggregateSavings; renders "Estimated Savings (projection)" header when no real handoff data exists, "Actual Savings" when at least one session has fork-handoff compression metrics; ACTUAL / proj. kind markers per row  [Issue #346]
│   │   ├── snapshot_tests/                # TUI snapshot tests using insta (41 tests, 9 views)  [Issue #16, #490]
│   │   │   ├── mod.rs                     # Module declarations for snapshot test submodules
│   │   │   ├── overview.rs                # 6 snapshot tests for PanelView (empty, single, multiple, selected, context overflow, forked)
│   │   │   ├── detail.rs                  # 6 snapshot tests for DetailView (basic, progress, activity log, no files, retries, markdown)
│   │   │   ├── fullscreen.rs              # 4 snapshot tests for FullscreenView (markdown, plain text, empty placeholder, auto-scroll)
│   │   │   ├── dashboard.rs               # 4 snapshot tests for HomeScreen (baseline, warnings, suggestions, selected action)
│   │   │   ├── issue_browser.rs           # 7 snapshot tests for IssueBrowserScreen (with issues, empty, loading, multi-select, filter, prompt overlays)
│   │   │   ├── milestone.rs               # 4 snapshot tests for MilestoneScreen (with milestones, empty, loading, detail pane); snapshots updated to reflect color hierarchy and selection visibility changes  [Issue #299]
│   │   │   ├── cost_dashboard.rs          # 5 snapshot tests for CostDashboard (no budget, under threshold, over 90%, empty, sorted)
│   │   │   ├── turboquant_dashboard.rs    # 3 snapshot tests for TurboQuantDashboard (projections-only, mixed actual+projections, empty sessions)  [Issue #346]
│   │   │   ├── caveman_row.rs             # 5 snapshot tests for caveman_row in SettingsScreen (explicit_true, explicit_false, default, error, focused_explicit_true)  [Issue #490]
│   │   │   └── snapshots/                 # Committed insta snapshot files (.snap files); includes caveman_row renders (default, error, explicit_false, explicit_true, focused_explicit_true)  [Issue #490]
│   │   ├── screen_dispatch.rs             # ScreenDispatch: routes key events and render calls to the active screen; constructor receives FeatureFlags for settings screen injection; always injects prompt history when constructing PromptInputScreen; ScreenAction::Push delegates to navigate_to(), ScreenAction::Pop delegates to navigate_back(); Scaffolding case in StartAdaptPipeline dispatch; reads app.config_path directly for settings save (removed relative-path probe at TuiMode::Settings); tracing::warn! when config_path is absent  [Issue #146, #232, #342, #371, #437]
│   │   └── screens/                       # Interactive screen components  [Issue #31-33]
│   │       ├── mod.rs                     # Screen types: ScreenAction enum (+ RefreshSuggestions variant), SessionConfig; re-exports HomeScreen, IssueBrowserScreen, MilestoneScreen; pub mod wizard_fields (added #447); wizard_paste removed  [Issue #31-33, #86, #447]
│   │       ├── adapt_follow_up.rs         # AdaptFollowUp: post-scaffold follow-up prompt screen
│   │       ├── bypass_warning.rs          # BypassWarningScreen: confirmation overlay shown when --bypass-review is active; displays policy summary and requires explicit acknowledgement before proceeding  [Issue #328]
│   │       ├── hollow_retry.rs            # HollowRetryScreen: minimal retry prompt overlay shown when a session stalls and user confirmation is required
│   │       ├── milestone.rs               # MilestoneScreen: milestone list, progress gauge, issue detail pane, run-all action; selected row uses SLOW_BLINK modifier for visibility; border color derived from selection state; progress gauge fill color uses milestone_gauge_color() (green=high completion, red=low); gauge empty portion dimmed; status counts (open/closed/in-progress) rendered BOLD; issue list uses visual hierarchy to distinguish selected vs unselected items  [Issue #33, #299]
│   │       ├── prompt_input.rs            # PromptInputScreen: free-text prompt entry; Enter submits, Shift+Enter/Alt+Enter inserts newline via insert_newline() (not input()), Ctrl+V pastes from clipboard (image or text), Esc cancels; Up/Down arrows navigate prompt history (injected at construction); image attachment list with [a]/[d]; keybinds bar always visible; uses wrap::soft_wrap_lines() for word-wrapped rendering  [Issue #101, #232, #263]
│   │       ├── queue_confirmation.rs      # QueueConfirmationScreen: confirmation overlay before bulk-queuing selected issues from the issue browser
│   │       ├── wizard_fields.rs           # Shared tui-textarea helpers: TextAreaField wraps tui_textarea::TextArea with single-line enforcement and insert_sanitized() paste path; WizardFields manages a fixed-size array of TextAreaField; strips Bidi overrides (U+202A-E, U+2066-9), Unicode line/paragraph separators (U+2028, U+2029), and BOM (U+FEFF) per CVE-2021-42574  [Issue #447]
│   │       ├── wizard_fields_tests.rs     # Inline unit tests for wizard_fields (split into sibling file to stay under the 400-LOC cap)  [Issue #447]
│   │       ├── wrap.rs                    # Soft-wrap utilities: soft_wrap_lines() splits a multi-line string into display lines that fit within a given column width using unicode-width for correct grapheme measurement  [Issue #263]
│   │       ├── adapt/                     # Adapt wizard screen components  [Issue #88, #371]
│   │       │   ├── mod.rs                 # AdaptScreen struct with Screen trait impl; wizard entry point; complete_scaffold(), set_scaffold_result()  [Issue #371]
│   │       │   ├── types.rs               # AdaptStep (+ Scaffolding variant), AdaptWizardConfig, AdaptResults (+ scaffold field), AdaptError  [Issue #371]
│   │       │   └── draw.rs                # ratatui rendering for adapt wizard steps and layout; scaffold phase rendering  [Issue #371]
│   │       ├── home/                      # Home screen components
│   │       │   ├── mod.rs                 # HomeScreen: idle dashboard, logo, quick-actions menu, suggestions panel, recent activity panel; SuggestionKind enum, Suggestion struct, HomeSection enum; build_suggestions() derives contextual hints from GitHub data; loading_suggestions bool field; R key emits RefreshSuggestions; Tab-based focus navigation; set_mascot() takes MascotStyle param  [Issue #31, #49, #34, #35, #86, #473]
│   │       │   ├── draw.rs                # ratatui rendering for home screen layout and panels; draw_suggestions() renders Suggestions panel with "Loading..." placeholder
│   │       │   └── types.rs               # HomeSection, SuggestionKind, Suggestion, ProjectInfo types (username field)
│   │       ├── issue_browser/             # Issue browser screen components
│   │       │   ├── mod.rs                 # IssueBrowserScreen: navigable issue list, multi-select, label/milestone filters, preview pane; set_issues() for async data delivery; reapply_filters() honours active filters on new data  [Issue #32, #46, #117]
│   │       │   └── draw.rs                # ratatui rendering for issue browser layout and panels
│   │       ├── issue_wizard/              # Issue creation wizard screen components  [Issue #447]
│   │       │   ├── mod.rs                 # IssueWizardScreen: multi-step wizard using WizardFields; sync_fields_into_payload(), rebuild_fields_for_step(), field_text(), refresh_field_blocks(); improve state fields and lifecycle methods  [Issue #447, #450]
│   │       │   ├── types.rs               # IssueWizardStep state machine and form payload types
│   │       │   ├── ai_improve.rs          # Improve prompt builder + JSON parser for AI-rewrite flow; pure logic, no I/O  [Issue #450]
│   │       │   ├── ai_review.rs           # AI-assisted review step: calls LLM to review draft issue fields before submission
│   │       │   ├── draw.rs                # ratatui rendering; renders TextArea widgets via refresh_field_blocks() mutable draw entry point  [Issue #447]
│   │       │   ├── draw_ai_review.rs      # Renders AiReview step and its improve sub-states (loading / error / diff / default review)  [Issue #450]
│   │       │   ├── draw_diff.rs           # 8-field red/green before-after diff renderer  [Issue #450]
│   │       │   └── prompt_common.rs       # Shared format_payload_for_prompt used by both review and improve flows  [Issue #450]
│   │       ├── landing/                   # Landing screen components
│   │       │   ├── mod.rs                 # LandingScreen struct with Screen trait impl; set_mascot() takes MascotStyle param  [Issue #473]
│   │       │   ├── types.rs               # Landing screen type definitions
│   │       │   └── draw.rs                # ratatui rendering for landing screen; picks MascotWidget style (sprite 32×16 vs ascii 11×6 canvas) based on MascotStyle  [Issue #473]
│   │       ├── milestone_wizard/          # Milestone creation wizard screen components  [Issue #447]
│   │       │   ├── mod.rs                 # MilestoneWizardScreen: three persistent TextAreaFields (goal_field, non_goals_field, doc_buffer_field)  [Issue #447]
│   │       │   ├── types.rs               # MilestoneWizardStep state machine and form payload types
│   │       │   ├── ai_planning.rs         # AI-assisted planning step: calls LLM to generate milestone dependency graph
│   │       │   └── draw.rs                # ratatui rendering; doc-refs step splits committed list / in-progress buffer / help hint  [Issue #447]
│   │       ├── pr_review/                 # PR review screen components
│   │       │   ├── mod.rs                 # PrReviewScreen struct with Screen trait impl
│   │       │   ├── types.rs               # PrReviewStep state machine, ReviewForm types
│   │       │   └── draw.rs                # ratatui rendering logic with markdown integration
│   │       ├── project_stats/             # Project statistics screen components
│   │       │   ├── mod.rs                 # ProjectStatsScreen struct with Screen trait impl
│   │       │   ├── types.rs               # Project stats type definitions
│   │       │   └── draw.rs                # ratatui rendering for project statistics display
│   │       ├── release_notes/             # Release notes screen components
│   │       │   ├── mod.rs                 # ReleaseNotesScreen struct with Screen trait impl
│   │       │   └── draw.rs                # ratatui rendering for release notes display
│   │       ├── roadmap/                   # Roadmap screen (v0.16.0 foundation)  [Issue #329]
│   │       │   ├── mod.rs                 # RoadmapScreen struct with Screen trait impl; renders milestones as a swimlane timeline
│   │       │   └── dep_levels.rs          # DepLevels: groups milestones and issues by dependency level for the roadmap layout
│   │       └── settings/                  # Settings screen components  [Issue #124, #146]
│   │           ├── mod.rs                 # SettingsScreen: interactive settings screen with tabbed TUI widget system; Flags tab displays all feature flags with name, on/off state, source (Default/Config/Cli), and description in read-only mode; focused fields rendered with green accent; Sessions tab gains hollow-retry widgets: [policy] dropdown (always/intent-aware/never), [work_max_retries] stepper, [consultation_max_retries] stepper; footer built from focused widget's edit_hint() so edit keys (Space/Enter/←→) are always advertised; KeymapProvider::keybindings() gains a third "Edit" group for consistent ? help overlay; save_config returns Err via let-else when config_path is None; Ctrl+S surfaces failures as a 5-second title-bar flash (save_error_flash: Option<(String, Instant)> field) rendered as "Settings [Save failed: <msg>]" in accent_error; with_caveman_mode() builder, sync hook for Space-toggling caveman mode, status flash in the title bar, Space → caveman binding in the help overlay  [Issue #275, #432, #437, #490]
│   │           ├── caveman_row.rs         # TUI render helper for the caveman-mode settings row; four visual states: ExplicitTrue (green checkbox + label), ExplicitFalse (dim checkbox), Default (grey "inherits settings.json"), Error (red warning); consumed by SettingsScreen  [Issue #490]
│   │           └── validation.rs          # Settings field validation helpers
│   │   └── widgets/                       # Reusable TUI widget components  [Issue #124]
│   │       ├── mod.rs                     # Module re-exports for all widgets; WidgetKind::edit_hint() returns a contextual (key, label) tuple per variant used by SettingsScreen to build the footer  [Issue #432]
│   │       ├── bypass_indicator.rs        # BypassIndicatorWidget: small status badge rendered in the F-key bar when --bypass-review is active, warning the user that the review council is disabled  [Issue #328]
│   │       ├── ci_monitor.rs              # CiMonitorWidget: compact bordered box rendering live CI check-run status for a PR; status icons, check names, elapsed times, and a summary footer
│   │       ├── dropdown.rs                # Dropdown selection widget with keyboard navigation
│   │       ├── list_editor.rs             # Editable list widget for adding and removing string items
│   │       ├── number_stepper.rs          # Numeric increment/decrement stepper widget
│   │       ├── text_input.rs              # Single-line text input widget with cursor support
│   │       └── toggle.rs                 # Boolean toggle widget for settings and forms; draw() routes through icons::get(IconId::CheckboxOn/Off) instead of hardcoded literals, eliminating the DRY drift that caused blank indicators on iTerm2 + some Nerd Font installs  [Issue #433]
│   ├── integration_tests/                 # End-to-end integration test suite (no external deps, all mocked)  [Issue #15]
│   │   ├── mod.rs                         # Module declarations; shared helpers: make_pool(), make_pool_with_worktree(), make_session(), make_session_with_issue(), make_gh_issue()
│   │   ├── session_lifecycle.rs           # 11 tests: enqueue/promote/complete lifecycle via handle_event()
│   │   ├── stream_parsing.rs              # 22 tests: stream event parsing and parser round-trips
│   │   ├── completion_pipeline.rs         # 9 tests: label transitions and PR creation
│   │   ├── concurrent_sessions.rs         # 6 tests: max_concurrent enforcement
│   │   ├── worktree_lifecycle.rs          # 8 tests: worktree create/cleanup and health monitoring
│   │   └── upgrade.rs                     # End-to-end upgrade flow tests: version check, banner states, installer backup/swap, restart command construction  [Issue #118]
│   ├── turboquant/                         # TurboQuant — vector quantization for context compression  [Issue #242-253, #343-345, #347]
│   │   ├── mod.rs                         # Module facade; combines PolarQuant + QJL into a unified API
│   │   ├── types.rs                       # QuantStrategy enum (TurboQuant, PolarQuant, QJL); TurboQuantConfig (+ fork_handoff_budget, system_prompt_budget, knowledge_budget); QuantResult; CompressionMetrics
│   │   ├── polar.rs                       # PolarQuant — recursive polar decomposition quantizer; preserves angular similarity (cosine distance)
│   │   ├── qjl.rs                         # Quantized Johnson-Lindenstrauss (QJL) — 1-bit residual projection; seeded deterministic sketch
│   │   ├── pipeline.rs                    # Two-stage quantization pipeline (PolarQuant + QJL); strategy-aware wrappers for quantization and dot-product estimation
│   │   ├── adapter.rs                     # TurboQuantAdapter: bridges quantization pipeline to session layer; TextRanker trait + impl; compress_handoff() (CompressedHandoff, Issue #343); compact_system_prompt() (Issue #344); compact_session_history() + StateCompactionReport (Issue #345); shared Arc<TurboQuantAdapter> on App; project_savings(), session_savings(), implied_rate_per_token() and public types SavingsProjection, SavingsKind, SessionSavings  [Issue #346]
│   │   └── budget.rs                      # TokenBudget helper: ranked-segment selection staying under a token limit; BudgetSelection struct (indices, tokens_used, truncated_first); used by fork-handoff, system-prompt, and knowledge compression  [Issue #343-345, #347]
│   └── work/                              # Work queue and scheduling  [Phase 2]
│       ├── mod.rs                         # Module exports; pub mod queue
│       ├── types.rs                       # WorkItem, WorkStatus; from_issue, is_ready
│       ├── assigner.rs                    # WorkAssigner: topo sort tiebreaker, cycle detection; mark_pending() transitions an item back to Pending; mark_pending_undo_cascade() cascades undo to dependents  [Phase 3, Issue #85]
│       ├── conflicts.rs                   # Conflict detection for concurrent work items
│       ├── dependencies.rs               # DependencyGraph: topological sort, cycle detection
│       ├── executor.rs                    # QueueExecutor state machine for sequential queue execution; ExecutorPhase enum (Idle, Running, AwaitingDecision, Finished); ExecutorItem struct; QueueItemState enum; FailureAction enum (Retry, Skip, Abort); advance(), mark_success(), mark_failure(), apply_decision(), set_session_id()
│       └── queue.rs                       # WorkQueue, QueuedItem, QueueValidationError; validate_selection()  [Issue #65]
├── docs/
│   ├── api-contracts/                     # JSON Schema (Draft 2020-12) for every external payload that crosses a process boundary; one file per payload type; referenced by /validate-contracts and subagent-gatekeeper
│   │   ├── README.md                      # Convention guide: naming, additionalProperties policy, gatekeeper integration  [Issue #327]
│   │   └── review-comment.json            # Schema for the maestro-review JSON block in /review PR comments; parsed by review::parse and TUI pr_review screen  [Issue #327]
│   ├── ci-smoke-check.md                  # CI smoke-check test harness guide
│   ├── FOLLOW-UPS.md                      # Pending hardening and security follow-up items (non-blocking, filed as issues before next release)
│   ├── harness-acceptance.md              # Acceptance criteria for the test harness
│   ├── layers-debt.txt                    # Layer-boundary debt notes
│   ├── RUST-GUARDRAILS.md                 # Rust coding policy and guardrails (single source of truth)
│   ├── tech-debt-catalog.md               # Automated tech-debt catalog (generated by maestro adapt)
│   └── superpowers/                       # Superpowers feature documentation
│       ├── plans/                         # Implementation plans
│       │   ├── 2026-04-21-implement-harness-enforcement-plan.md
│       │   └── 2026-04-22-ci-quality-gates-plan.md
│       └── specs/                         # Feature specifications
│           ├── 2026-04-21-implement-harness-enforcement-design.md
│           └── 2026-04-22-ci-quality-gates-design.md
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
├── scripts/                               # Project-level shell scripts for architecture checks, coverage, and verification gates
│   ├── allowlist-large-files.txt          # Allowlist for large files exempted from size checks
│   ├── architecture-layers.yml            # Layer dependency rules for check-layers.sh
│   ├── check-coverage-tiers.sh            # Validate test-coverage tier thresholds
│   ├── check-file-size.sh                 # Enforce per-file LOC limits (500-line rule)
│   ├── check-layers.sh                    # Enforce architecture layer boundaries
│   ├── coverage-tiers.yml                 # Coverage tier definitions
│   └── verify-issue-485.sh                # TDD verification harness: 27 grep/awk assertions for /triage-idea command + CLAUDE.md registry entries  [Issue #485]
├── tests/                                 # Cargo integration tests (run as a separate binary, full crate access)
│   ├── settings_caveman.rs                # Integration tests for FsSettingsStore against real tempfiles: read/write/toggle round-trips for caveman mode, missing-key defaults, malformed JSON handling  [Issue #490]
│   ├── gatekeeper/                        # Gatekeeper harness fixtures and tests
│   ├── hooks/                             # Hook script tests
│   ├── manifests/                         # Test manifest fixtures
│   └── scripts/                           # Test script fixtures
├── .gitignore                             # Includes .maestro/worktrees/ and runtime artifacts; .maestro/knowledge.md (written by maestro adapt, auto-loaded as system-prompt component by SessionPool::try_promote) is also excluded
├── Cargo.lock                             # Dependency lock file
├── Cargo.toml                             # Rust package manifest; tempfile and insta dev-dependencies; optimized release profile; [features] section with experimental-sanitizer = []; flate2 and tar dependencies added for tar.gz archive extraction in self-updater  [Issue #142, #233]
├── CHANGELOG.md                           # Release history following Keep a Changelog format
├── LICENSE
├── README.md                              # Project front door
├── ROADMAP.md                             # Project milestones and implementation order
├── SECURITY.md                            # Security policy: supported versions, vulnerability reporting, and disclosure process
├── directory-tree.md                      # This file — SINGLE SOURCE OF TRUTH for structure
├── maestro-state.json                     # Runtime state persistence file
└── maestro.toml                           # Runtime configuration; [sessions.context_overflow] section; guardrail_prompt option (commented); [sessions.completion_gates] with fmt, clippy, test defaults; [sessions.hollow_retry] section (policy, work_max_retries, consultation_max_retries)  [Issue #12, #40, #43, #275]
```

## Quick Reference

| Path | Description |
|------|-------------|
| `.github/ISSUE_TEMPLATE/config.yml` | Template chooser config — blank issues disabled |
| `.github/ISSUE_TEMPLATE/bug.yml` | Bug report issue form with DOR fields; `Blocked By` required |
| `.github/ISSUE_TEMPLATE/feature.yml` | Feature request issue form with DOR fields; `Blocked By` required; `Dependency Graph` textarea for epic/multi-issue ordering |
| `.github/ISSUE_TEMPLATE/idea.yml` | Idea inbox issue form — 5 required textareas (the itch + Q1-Q4 honesty checks) + Q5 vision-alignment dropdown; auto-applies labels `idea` and `needs-triage` |
| `.github/workflows/ci.yml` | GitHub Actions CI pipeline |
| `.github/workflows/release.yml` | Release automation: cross-platform builds, GitHub Release with SHA256 checksums, Homebrew tap update |
| `.claude/` | Claude Code agent configuration |
| `.claude/agents/` | Subagent definitions |
| `.claude/commands/` | Slash command definitions |
| `.claude/hooks/` | Pre/post command notification hooks |
| `.claude/skills/` | Reusable knowledge bases for subagents |
| `.claude/worktrees/` | Worktree checkouts managed by maestro |
| `build.rs` | Build script: generates `maestro.1` man page and bash/zsh/fish completions into `OUT_DIR` at build time (Issue #18) |
| `docs/` | Project documentation |
| `docs/FOLLOW-UPS.md` | Pending hardening and security follow-up items (non-blocking; file as issues before next release) |
| `docs/RUST-GUARDRAILS.md` | Rust coding policy — single source of truth; amend via PR |
| `docs/tech-debt-catalog.md` | Tech-debt catalog generated by `maestro adapt` |
| `src/` | Rust source code |
| `src/main.rs` | CLI entry point; `--skip-doctor` flag on `run` subcommand; `cmd_run()` calls `validate_preflight()` before launch and uses `PromptBuilder::build_issue_prompt()` for issue sessions; `setup_app_from_config()` propagates `once_mode` into `App`; forces `max_concurrent=1` when `--continuous` is set; `cmd_dashboard()` with startup cleanup, config-driven wiring, and `FetchSuggestionData` queued on startup; declares `mod updater`; declares `mod flags`; propagates startup gh auth check result into `App.gh_auth_ok`; declares `#[cfg(feature = "experimental-sanitizer")] mod sanitizer` (Issues #29, #34, #35, #36, #49, #52, #83, #85, #118, #141, #142, #158) |
| `src/cli.rs` | CLI struct and subcommand definitions; `--once` flag on `run` subcommand exits after all sessions complete (CI/scripting mode); `--continuous` / `-C` flag auto-advances through ready issues; `generate_completions()`, `cmd_completions()`, `cmd_mangen()`; `Completions` and `Mangen` subcommands (Issues #18, #83, #85) |
| `src/continuous.rs` | `ContinuousModeState` and `ContinuousFailure` structs; state machine tracking current issue, completed/skipped counts, and accumulated failures for `--continuous` mode (Issue #85) |
| `src/budget.rs` | Per-session and global budget enforcement (Phase 3) |
| `src/sanitizer.rs` | Compile-time gated placeholder module; only included when `--features experimental-sanitizer` is passed to cargo (Issue #142) |
| `src/flags/` | Feature flag registry and runtime resolution (Issues #141, #146) |
| `src/flags/mod.rs` | `Flag` enum with 6 variants; `FlagSource` enum (`Default`, `Config`, `Cli`); `serde` derive; `default_enabled()`, `description()`, `name()`, `all()` helpers |
| `src/flags/store.rs` | `FeatureFlags` store; per-flag source tracking; `HashMap`-based resolution order: CLI flag > config file > compile-time defaults; `source()` and `all_with_source()` methods |
| `src/mascot/` | Pixel-art and ASCII mascot rendering subsystem (Issues #473-476) |
| `src/mascot/mod.rs` | `MascotStyle` enum (`Sprite` \| `Ascii`) re-exported; `pub mod sprites` declared |
| `src/mascot/frames.rs` | `AsciiMascotFrames` (renamed from `MascotFrames`); `MASCOT_ROWS_ASCII` / `MASCOT_WIDTH_ASCII` constants replacing old `MASCOT_ROWS` / `MASCOT_WIDTH` aliases (Issue #476) |
| `src/mascot/widget.rs` | `MascotWidget`; `style: MascotStyle` field; `with_style()` builder; `render_sprite()` (128×128 pixel grid) and `render_ascii()` render paths (Issue #473) |
| `src/mascot/sprites.rs` | `sprite()` / `pixel()` accessors; embeds six 128×128 RGBA `.bin` files from `sprites/` at compile time via `include_bytes!` (Issue #474) |
| `src/mascot/sprites/` | Compiled pixel-art sprite data: `conducting.bin`, `error.bin`, `happy.bin`, `idle.bin`, `sleeping.bin`, `thinking.bin` — 128×128 px each (Issue #474) |
| `src/doctor.rs` | Preflight check system: `CheckSeverity`, `CheckResult`, `DoctorReport`, `run_all_checks()`, `print_report()`; `validate_preflight()` fails fast if any required check fails; `build_claude_cli_result()` (pub(crate), pure/testable); `check_claude_cli()` is Required severity; `build_gh_auth_result()` (pure/testable); `check_az_identity()` for Azure DevOps (Issues #49, #34, #52) |
| `src/git.rs` | GitOps trait and CLI-backed commit+push (Phase 3) |
| `src/models.rs` | Label-based model routing (Phase 3) |
| `src/prompts.rs` | Structured issue prompt builder with task-type detection; ProjectLanguage detection; guardrail resolution (Phase 3, Issue #43) |
| `src/adapt/` | Adapt pipeline: onboard existing projects to maestro workflow (Issues #87-95, #371) |
| `src/adapt/mod.rs` | Module exports; `cmd_adapt()` CLI entry point; adapt pipeline orchestration including scaffold phase; `pub mod scaffolder` (Issue #371) |
| `src/adapt/types.rs` | `AdaptPlan`, `AdaptReport`, `TechDebtItem`, `AdaptConfig`, `ScaffoldFileStatus`, `ScaffoldedFile`, `ScaffoldResult` type definitions (Issue #371) |
| `src/adapt/scanner.rs` | Project scanner Phase 1: detect language, framework, existing issues, CI config |
| `src/adapt/analyzer.rs` | Claude-backed analyzer Phase 2: structured adapt plan from scan results |
| `src/adapt/planner.rs` | Adaptation planner Phase 3: maps analyzer output to actionable plan steps |
| `src/adapt/materializer.rs` | Plan materializer Phase 4 — `GhMaterializer`: creates GitHub issues and milestones; `ensure_labels()` auto-creates missing labels before issue creation; `STANDARD_LABEL_COLORS` constant defines canonical hex colors for all maestro labels (Issues #93, #348) |
| `src/adapt/scaffolder.rs` | Scaffold phase — `ProjectScaffolder` trait, `ClaudeScaffolder` impl, `write_scaffold_files()`; generates project config files from the adapt plan (Issue #371) |
| `src/adapt/prompts.rs` | Claude prompt builders for the analyzer, planner, and scaffold phases; `build_scaffold_prompt()` added (Issue #371) |
| `src/adapt/knowledge.rs` | Knowledge-base compression (Phase 2.6 of `cmd_adapt`); `KnowledgeBase` struct (six `KnowledgeSection` fields); `write_knowledge_file()` writes `.maestro/knowledge.md`; auto-loaded by `SessionPool::try_promote` as a system-prompt component; 1 MiB size cap, symlink rejection, TOCTOU-safe load, envelope-wrapped injection (Issue #347) |
| `src/gates/` | Completion gates: TestsPass, FileExists, FileContains, PrCreated, Command (Phase 3, Issue #40) |
| `src/updater/` | Self-upgrade subsystem: version check, binary installation, and restart (Issue #118) |
| `src/updater/mod.rs` | `UpgradeState` state machine (`Idle` → `Checking` → `UpdateAvailable` → `Downloading` → `Installing` → `Done` / `Failed`); `ReleaseInfo` type |
| `src/updater/checker.rs` | `UpdateChecker` trait; `GitHubReleaseChecker` hits GitHub Releases API; semver version comparison; asset names use Rust target triples (e.g. `aarch64-apple-darwin`); checksum file resolves to `sha256sums.txt`; `check_for_update()` async entry point (Issues #118, #233) |
| `src/updater/installer.rs` | Binary replacement with pre-install backup; atomic swap via temp file; `.tar.gz` archives extracted via `flate2` + `tar` pipeline; `restart_with_same_args()` re-execs the upgraded binary with original argv (Issues #118, #233) |
| `src/updater/restart.rs` | `RestartBuilder` and `RestartCommand`: pure, testable post-upgrade re-exec command construction; no side effects until `.execute()` is called |
| `src/provider/` | Multi-provider abstraction layer (Issue #29) |
| `src/provider/mod.rs` | create_provider factory; detect_provider_from_remote |
| `src/provider/types.rs` | ProviderKind enum; re-exports Issue/Priority/MaestroLabel/SessionMode/Milestone |
| `src/provider/azure_devops.rs` | AzDevOpsClient (`az` CLI); parse_work_items_json; stub `list_milestones()`; stub `list_labels()` and `create_label()` to satisfy `GitHubClient` trait (Issue #348) |
| `src/github/` | GitHub API integration (Phase 2) |
| `src/github/types.rs` | GhIssue (milestone, assignees fields added), GhMilestone, Priority, MaestroLabel, SessionMode |
| `src/github/client.rs` | GitHubClient trait + `list_milestones()`; GhCliClient; MockGitHubClient; `parse_issues_json`; `parse_milestones_json`; `is_auth_error()`, `is_gh_auth_error()`; auth error detection in `run_gh()` (Issue #158); `list_labels()` and `create_label()` on trait and `GhCliClient` impl; `MockGitHubClient` gains `set_list_labels_error()`, `set_create_label_error()`, `list_labels_call_count()`, `create_label_calls()` helpers (Issue #348) |
| `src/github/ci.rs` | `CiChecker` (`check_pr_status`, `fetch_failure_log`); `CiStatus`; `CiPollAction`; `PendingPrCheck` (with `fix_attempt`, `awaiting_fix_ci`); `build_ci_fix_prompt`; `truncate_log`; `parse_ci_json`; `decide_ci_action` (Issue #41) |
| `src/github/labels.rs` | Issue label lifecycle transitions |
| `src/github/merge.rs` | `PrMergeCheck` trait (mockable); `PrMergeChecker` impl (`gh pr view` + `git diff`); `MergeState` enum; `PrConflictStatus` struct; `parse_merge_json()`; `parse_conflicting_files()`; `build_conflict_fix_prompt()` |
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
| `src/session/retry.rs` | Configurable retry policy; `hollow: HollowRetryConfig` field; `effective_max()` dispatches by policy + session intent (Phase 3, Issue #275) |
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
| `src/tui/mod.rs` | Event loop; `handle_screen_action()`; command processing; `launch_session_from_config()`; `FetchSuggestionData` async handler for GitHub ready/failed counts and milestone progress; spawns async version check on startup via `check_for_update()` — result delivered as `VersionCheckResult` data event; key handlers for upgrade confirmation banner (`[y]` confirm / `[n]` decline); `CompletionSummary` key-intercept branch with `[i]` issue browser, `[r]` new prompt, `[l]` activity log view, `[Enter]`/`[Esc]` dashboard; `ContinuousPause` key-intercept overlay: `[s]` skip, `[r]` retry, `[q]` quit continuous loop; exit path respects `once_mode`; `PromptInputScreen` always constructed with injected history for correct Up/Down recall; `pub mod theme`; `RunAdaptScaffold` command dispatch (Issues #31-33, #35, #38, #46-48, #83, #84, #85, #118, #232, #371) |
| `src/tui/app/` | App state module split into focused sub-files; `App` struct with `nav_stack: NavigationStack` field (replaces `confirm_exit_return_mode`); `navigate_to()`, `navigate_back()`, `navigate_back_or_dashboard()`, `navigate_to_root()` navigation methods; `theme: Theme`; `gh_auth_ok: bool`; `upgrade_state: UpgradeState`; `pending_prs: Vec<PendingPr>`; `config_path: Option<PathBuf>` propagated from `LoadedConfig` for settings save (Issues #12, #31-33, #35, #38, #40, #41, #43, #46-48, #52, #83, #84, #85, #118, #158, #342, #437) |
| `src/tui/app/types.rs` | `TuiMode` enum with `breadcrumb_label()` for human-readable mode names; `NavigationStack` struct — push/pop/peek/clear/breadcrumbs with a cap of 32 entries; `TuiCommand` (+ `RunAdaptScaffold`), `TuiDataEvent` (+ `AdaptScaffoldResult`), `SuggestionDataPayload`, `CompletionSummaryData`, `CompletionSessionLine`, `GateFailureInfo` (Issues #342, #371) |
| `src/tui/app/completion_summary.rs` | `build_completion_summary()`; `transition_to_dashboard()` now calls `navigate_to_root()` to fully clear the nav stack on dashboard return (Issue #342) |
| `src/tui/theme.rs` | `Theme` (resolved ratatui `Color` fields); `ThemeConfig` (`preset` + `overrides`); `ThemePreset` (`Dark`, `Light`); `ThemeOverrides` (per-field optional overrides); `SerializableColor` (named string / `#rrggbb` hex / 256-color index); `ColorCapability`; all 14 TUI rendering files consume theme fields instead of hardcoded `Color::` constants (Issue #38) |
| `src/tui/activity_log.rs` | Scrollable log widget |
| `src/tui/cost_dashboard.rs` | Per-session and aggregate cost display (Phase 3) |
| `src/tui/turboquant_dashboard.rs` | TurboQuant savings dashboard; `draw_turboquant_dashboard()`; `classify_savings()` → `(Vec<SessionSavings>, bool)`; `aggregate_savings()` → `AggregateSavings`; renders "Estimated Savings (projection)" (italic, rounded border) when no fork-handoff data, "Actual Savings" (bold, plain border) when real handoff metrics exist; per-session rows show `ACTUAL` or `proj.` kind markers (Issue #346) |
| `src/tui/dep_graph.rs` | ASCII dependency graph visualization (Phase 3) |
| `src/tui/detail.rs` | Session detail view (Phase 3) |
| `src/tui/fullscreen.rs` | Fullscreen session view with phase progress overlay (Phase 3) |
| `src/tui/help.rs` | Help overlay widget with keybinding reference (Phase 3) |
| `src/tui/markdown.rs` | markdown-to-ratatui rendering module; `wrap_and_push_text()` performs width-aware word wrapping when appending text spans to a line buffer |
| `src/tui/navigation/` | Keyboard navigation system and focus management (Issue #37) |
| `src/tui/navigation/mod.rs` | Module exports for navigation subsystem |
| `src/tui/navigation/focus.rs` | `FocusManager`: focus ring, widget focus state tracking |
| `src/tui/navigation/keymap.rs` | Keymap definitions: action-to-key bindings, context-sensitive keymaps |
| `src/tui/panels.rs` | Split-pane multi-session view; `panel_border_type()` returns thick borders for the focused grid panel; `▸` indicator on the selected panel title; `GatesRunning` (Cyan), `NeedsReview` (LightYellow), and `CiFix` (LightMagenta) status colors (Issues #40, #41) |
| `src/tui/ui.rs` | `draw_upgrade_banner()`: top-of-screen banner that renders all `UpgradeState` variants; `draw_gh_auth_warning()`: persistent top-of-screen banner shown when gh CLI is not authenticated, blocks gh-dependent actions until resolved; `draw_completion_overlay()`: centred overlay rendering PR links (underlined, full GitHub URL or `#N`), per-session error summaries in error color, and a keybindings bar with `[i]` Browse issues, `[r]` New prompt, `[l]` View logs, `[q]` Quit, `[Esc]` Dashboard; `ContinuousPause` render branch with pause overlay and status bar indicator; `HelpBarContext` struct drives context-aware keybinding dimming in the help bar; breadcrumb trail rendered in status bar from `nav_stack.breadcrumbs()` using `TuiMode::breadcrumb_label()`; `should_show_dashboard_mascot_panel()` / `dashboard_mascot_panel_width()` style-aware panel gate; passes `MascotStyle` through `draw_mascot_block()`, `HomeScreen::set_mascot()`, `LandingScreen::set_mascot()` (Issues #83, #84, #85, #118, #158, #342, #473) |
| `src/tui/screens/` | Interactive TUI screen components (Issues #31-33) |
| `src/tui/screens/mod.rs` | `ScreenAction` enum, `SessionConfig`; re-exports all screen types including `PromptInputScreen`; adds `pub mod wizard_fields`; removes `wizard_paste` (sanitizer moved into `TextAreaField::insert_sanitized`) (Issues #31-33, #86, #447) |
| `src/tui/screens/adapt_follow_up.rs` | `AdaptFollowUp`: post-scaffold follow-up prompt screen |
| `src/tui/screens/hollow_retry.rs` | `HollowRetryScreen`: minimal retry prompt overlay for stalled sessions awaiting user confirmation |
| `src/tui/screens/milestone.rs` | `MilestoneScreen`: milestone list with progress gauge and run-all action (Issue #33) |
| `src/tui/screens/prompt_input.rs` | `PromptInputScreen`: free-text prompt entry; `Enter` submits, `Shift+Enter`/`Alt+Enter` inserts newline via `insert_newline()` (not `input()`), `Ctrl+V` pastes from clipboard (image or text), `Esc` cancels; Up/Down arrows navigate prompt history; image attachment list with `[a]`/`[d]`; custom wrapped rendering via `wrap::soft_wrap_lines()` replaces tui-textarea widget rendering (Issues #101, #232, #263) |
| `src/tui/screens/queue_confirmation.rs` | `QueueConfirmationScreen`: confirmation overlay before bulk-queuing selected issues from the issue browser |
| `src/tui/screens/wizard_fields.rs` | Shared `tui-textarea` helpers: `TextAreaField` wraps `tui_textarea::TextArea` with single-line enforcement and `insert_sanitized()` paste path; `WizardFields` manages a fixed-size array of `TextAreaField`; strips Bidi overrides (U+202A–E, U+2066–9), Unicode line/paragraph separators (U+2028, U+2029), and BOM (U+FEFF) — CVE-2021-42574 "Trojan Source" hardening (Issue #447) |
| `src/tui/screens/wizard_fields_tests.rs` | Inline unit tests for `wizard_fields` — split into sibling file to stay under the 400-LOC cap (Issue #447) |
| `src/tui/screens/wrap.rs` | Soft-wrap utilities: `soft_wrap_lines()` splits a multi-line string into display lines that fit within a given column width using `unicode-width` for correct grapheme measurement (Issue #263) |
| `src/tui/screens/home/mod.rs` | `HomeScreen`: idle dashboard with 3-column layout (Quick Actions 30% / Suggestions 35% / Recent Activity 35%); `SuggestionKind` enum (`ReadyIssues`, `MilestoneProgress`, `IdleSessions`, `FailedIssues`); `Suggestion` struct with `build_suggestions()` factory; `HomeSection` enum for Tab-based focus toggle; `draw_suggestions()` renderer; `@username` display in project info bar; `set_mascot()` now takes a `MascotStyle` param (Issues #31, #34, #35, #49, #473) |
| `src/tui/screens/issue_browser/` | Issue browser screen: navigable issue list with multi-select, label/milestone filters, and preview pane |
| `src/tui/screens/issue_browser/mod.rs` | `IssueBrowserScreen`: multi-select list with label/milestone filters; `set_issues()` delivers async data; `reapply_filters()` honours active filters when new data arrives (Issues #32, #46, #117) |
| `src/tui/screens/issue_wizard/` | Issue creation wizard screen: multi-step TUI wizard for authoring GitHub issues (Issues #447, #450) |
| `src/tui/screens/issue_wizard/mod.rs` | `IssueWizardScreen`: `WizardFields`-backed wizard; `sync_fields_into_payload()`, `rebuild_fields_for_step()`, `field_text()`, `refresh_field_blocks()`; improve state fields (`improve_loading`, `improve_candidate`, `improve_error`, `improve_enqueued`, `diff_scroll`) and lifecycle methods (`begin_improve`, `apply_improve_result`, `accept_improve`, `discard_improve`); `AiReview` step key handler (`i`/`a`/`d`/`r`/`Esc`/`j`/`k`) (Issues #447, #450) |
| `src/tui/screens/issue_wizard/types.rs` | `IssueWizardStep` state machine and form payload types |
| `src/tui/screens/issue_wizard/ai_improve.rs` | Improve prompt builder (`build_improve_prompt`) and JSON parser (`parse_improve_response`); pure logic, no I/O; 13 unit tests (Issue #450) |
| `src/tui/screens/issue_wizard/ai_review.rs` | AI-assisted review step: calls LLM to review draft issue fields; refactored to use shared `format_payload_for_prompt` |
| `src/tui/screens/issue_wizard/draw.rs` | ratatui rendering; renders `TextArea` widget directly; blocks set via `refresh_field_blocks()` on mutable draw entry point (Issue #447) |
| `src/tui/screens/issue_wizard/draw_ai_review.rs` | Renders `AiReview` step and its improve sub-states: loading spinner, error view, before/after diff, and default review display (Issue #450) |
| `src/tui/screens/issue_wizard/draw_diff.rs` | 8-field red/green before-after diff renderer; 3 unit tests (Issue #450) |
| `src/tui/screens/issue_wizard/prompt_common.rs` | Shared `format_payload_for_prompt` helper used by both review and improve flows; 3 unit tests (Issue #450) |
| `src/tui/screens/landing/` | Landing screen components |
| `src/tui/screens/landing/mod.rs` | `LandingScreen` struct with `Screen` trait impl; `set_mascot()` takes `MascotStyle` param (Issue #473) |
| `src/tui/screens/landing/types.rs` | Landing screen type definitions |
| `src/tui/screens/landing/draw.rs` | ratatui rendering for landing screen; selects sprite (32×16) vs ascii (11×6) canvas based on `MascotStyle` (Issue #473) |
| `src/tui/screens/milestone_wizard/` | Milestone creation wizard screen: multi-step TUI wizard for authoring GitHub milestones (Issue #447) |
| `src/tui/screens/milestone_wizard/mod.rs` | `MilestoneWizardScreen`: three persistent `TextAreaField`s (`goal_field`, `non_goals_field`, `doc_buffer_field`); analogous migration to `IssueWizardScreen` (Issue #447) |
| `src/tui/screens/milestone_wizard/types.rs` | `MilestoneWizardStep` state machine and form payload types |
| `src/tui/screens/milestone_wizard/ai_planning.rs` | AI-assisted planning step: calls LLM to generate milestone dependency graph |
| `src/tui/screens/milestone_wizard/draw.rs` | ratatui rendering; doc-refs step splits committed list / in-progress buffer / help hint (Issue #447) |
| `src/tui/screens/project_stats/` | Project statistics screen components |
| `src/tui/screens/project_stats/mod.rs` | `ProjectStatsScreen` struct with `Screen` trait impl |
| `src/tui/screens/project_stats/types.rs` | Project stats type definitions |
| `src/tui/screens/project_stats/draw.rs` | ratatui rendering for project statistics display |
| `src/tui/screens/settings/mod.rs` | `SettingsScreen`: tabbed interactive settings UI; `Flags` tab shows all feature flags with name, state, source (`Default`/`Config`/`Cli`), and description; footer built from focused widget's `edit_hint()` so edit keys (`Space`/`Enter`/`←→`) are always advertised; `KeymapProvider::keybindings()` gains a third `"Edit"` group for the `?` help overlay; `save_config` returns `Err` when `config_path` is `None`; Ctrl+S failures surfaced as a 5-second title-bar flash via `save_error_flash: Option<(String, Instant)>` (`accent_error`, message sanitized + truncated to 80 chars) (Issues #124, #146, #432, #437) |
| `src/icon_mode.rs` | Shared icon mode detection: `AtomicBool` global, `init_from_config()`, `use_nerd_font()`; reads `tui.ascii_icons` config and `MAESTRO_ASCII_ICONS` env var (Issue #307) |
| `src/icons.rs` | Shared icon registry: `IconId` enum (38 variants + `NeedsReview`), `IconPair` struct, `icon_pair()` const jump table, `get(IconId)`, `get_for_mode(id, nerd_font)`; `CheckboxOn` = U+F14A (nf-fa-check_square), `CheckboxOff` = U+F0C8 (nf-fa-square) — FA-core glyphs replacing legacy nf-oct codepoints (Issues #308, #433) |
| `src/tui/icons.rs` | Thin re-export shim: re-exports all public items from `src/icon_mode.rs` and `src/icons.rs` so existing `tui::icons::` import paths remain valid (Issues #307, #308) |
| `src/tui/screens/adapt/` | Adapt wizard screen: multi-step TUI wizard for onboarding a project into maestro (Issues #88, #371) |
| `src/tui/screens/adapt/mod.rs` | `AdaptScreen` struct implementing the `Screen` trait; wizard entry point and step coordination; `complete_scaffold()`, `set_scaffold_result()` methods (Issue #371) |
| `src/tui/screens/adapt/types.rs` | `AdaptStep` (+ `Scaffolding` variant), `AdaptWizardConfig`, `AdaptResults` (+ `scaffold` field), `AdaptError` type definitions (Issue #371) |
| `src/tui/screens/adapt/draw.rs` | ratatui rendering functions for adapt wizard steps and layout; scaffold phase rendering (Issue #371) |
| `src/tui/screens/pr_review/` | PR review screen: multi-step TUI screen for reviewing and submitting pull request feedback |
| `src/tui/screens/pr_review/mod.rs` | `PrReviewScreen` struct implementing the `Screen` trait |
| `src/tui/screens/pr_review/types.rs` | `PrReviewStep` state machine, `ReviewForm` and related type definitions |
| `src/tui/screens/pr_review/draw.rs` | ratatui rendering logic with markdown integration |
| `src/tui/screen_dispatch.rs` | `ScreenDispatch`: routes key events and render calls to the active screen; constructor accepts `FeatureFlags` to supply the settings screen; always injects prompt history when constructing `PromptInputScreen`; `ScreenAction::Push` delegates to `navigate_to()`, `ScreenAction::Pop` delegates to `navigate_back()`; `Scaffolding` case wired in `StartAdaptPipeline` dispatch; reads `app.config_path` directly for settings save (removed relative-path probe); `tracing::warn!` when `config_path` is absent (Issues #146, #232, #342, #371, #437) |
| `src/tui/spinner.rs` | Braille spinner helpers: `spinner_frame()`, `format_thinking_elapsed()`, full spinner activity string builder |
| `src/settings/` | Settings persistence layer: reads/writes `.claude/settings.json`; exposes `SettingsStore` trait, `FsSettingsStore` impl, `CavemanModeState` enum (Issue #490) |
| `src/settings/claude_settings.rs` | `CavemanModeState` (ExplicitTrue/ExplicitFalse/Default/Error); `SettingsStore` trait; `FsSettingsStore` atomic writer; `toggle_caveman_mode()` (Issue #490) |
| `src/tui/screens/settings/caveman_row.rs` | Render helper for the caveman-mode row in SettingsScreen; four visual states driven by `CavemanModeState` (Issue #490) |
| `src/tui/snapshot_tests/caveman_row.rs` | 5 insta snapshot tests for caveman row rendering (Issue #490) |
| `src/tui/snapshot_tests/` | TUI snapshot test suite; 41 tests across 9 views using `insta`; run with `cargo test tui::snapshot_tests`; update with `INSTA_UPDATE=always cargo test` or `cargo insta review` (Issues #16, #490) |
| `src/tui/snapshot_tests/overview.rs` | 6 snapshot tests for `PanelView`: empty, single running, multiple, selected, context overflow, forked |
| `src/tui/snapshot_tests/detail.rs` | 6 snapshot tests for `DetailView`: basic, progress, activity log, no files touched, files + retries, markdown content |
| `src/tui/snapshot_tests/fullscreen.rs` | 4 snapshot tests for `FullscreenView`: markdown last message, plain text, empty placeholder, auto-scroll to bottom |
| `src/tui/snapshot_tests/dashboard.rs` | 4 snapshot tests for `HomeScreen`: baseline, with warnings, with suggestions, selected action |
| `src/tui/snapshot_tests/issue_browser.rs` | 7 snapshot tests for `IssueBrowserScreen`: with issues, empty, loading, multi-select, filter active, prompt overlay empty, prompt overlay with text |
| `src/tui/snapshot_tests/milestone.rs` | 4 snapshot tests for `MilestoneScreen`: with milestones, empty, loading, issues in detail pane |
| `src/tui/snapshot_tests/cost_dashboard.rs` | 5 snapshot tests for `CostDashboard`: no budget, under threshold, over 90%, empty sessions, sorted by cost |
| `src/tui/snapshot_tests/turboquant_dashboard.rs` | 3 snapshot tests for `TurboQuantDashboard`: projections-only, mixed actual+projections, empty sessions (Issue #346) |
| `src/tui/snapshot_tests/snapshots/` | Committed `.snap` files — insta ground-truth for TUI rendering regressions |
| `src/integration_tests/` | End-to-end integration test suite; MockGitHubClient and MockWorktreeManager; no external process dependencies (Issue #15) |
| `src/integration_tests/mod.rs` | Module declarations and shared helpers: `make_pool()`, `make_pool_with_worktree()`, `make_session()`, `make_session_with_issue()`, `make_gh_issue()` |
| `src/integration_tests/session_lifecycle.rs` | 11 tests covering enqueue, promote, and complete session lifecycle via `handle_event()` |
| `src/integration_tests/stream_parsing.rs` | 22 tests covering stream event parsing and parser round-trips |
| `src/integration_tests/completion_pipeline.rs` | 9 tests covering label transitions and PR creation |
| `src/integration_tests/concurrent_sessions.rs` | 6 tests covering `max_concurrent` enforcement |
| `src/integration_tests/worktree_lifecycle.rs` | 8 tests covering worktree create/cleanup and health monitoring |
| `src/integration_tests/upgrade.rs` | End-to-end upgrade flow tests: version check, banner state transitions, installer backup/swap, `RestartCommand` construction (Issue #118) |
| `src/turboquant/` | Vector quantization for context compression (Issues #242-253, #343-345, #347) |
| `src/turboquant/types.rs` | `QuantStrategy` enum; `TurboQuantConfig` with three v0.14.0 budget fields (`fork_handoff_budget`, `system_prompt_budget`, `knowledge_budget`); `QuantResult`; `CompressionMetrics` |
| `src/turboquant/polar.rs` | PolarQuant — recursive polar decomposition; preserves cosine distance |
| `src/turboquant/qjl.rs` | QJL — seeded 1-bit Johnson-Lindenstrauss residual projection |
| `src/turboquant/pipeline.rs` | Two-stage quantization pipeline (PolarQuant + QJL); strategy-aware wrappers |
| `src/turboquant/adapter.rs` | `TurboQuantAdapter`: text-to-embedding bridge; `TextRanker` trait + impl; `compress_handoff()` → `CompressedHandoff` (Issue #343); `compact_system_prompt()` (Issue #344); `compact_session_history()` → `StateCompactionReport` (Issue #345); shared `Arc<TurboQuantAdapter>` on `App`; `project_savings()`, `session_savings()`, `implied_rate_per_token()` and public types `SavingsProjection`, `SavingsKind`, `SessionSavings` (Issue #346) |
| `src/turboquant/budget.rs` | `TokenBudget` helper: greedy ranked-segment selection under a token limit; `BudgetSelection` struct; used by fork-handoff, system-prompt, and knowledge compression (Issues #343-345, #347) |
| `src/work/` | Work queue and dependency scheduling (Phase 2) |
| `src/work/assigner.rs` | Priority-ordered work assignment; `mark_pending()` and `mark_pending_undo_cascade()` for continuous mode retry/skip (Issue #85) |
| `src/work/conflicts.rs` | Conflict detection for concurrent work items |
| `src/work/dependencies.rs` | Dependency graph, topological sort |
| `src/work/executor.rs` | `QueueExecutor` state machine: `ExecutorPhase` (Idle, Running, AwaitingDecision, Finished); `ExecutorItem`; `QueueItemState`; `FailureAction` (Retry, Skip, Abort); `advance()`, `mark_success()`, `mark_failure()`, `apply_decision()`, `set_session_id()` |
| `tests/settings_caveman.rs` | Integration tests for `FsSettingsStore`: real-tempfile read/write/toggle round-trips, missing-key defaults, malformed JSON handling (Issue #490) |
| `template/` | Reproducible project template |
| `CHANGELOG.md` | Release history |
| `ROADMAP.md` | Project milestones and implementation order |
| `directory-tree.md` | This file |
| `maestro.toml` | Runtime configuration; `[turboquant]` section gains `fork_handoff_budget`, `system_prompt_budget`, and `knowledge_budget` fields (token limits for v0.14.0 compression features) |
| `maestro-state.json` | Persisted session state |
