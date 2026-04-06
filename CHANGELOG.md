# Changelog

All notable changes to Maestro are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Release Workflow for Binary Build and Distribution (#17)

- Release workflow now prevents concurrent builds on the same tag
- Homebrew tap update fails fast when API credentials are missing or the API returns an error
- Release binaries are fully optimized and stripped for minimal distribution size (LTO, single codegen unit, symbol stripping)

### TUI Rendering Snapshot Tests (#16)

- `Cargo.toml` — `insta = "1"` added as a dev-dependency for snapshot-based TUI rendering tests
- `src/tui/snapshot_tests/mod.rs` — new `#[cfg(test)]` module declared inside the binary crate (no `lib.rs` required); declares the six view submodules
- `src/tui/snapshot_tests/overview.rs` — 6 snapshot tests for `PanelView` (empty sessions, single running, multiple sessions, selected session, context overflow, forked session)
- `src/tui/snapshot_tests/detail.rs` — 5 snapshot tests for `DetailView` (basic, with progress, with activity log, no files touched, files with retries)
- `src/tui/snapshot_tests/dashboard.rs` — 4 snapshot tests for `HomeScreen` (baseline, with warnings, with suggestions, selected action)
- `src/tui/snapshot_tests/issue_browser.rs` — 5 snapshot tests for `IssueBrowserScreen` (with issues, empty list, loading state, multi-select, filter active)
- `src/tui/snapshot_tests/milestone.rs` — 4 snapshot tests for `MilestoneScreen` (with milestones, empty, loading, issues in detail pane)
- `src/tui/snapshot_tests/cost_dashboard.rs` — 5 snapshot tests for `CostDashboard` (no budget, under threshold, over 90% budget, empty sessions, sorted by cost)
- `src/tui/snapshot_tests/snapshots/` — 29 committed `.snap` files forming the ground-truth for TUI rendering regression detection; run with `cargo test tui::snapshot_tests`; update with `INSTA_UPDATE=always cargo test` or `cargo insta review`

### CI Error Detection and Auto-Fix Loop (#41)

- `src/config.rs` — `CiAutoFixConfig` struct added under `GatesConfig.ci_auto_fix`: `enabled: bool` (default `true`), `max_retries: u32` (default `3`), `poll_interval_secs: u64` (default `90`); fully TOML-deserializable with sane defaults when the `[gates.ci_auto_fix]` section is absent
- `src/github/ci.rs` — `CiPollAction` enum added with three variants: `Wait` (CI still running or fix session in progress), `SpawnFix { log: String }` (spawn a fix session with this failure log), `Abandon` (retries exhausted or auto-fix disabled); `PendingPrCheck` extended with `fix_attempt: u32` and `awaiting_fix_ci: bool` fields; `fetch_failure_log(pr_number, branch)` method added to `CiChecker`: calls `gh run list` then `gh run view --log-failed` and returns a truncated log (max 4 000 chars); `build_ci_fix_prompt(pr_number, issue_number, branch, attempt, failure_log)` helper builds the unattended fix prompt injected into the fix session; `truncate_log(log, max_chars)` helper trims long logs to the last `max_chars` bytes while preserving line boundaries; `parse_ci_json(json)` extracted to a `pub(crate)` free function for unit-test coverage; `decide_ci_action(check, max_retries, error_log)` free function encodes the state-machine decision: `Wait` if `awaiting_fix_ci`, `Abandon` if `fix_attempt >= max_retries`, otherwise `SpawnFix`
- `src/session/types.rs` — `SessionStatus::CiFix` variant added: symbol `"🔧"`, label `"CI_FIX"`, non-terminal; `CiFixContext` struct added (`pr_number`, `issue_number`, `branch`, `attempt`) with `Serialize`/`Deserialize`; `ci_fix_context: Option<CiFixContext>` field added to `Session`
- `src/tui/app.rs` — `poll_ci_status()` extended with auto-fix loop: on `CiStatus::Failed`, calls `fetch_failure_log()` and `decide_ci_action()` to choose between `Wait`, `SpawnFix`, or `Abandon`; sets `awaiting_fix_ci = true` when a fix session is spawned, and clears it when the fix session exits; `spawn_ci_fix_session(pr_number, issue_number, branch, attempt, failure_log)` added: builds a `Session` with status `CiFix` and a populated `ci_fix_context`, then adds it to the pool; `on_issue_session_completed()` updated to skip PR creation when `is_ci_fix` is true, treating a completed fix session as a signal to re-enter the CI polling cycle
- `src/tui/panels.rs` — `CiFix` mapped to `Color::LightMagenta` in `status_color()`

### Auto-fmt, Clippy, and Test Completion Gates (#40)

- `src/config.rs` — `CompletionGatesConfig` struct added to `SessionsConfig` with `enabled: bool` (default `true`) and `commands: Vec<CompletionGateEntry>`; `CompletionGateEntry` struct with `name`, `run`, and `required` (default `true`) fields; both are TOML-deserializable and serializable; `completion_gates` field replaces ad-hoc gate setup
- `src/gates/types.rs` — `Command` variant added to `CompletionGate` enum with `name: String`, `command: String`, and `required: bool` fields; `is_required()` method returns `true` for all legacy variants and the `required` field for `Command`; `display_name()` method returns the gate's log-friendly name; `from_config_entry(entry: &CompletionGateEntry) -> Self` constructor maps config entries to the new variant
- `src/gates/runner.rs` — `Command` match arm added to `run_single_gate()`: splits the command string, executes it in the worktree directory, and produces a named `GateResult`; empty command guard returns a failing result; `all_required_gates_passed(results: &[(GateResult, bool)]) -> bool` added to evaluate gate results paired with their required flag — optional gate failures are advisory only
- `src/session/types.rs` — `GatesRunning` variant added to `SessionStatus`: used while config-driven gates are executing after a session completes; `NeedsReview` variant added to `SessionStatus`: terminal state assigned when one or more required gates fail; both variants have `symbol()`, `label()`, and `is_terminal()` implementations (`NeedsReview` is terminal, `GatesRunning` is not)
- `src/session/pool.rs` — `find_by_issue_mut(issue_number: u64) -> Option<&mut ManagedSession>` added: searches active sessions first, then finished sessions, by issue number; used by `check_completions()` to update session status during gate execution
- `src/tui/app.rs` — `check_completions()` updated: when a session succeeds, it now loads `[sessions.completion_gates]` commands (falling back to the legacy `[gates].test_command` if the new section is absent or empty); transitions session to `GatesRunning`, runs each gate via `GateRunner`, logs per-gate `[gate_name]: message` entries to the activity log with `Info`/`Error` level, then transitions to `NeedsReview` and fires the `TestsFailed` plugin hook if any required gate fails; fires `TestsPassed` and logs "All required gates passed" on success
- `src/tui/panels.rs` — `GatesRunning` mapped to `Color::Cyan`; `NeedsReview` mapped to `Color::LightYellow` in the `status_color()` function
- `maestro.toml` — `[sessions.completion_gates]` section added with `enabled = true` and three default `[[sessions.completion_gates.commands]]` entries: `fmt` (`cargo fmt --check`, required), `clippy` (`cargo clippy -- -D warnings`, required), `test` (`cargo test`, required)

### Work Suggestions and Quick Commands (#35)

- `src/tui/screens/home.rs` — `SuggestionKind` enum added with four variants: `ReadyIssues { count }`, `MilestoneProgress { title, closed, total }`, `IdleSessions`, and `FailedIssues { count }`
- `src/tui/screens/home.rs` — `Suggestion` struct added with `kind`, `message`, `shortcut`, and `action` fields; `build_suggestions()` factory method derives contextual hints from GitHub data (ready/failed issue counts, milestone progress) and current session state
- `src/tui/screens/home.rs` — `HomeSection` enum added (`QuickActions`, `Suggestions`); `HomeScreen` gains `suggestions`, `selected_suggestion`, and `focus_section` fields; `Tab` key toggles focus between panels; `j`/`k`/arrows navigate within the focused panel; `Enter` executes the highlighted item in either panel; `set_suggestions()` method for async data delivery
- `src/tui/screens/home.rs` — `draw()` bottom section refactored from a 2-column to a 3-column layout: Quick Actions (30%) | Suggestions (35%) | Recent Activity (35%); `draw_suggestions()` renders the new panel with focus-aware green/gray border and an empty-state fallback message
- `src/tui/app.rs` — `SuggestionDataPayload` struct added (`ready_issue_count`, `failed_issue_count`, `milestones`); `TuiCommand::FetchSuggestionData` variant added; `TuiDataEvent::SuggestionData(SuggestionDataPayload)` variant added; `handle_data_event()` routes `SuggestionData` into `Suggestion::build_suggestions()` and delivers the result to `HomeScreen::set_suggestions()`
- `src/tui/mod.rs` — `FetchSuggestionData` branch added to the command processing loop: spawns a background `tokio` task that fetches `maestro:ready` and `maestro:failed` issue counts and open milestone progress via `GhCliClient`, then delivers a `SuggestionData` event
- `src/main.rs` — `cmd_dashboard()` queues `TuiCommand::FetchSuggestionData` immediately after `App` construction so suggestions are populated on first render

### Session Launch with Worktree Isolation from TUI (#36)

- `src/main.rs` — `setup_app_from_config()` helper introduced: consolidates `App` construction shared between `cmd_run` and `cmd_dashboard`; wires `BudgetEnforcer`, `ModelRouter`, `NotificationDispatcher`, and `PluginRunner` from config; reads `permission_mode` and `allowed_tools` from `[sessions]` config rather than hardcoding them
- `src/main.rs` — `cmd_dashboard()` now performs orphan worktree cleanup and old log cleanup (same as `cmd_run`) on startup; delegates `App` construction to `setup_app_from_config()` when a config is present; wires `github_client` unconditionally
- `src/main.rs` — `cmd_run()` refactored to call `setup_app_from_config()` instead of duplicating wiring logic

### Provider Auth Verification and User Context (#34)

- `src/doctor.rs` — `build_gh_auth_result(auth_ok, username, scopes)` extracted as a pure, testable function; `check_gh_authenticated()` refactored to call `gh api user -q .login` for the authenticated username and to parse token scopes from `gh auth status` stderr; success message now reads `authenticated as @<username>, scopes: <scopes>`
- `src/doctor.rs` — `check_az_identity()` added: runs `az account show -o tsv --query user.name` and surfaces the signed-in Azure identity as an Optional check; only executed when the Azure DevOps provider is configured and `az cli` is already passing
- `src/tui/screens/home.rs` — `ProjectInfo` struct gains `username: Option<String>` field; `draw_project_info()` renders `@<username>` (or `@unknown` as fallback) in the project info bar alongside repo and branch
- `src/main.rs` — `cmd_dashboard()` extracts the authenticated username from the `gh auth` check result produced by `run_all_checks()` and passes it into `ProjectInfo`; no additional subprocess is spawned — username is reused from the doctor report

### Standardized Issue Templates with Definition of Ready (#53)

- `.github/ISSUE_TEMPLATE/config.yml` — template chooser added; blank issues disabled to enforce structured reporting
- `.github/ISSUE_TEMPLATE/feature.yml` — feature request form with Definition of Ready (DOR) fields: acceptance criteria, scope, affected components, and a DOR checklist (problem/value statement, testable acceptance criteria, no undecided blockers, estimated scope)
- `.github/ISSUE_TEMPLATE/bug.yml` — bug report form with DOR fields: steps to reproduce, expected vs actual behaviour, environment details, and a DOR checklist (reproducible steps, expected behaviour documented, scope estimated)
- `.claude/CLAUDE.md` — DOR section (section 3) added before the TDD section, establishing the Definition of Ready as a mandatory gate before any implementation work begins

### Onboarding Preflight Check — `maestro doctor` (#49)

- New `src/doctor.rs` module with a self-contained preflight check system
- `CheckSeverity` enum (`Required`, `Optional`) — distinguishes blocking failures from soft warnings
- `CheckResult` struct with `pass()` and `fail()` constructors; `symbol()` returns `"OK"`, `"FAIL"`, or `"WARN"` based on severity and outcome
- `DoctorReport` struct aggregating all check results; exposes `has_failures()`, `has_warnings()`, and `failed_checks()` helpers
- `run_all_checks(config)` executes 9 individual checks in order:
  - `check_gh_installed` — verifies `gh` CLI is on `$PATH` (Required)
  - `check_gh_authenticated` — runs `gh auth status` (Required)
  - `check_git_installed` — verifies `git` is on `$PATH` (Required)
  - `check_git_user_config` — confirms `user.name` and `user.email` are set (Required)
  - `check_git_remote` — ensures at least one remote is configured (Required)
  - `check_config_exists` — looks for `maestro.toml` in the working directory (Required)
  - `check_az_cli` — runs only when the configured provider is `AzureDevops` (Optional)
  - `check_claude_cli` — verifies `claude` CLI is available; failure is a warning, not a hard block (Optional)
  - `check_gh_repo_accessible` — runs `gh repo view` only when `gh auth` passed (Required)
- `print_report(report)` renders a colour-coded table to stdout (green OK, red FAIL, yellow WARN) with a one-line summary at the end
- `Commands::Doctor` variant added to the clap CLI in `src/main.rs`; `cmd_doctor()` handler loads config optionally (no error if `maestro.toml` is absent) and exits with a non-zero code when required checks fail
- TUI dashboard integration: `cmd_dashboard()` in `src/main.rs` now runs `run_all_checks()` at startup and passes the list of failed/warned check messages into `HomeScreen`
- `HomeScreen` in `src/tui/screens/home.rs` gains a `warnings: Vec<String>` field, a `draw_warnings()` method that renders a yellow bordered panel beneath the logo, and dynamic layout that hides the panel entirely when there are no warnings

### Live GitHub Data Fetching and Session Launch from TUI (#46, #47, #48)

- **Issue browser live fetch (#46):** opening the issue browser now triggers an async GitHub fetch via `tokio::spawn` + `mpsc` channel; the screen shows a loading state while data arrives and calls `set_issues()` on the `IssueBrowserScreen` once the fetch completes
- **Milestone screen live fetch (#47):** opening the milestone overview triggers an async fetch that calls the new `list_milestones()` method on `GhCliClient`, then fetches per-milestone issue lists in the same background task and delivers `MilestonesFetched` data events to the app
- **Session launch wired from screens (#48):** `LaunchSession` and `LaunchSessions` screen actions now produce real Claude sessions; `launch_session_from_config()` in `src/tui/mod.rs` fetches the full issue via `get_issue()`, resolves the mode from issue labels, constructs a `Session`, and calls `app.add_session()`; both single-launch (`Enter`) and multi-select batch-launch (`Space` + `Enter`) are fully wired
- `TuiCommand` enum added to `src/tui/app.rs`: `FetchIssues`, `FetchMilestones`, `LaunchSession(SessionConfig)`, `LaunchSessions(Vec<SessionConfig>)` — queued by synchronous input handlers and processed each event loop tick
- `TuiDataEvent` enum added to `src/tui/app.rs`: `IssuesFetched(Result<Vec<GhIssue>>)`, `MilestonesFetched(Result<Vec<(GhMilestone, Vec<GhIssue>)>>)` — delivered from `tokio::spawn` tasks via `mpsc::UnboundedSender`
- `App::handle_data_event()` added: routes `IssuesFetched` to `IssueBrowserScreen::set_issues()` and `MilestonesFetched` into `MilestoneScreen::milestones`; propagates errors to the activity log
- `data_tx` / `data_rx` channel fields added to `App` struct; `App::new()` initialises the `mpsc::unbounded_channel()` pair
- `handle_screen_action()` in `src/tui/mod.rs` rewrote the `Push(IssueBrowser)` branch: if pushing from milestone view the pre-loaded issue list is used, otherwise a loading screen is shown and `FetchIssues` is queued; `Push(MilestoneView)` queues `FetchMilestones` on first open
- `IssueBrowserScreen::set_issues()` added: atomically replaces the issue list, resets `filtered_indices`, `selected`, `scroll_offset`, and clears the loading flag
- `GitHubClient` trait extended with `list_milestones(&self, state: &str) -> Result<Vec<GhMilestone>>`; `GhCliClient` implements it by calling `gh api repos/{owner}/{repo}/milestones`; `MockGitHubClient` implements it from an in-memory `milestones: Vec<GhMilestone>` set via `set_milestones()`
- `parse_milestones_json()` added to `src/github/client.rs`: deserialises the GitHub Milestones API array response via `serde_json`
- `AzDevOpsClient` in `src/provider/azure_devops.rs` gains a stub `list_milestones()` that returns an empty vec (Azure DevOps milestone support is tracked separately)
- Blanket `impl<T: GitHubClient> GitHubClient for &T` updated to delegate `list_milestones`

### Interactive TUI Screens (#31, #32, #33)

- `src/tui/screens/` module with three new interactive screens and a shared navigation contract
- `ScreenAction` enum (`None`, `Push(TuiMode)`, `Pop`, `LaunchSession`, `LaunchSessions`, `Quit`) drives navigation without tight coupling between screens
- `SessionConfig` struct carries issue number and title through the `LaunchSession`/`LaunchSessions` actions
- **HomeScreen** (`screens/home.rs`, Issue #31): idle dashboard rendered at startup; displays ASCII logo, repo/branch info, a keyboard-navigable quick-actions menu (`[i]` Browse Issues, `[m]` Browse Milestones, `[c]` Cost Report, `[q]` Quit), and a "Recent Activity" panel showing the last N session outcomes
- **IssueBrowserScreen** (`screens/issue_browser.rs`, Issue #32): full-screen issue browser; `j`/`k` or arrow keys navigate the list; `Space` toggles multi-select (highlighted in green); `Enter` launches a single session or, when items are multi-selected, emits `LaunchSessions`; `/` activates label-text filter mode; `m` activates milestone filter mode; `Esc` exits filter mode or pops back; live-filter reapplication clamps cursor to avoid index out-of-bounds
- **MilestoneScreen** (`screens/milestone.rs`, Issue #33): milestone overview with per-milestone ratatui `Gauge` progress bars showing `closed/total issues (N%)`; `j`/`k` navigation; `Enter` pushes `IssueBrowser` pre-filtered to the selected milestone; `r` emits `LaunchSessions` for all open issues in the selected milestone; empty-list guard prevents panics
- `TuiMode` enum extended with `Dashboard`, `IssueBrowser`, `MilestoneView` variants in `src/tui/app.rs`
- `src/tui/mod.rs`: screen event delegation wired into the main event loop; `handle_screen_action` dispatcher translates `ScreenAction` results into pool operations and mode transitions
- `src/tui/ui.rs`: rendering branches added for the three new `TuiMode` variants
- `src/github/types.rs`: `GhIssue` gains `milestone: Option<u64>` and `assignees: Vec<String>` fields (both `serde(default)`); new `GhMilestone` struct with `number`, `title`, `description`, `state`, `open_issues`, `closed_issues`
- `src/github/client.rs`: `parse_issues_json` updated to populate `milestone` and `assignees` from the GitHub API response
- `src/provider/types.rs`: `GhMilestone` re-exported as `Milestone` for provider-agnostic usage
- `src/provider/azure_devops.rs`: `GhIssue` construction updated to initialise the new `milestone` and `assignees` fields
- `src/main.rs`: `cmd_dashboard` updated to initialise and push `HomeScreen` as the entry point

### Session Prompt Guardrails (#43)

- `ProjectLanguage` enum (`Rust`, `TypeScript`, `Python`, `Go`, `Unknown`) in `src/prompts.rs`
- `detect_project_language(dir)`: inspects manifest files (`Cargo.toml`, `package.json`, `pyproject.toml`, `requirements.txt`, `go.mod`) to identify the project language
- `default_guardrail(lang)`: returns a language-specific, pre-completion checklist (format, lint, test, commit) for each supported language; falls back to a generic checklist for unknown projects
- `resolve_guardrail(custom, dir)`: uses the custom prompt from config when non-empty, otherwise auto-detects via `detect_project_language`
- `SessionsConfig.guardrail_prompt: Option<String>` added to `config.rs`; when `None` or empty the guardrail is auto-detected
- `SessionPool.guardrail_prompt` field and `set_guardrail_prompt()` setter in `session/pool.rs`; `try_promote()` appends the guardrail to every session's system prompt
- `App::configure()` in `tui/app.rs` now calls `resolve_guardrail` and forwards the result to `pool.set_guardrail_prompt()`
- `maestro.toml`: `guardrail_prompt` option added as a commented-out example under `[sessions]` with inline documentation

## [0.3.0] - 2026-04-01

### Multi-Provider Support (#29)

- Provider abstraction layer with `ProviderKind` enum (GitHub, AzureDevOps)
- `AzDevOpsClient` implementing full issue/PR/label lifecycle via `az` CLI
- `ProviderConfig` in `maestro.toml` with `kind`, `organization`, `az_project` fields
- `create_provider()` factory and `detect_provider_from_remote()` auto-detection
- Provider-agnostic type aliases (`Issue`, `Priority`, `MaestroLabel`, `SessionMode`)

### Homebrew Release Automation (#27)

- Cross-platform release workflow (macOS arm64/x86_64, Linux x86_64) triggered on tag push
- Auto-updates Homebrew tap formula via `repository_dispatch`

## [0.2.0] - 2026-03-31

### Context Overflow Detection and Auto-Fork (#12)

- `ContextMonitor` trait with `ProductionContextMonitor`: tracks per-session context usage percentage, firing overflow events when a configurable threshold is crossed and emitting a one-time commit-prompt signal at a lower threshold
- `SessionForker` trait with `ForkPolicy`: auto-forks a running session into a continuation child session when context overflows, enforcing a configurable maximum fork depth to prevent runaway chains
- `build_continuation_prompt`: constructs a structured handoff prompt for the child session, embedding parent session ID, current phase, files modified, tools used, and an explicit "do not redo completed work" instruction
- `ContextOverflowConfig` added to `SessionsConfig` in `config.rs`: `overflow_threshold_pct` (default 70), `auto_fork` (default true), `commit_prompt_pct` (default 50), `max_fork_depth` (default 5)
- Fork lineage tracking in `MaestroState`: `fork_lineage` HashMap, `record_fork`, `fork_chain`, and `fork_depth` helpers; cycle guard prevents infinite ancestry walks
- `Session` fields added: `parent_session_id`, `child_session_ids`, `fork_depth`
- `StreamEvent::ContextUpdate` variant: emitted by the parser when Claude CLI reports context usage in system events
- `manager.rs` handles `ContextUpdate` events; `logger.rs` logs them
- `tui/app.rs` gains `context_monitor` and `fork_policy` fields, plus `check_context_overflow` method that triggers auto-fork via the session pool
- `tui/panels.rs`: fork depth indicator shown in panel title; context gauge changes colour and displays warning text when approaching overflow threshold
- `HookPoint::ContextOverflow` variant added so plugins can react to overflow events
- `maestro.toml`: `[sessions.context_overflow]` section documents all four configuration knobs

## [0.1.0] - 2026-03-24

First feature-complete release encompassing Phases 0 through 4 of the PRD.

### Phase 4: Plugin System, Modes, Polish

- Plugin hook system with 7 lifecycle events (`session_started`, `session_completed`, `tests_passed`, `tests_failed`, `budget_threshold`, `file_conflict`, `pr_created`)
- Config-driven mode system with built-in orchestrator/vibe/review modes
- Custom modes via `[modes.<name>]` in `maestro.toml`
- Full-screen agent detail view (Enter / 1-9 keys)
- Help overlay (? key)
- Cost dashboard with budget gauge and per-session breakdown
- Session resumption (`maestro resume`, `maestro run --resume`)
- Session transcript logging with `maestro logs` and JSON export
- Shell completions for bash, zsh, fish (`maestro completions <shell>`)
- Orphan worktree cleanup (`maestro clean`)
- Auto-merge support with configurable merge method
- Review council: multi-reviewer parallel dispatch with consensus

### Phase 3: Intelligence Layer

- Budget enforcement with per-session and global limits
- Stall detection via HealthMonitor with configurable timeout
- Retry policy with cooldown (`max_retries`, `retry_cooldown_secs`)
- Session progress tracking (Analyzing, Implementing, Testing, CreatingPR)
- Completion gates framework (TestsPass, FileExists, FileContains, PrCreated)
- CI status polling with configurable intervals
- Desktop notification dispatcher (Info / Warning / Critical / Blocker levels)
- Dependency graph ASCII visualization in TUI
- Model routing based on issue labels
- Label-based concurrency control for heavy tasks

### Phase 2: GitHub Integration

- Fetch issues via `gh` CLI with label filtering (`maestro:ready`)
- Priority-based scheduling (P0 / P1 / P2 labels)
- Dependency resolution with topological sort and cycle detection
- `blocked-by:#N` label and body parsing
- Automated PR creation with cost report and file list
- Label lifecycle management (ready -> in-progress -> done / failed)
- Milestone mode (`--milestone <name>`)
- Issue data caching with configurable TTL
- `maestro queue` and `maestro add` commands

### Phase 1: Multi-Session Pool + Split TUI

- Session pool with configurable max concurrency
- Automatic queue promotion when slots free up
- Git worktree creation per session for file isolation
- Split-pane TUI with per-agent panels
- File claim system to prevent concurrent edits
- Activity log with session labels and color-coded log levels

### Phase 0: Foundation (MVP)

- Single-session TUI with ratatui + crossterm
- Claude CLI stream-json output parser
- Session state machine (Queued -> Spawning -> Running -> Completed / Errored / Killed / Paused)
- State persistence to `maestro-state.json`
- Keyboard controls (q = quit, p = pause, r = resume, k = kill)
- Per-session and total cost tracking
- `maestro init` for config scaffolding
- `maestro status` and `maestro cost` commands
