# Changelog

All notable changes to Maestro are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.15.0] - 2026-04-23

Milestone "Guided Creation Flows" — transforms the startup experience into a persistent landing screen and ships two AI-assisted wizards for structured issue and milestone creation, plus a read-only project stats dashboard, a tabbed compact milestone view, and a marquee-scrolled header. Twelve issues closed via PR #446 on the bundled milestone branch.

### Added

- Persistent `LandingScreen` replaces the 1.2s timed splash: mascot + MAESTRO logo + 5-item menu (Dashboard / Create Issue / Create Milestone / Project Stats / Quit); j/k or Up/Down navigates, Enter activates, direct shortcuts (d/i/m/s/q) jump; Esc on Dashboard pops back to Landing; `--no-splash` bypasses Landing for Dashboard entry (#290)
- `IssueWizardScreen` scaffold with 10-step linear state machine (Context → TypeSelect → BasicInfo → DorFields → Dependencies → AiReview → Preview → Creating → Complete → Failed), `IssueCreationPayload` DTO carrying all DOR fields, and `TuiCommand::CreateIssue` + `TuiDataEvent::IssueCreated` (#291)
- `ProjectStatsScreen` read-only dashboard: milestone progress bars (ratatui `Gauge`), issue counts table (open/closed/ready/done/failed), session metrics (cost, tokens, success rate), last-10 recent activity; pure `aggregate()` helper keeps the math testable without async (#292)
- `MilestoneScreen` compact view redesign: left/right layout with tabbed right pane (Issues, Preview); Tab cycles tabs, `1`/`2` jump directly; issue list sorted by parsed dependency level (`count_blocked_by` ascending, ties on issue number); J/K navigates focused issue inside the right pane (#325)
- Marquee carousel on the header status bar: `App::status_bar_marquee: MarqueeState` + content-width fingerprint; renders static on fit, 3-phase scroll on overflow with span styles preserved; mirrors the existing stats-bar (#410) and issues-tab (#262) marquee integration (#417)
- Issue Wizard form steps: TypeSelect (Feature/Bug toggle via ←/→ or h/l), BasicInfo (Title + Overview with Tab cycling, Shift+Enter for multi-line newlines), DorFields (4 fields for Feature, 6 for Bug); Title must be non-empty to advance BasicInfo, Acceptance Criteria required to advance DorFields (#293)
- `MilestoneWizardScreen` scaffold with 9-step AI-guided flow (GoalDefinition → NonGoals → DocReferences → AiStructuring → ReviewPlan → Preview → Materializing → Complete → Failed); doc references validated as URL-or-existing-file; `claude --print` invocation via the canonical `adapt::prompts::run_claude_print` (#294)
- `c` keybinding on `MilestoneScreen` opens the Issue Wizard pre-filled with the selected milestone + a suggested `Blocked By` list derived from the milestone's open-issue leaves; `update_milestone_dependency_graph` helper ready for the description PATCH on create (#326)
- Dependency selection step on Issue Wizard: multi-select checkbox list of open GitHub issues via `TuiCommand::FetchWizardDependencies`; Space toggles, j/k navigates, Enter persists; pre-seeded `payload.blocked_by` (from #326 path) renders as already-checked (#295)
- AI Review companion step on Issue Wizard: structured critique prompt built from all DOR fields + Blocked By, run via `claude --print`; keys `r` revise (jumps back to BasicInfo), `s` skip, `Enter` continue, `R` retry on error; auto-launches on step entry via `tick_wizard_step_hooks` (#296)
- Milestone Wizard Review → Preview → Materialize → Complete/Failed: ReviewPlan accepts/rejects proposed issues with `a`/`x`; Preview renders an ASCII dependency graph via `level_buckets` BFS + `Sequence:` line in the project's `→` / `∥` convention; Materializing creates the milestone first, then each issue in dependency order with `Blocked By` rewritten to actual issue numbers; Complete shows the created milestone + issue URLs (#297)
- Issue Wizard Preview → Creating → Complete/Failed: `render_body_markdown` emits all DOR sections plus an auto-generated `## Definition of Done` checklist synthesised from Acceptance Criteria; `GhCliClient::create_issue` call auto-applies `maestro:ready` + `enhancement`/`bug` labels; Complete resets for another issue; Failed supports `r` retry (#298)

### Changed

- All wizard text fields accept `Event::Paste(…)` (bracketed paste, Cmd+V in the terminal) and Ctrl+V (reads the system clipboard via the existing `ClipboardProvider` trait). Image clipboard content → `payload.image_paths` rendered into the issue body as a `## Attachments` section. C0/C1 control characters (ANSI escapes, DEL) are stripped before insert, preserving `\n` and `\t`. Title fields collapse embedded newlines to spaces so GitHub accepts the payload.
- Global `q` → `ConfirmExit` gate in `input_handler::is_text_input_mode` now consults the active screen's `Screen::desired_input_mode()` instead of a hardcoded `TuiMode` allowlist (neovim-style mode-aware global shortcuts). Wizard Insert-mode surfaces swallow `q` as a typed character. `PromptInput` / `Settings` / `SessionSwitcher` retained as a fallback allowlist (SessionSwitcher doesn't implement the `Screen` trait yet).
- `ui::active_screen()` dispatch table now includes `Landing`, `IssueWizard`, `MilestoneWizard`, and `ProjectStats` — restoring help-overlay content, F-key hints, and keybinding resolution on those screens.
- `adapt::prompts::run_claude_print` is now `pub`; both wizards route through it with sensible defaults (sonnet model, current-dir cwd) instead of maintaining a duplicate in `src/tui/mod.rs`.
- `IssueWizardScreen::render_body_markdown` and `render_labels` promoted to free functions in the wizard module so the background `CreateIssue` task can call them without a screen handle (eliminates a ~70-line duplicate).

### Removed

- Deleted `src/tui/splash.rs` outright — replaced by `LandingScreen` (#290).

## [0.14.1] - 2026-04-22

### Fixed

- Settings screen footer omitted edit keys (Space/Enter/←→) for non-Flags widgets — `WidgetKind::edit_hint()` now returns a contextual `(key, label)` tuple per variant; `SettingsScreen::draw` builds the footer from the focused widget's hint; `KeymapProvider::keybindings()` gains a third `"Edit"` group so the `?` help overlay stays consistent (#432)
- Settings Ctrl+S save was a silent no-op in release builds — the config file path was never propagated from the loader into `App`, so `save_config` always received `None` and discarded all changes without error; fixed by introducing `LoadedConfig { config, path }` in `src/config.rs`, threading the resolved `PathBuf` through `setup_app_from_config` into `App.config_path`, and updating `screen_dispatch.rs` to read `app.config_path` directly instead of probing relative paths; `save_config` now returns `Err` when the path is absent; Ctrl+S surfaces failures as a 5-second title-bar flash (`Settings [Save failed: <msg>]` rendered in `accent_error`, message sanitized and truncated to 80 chars) (#437)
- Toggle widget rendered a blank checkbox indicator on iTerm2 with some Nerd Font installs — `draw()` was hardcoding glyph literals that could drift from the icon registry; fixed by routing through `icons::get(IconId::CheckboxOn/Off)` and updating the registry codepoints to the universally present Font Awesome core glyphs (U+F14A `nf-fa-check_square`, U+F0C8 `nf-fa-square`) which replace the unreliable legacy nf-oct variants (#433)
- Paste via terminal context menu (iTerm2 right-click, Cmd+V) no longer submits mid-paste or leaks to the underlying shell — `EnableBracketedPaste` is now enabled at TUI startup and `DisableBracketedPaste` is emitted on teardown; a new `Event::Paste(String)` arm routes pastes through `App::handle_paste` → `dispatch_paste_to_active_screen` → `PromptInputScreen::paste_text`, which inserts text verbatim into the textarea when the prompt editor is focused and treats the value as an image path when the image-list pane is focused; all other screens silently no-op. Pasted payloads are sanitized via `sanitize_paste` to strip C0 control bytes (ESC, NUL, BEL, DEL, …) while preserving `\n` and `\t`, so ANSI colour sequences in pasted terminal output no longer render as styled spans in the textarea or leak into the prompt sent to the model (#441)

## [0.14.0] - 2026-04-21

### Added

- Fork-handoff compression — `compress_handoff()` on `TurboQuantAdapter` produces a `CompressedHandoff` struct; integrated into `ForkPolicy` to keep continuation prompts within a configurable token budget (#343)
- System-prompt compaction — `compact_system_prompt()` on `TurboQuantAdapter`; integrated into `SessionPool::try_promote` to trim oversized system prompts before session launch (#344)
- State compression — `compact_session_history()` on `TurboQuantAdapter` returns a `StateCompactionReport`; `MaestroState::compact()` and `StateStore::save_compacted()` persist trimmed state (#345)
- Knowledge compression in `maestro adapt` — new `src/adapt/knowledge.rs` module (Phase 2.6); produces a token-budgeted `KnowledgeBase` and writes `.maestro/knowledge.md`; auto-loaded by `SessionPool::try_promote` as a system-prompt component (#347)
- TurboQuant savings projections dashboard — `src/tui/turboquant_dashboard.rs`; shows "Estimated Savings (projection)" when no fork-handoff compression data exists, "Actual Savings" once real handoff metrics are present; per-session `ACTUAL` / `proj.` kind markers; aggregate token and USD totals (#346)
- `SavingsProjection`, `SavingsKind`, `SessionSavings` public types and `project_savings()`, `session_savings()`, `implied_rate_per_token()` free functions in `src/turboquant/adapter.rs` (#346)
- `tq_handoff_original_tokens` and `tq_handoff_compressed_tokens` fields on `Session` (with `#[serde(default)]` for backward compat) — populated by `context_overflow.rs` after fork-handoff compression so the dashboard can surface real savings (#346)
- 3 new snapshot tests for `TurboQuantDashboard` (projections-only, mixed actual+projections, empty sessions) in `src/tui/snapshot_tests/turboquant_dashboard.rs` (#346)
- `TextRanker` trait and impl in `src/turboquant/adapter.rs` — shared text scoring primitive used by all compression paths
- `TokenBudget` helper in `src/turboquant/budget.rs` — greedy ranked-segment selection under a token limit; `BudgetSelection` struct (indices, tokens_used, truncated_first)
- Three new `TurboQuantConfig` fields: `fork_handoff_budget`, `system_prompt_budget`, `knowledge_budget` (token-limit knobs for each compression feature)
- Shared `Arc<TurboQuantAdapter>` on `App` — single adapter instance reused across all compression features
- Session intent classification (`work` vs `consultation`) used to drive retry decisions (#273)
- Skip hollow retry for consultation/Q&A prompts — no retry loop for questions (#274)
- `[sessions.hollow_retry]` config section with three policies: `always`, `intent-aware` (default), and `never`; replaces the flat `sessions.hollow_max_retries` field (#275)
- `HollowRetryPolicy` enum and `HollowRetryConfig` struct in `src/config.rs`; `merge_legacy_hollow()` pure function for backward-compatible TOML parsing (#275)
- Per-intent retry limits: `work_max_retries` (default 2) and `consultation_max_retries` (default 0) under `[sessions.hollow_retry]` (#275)
- Settings UI hollow-retry section in the Sessions tab: `[policy]` dropdown, `[work_max_retries]` stepper, `[consultation_max_retries]` stepper (#275)
- Interactive follow-up after `maestro adapt` — selectable next actions menu (#391)
- PRD source selection in adapt — local file, GitHub issue, or Azure DevOps work item (#390)

### Changed

- Replaced the A/B benchmark dashboard (#253) with the honest savings-projection dashboard; removed `partition_sessions`, `compute_panel_stats`, and `aggregate_tq_metrics` from `turboquant_dashboard.rs` (#346)
- Removed synthetic prompt-compression block from `event_handler.rs` (formerly in the `Completed` arm); honest projection replaces fabricated compression metrics (#346)
- Removed `TQ Ratio` column from `src/tui/token_dashboard.rs`; TurboQuant ownership moved to the dedicated savings dashboard (#346)
- Hollow retry dispatch is now intent-aware by default: work sessions retry up to 2 times, consultation sessions never retry (#275)
- `RetryPolicy` in `src/session/retry.rs` owns a `hollow: HollowRetryConfig` field (was flat `hollow_max_retries: u32`); `effective_max()` dispatches by policy and session intent (#275)
- `HollowRetryScreen` in `src/tui/app/completion_pipeline.rs` receives the per-intent `effective_max` rather than the raw work limit (#275)

> **Backward compatibility**: existing `sessions.hollow_max_retries = N` in `maestro.toml` still parses and maps to `work_max_retries = N` with policy `intent-aware`.

### Fixed

- Marquee-scroll the stats bar when the repo/branch line overflows the viewport width (#410)

### Security

- `.maestro/knowledge.md` write path enforces a 1 MiB size cap, rejects symlinks, and uses a TOCTOU-safe load sequence
- Session-prompt injection is envelope-wrapped to prevent prompt-injection via project content
- Handoff splitter enforces a 2000-segment cap to bound memory use in degenerate inputs

## [0.13.1] - 2026-04-17

### Added

- Configurable milestone naming convention in adapt settings (#368)
- PRD generator — standalone command + adapt integration (#370)
- Adapt AI scaffolding phase — generate .claude/ commands, skills, and subagents for target project (#371)

### Changed

- Add runtime state files to `.gitignore` (#352)
- Remove or scaffold src/modes stub module (#354)
- Consolidate GitHub integration under src/provider/github/ (#355)
- Consolidate Azure DevOps module into src/provider/azure_devops/ (#356)
- Consolidate src/flags/ store into src/state/ or document the boundary (#357)
- Split src/util.rs into focused sub-modules (#362)
- Extract CI polling service from src/tui/app/ (#363)
- Extract session spawning service from src/tui/app/ (#364)
- Extract work assignment service from src/tui/app/ (#365)
- Tech debt catalog (#366)

### Testing

- Add unit tests for src/adapt/ pipeline modules (#358)
- Add unit tests for src/review/ council and dispatch (#359)
- Enforce snapshot test review in CI via cargo-insta (#360)

### Documentation

- Document build.rs purpose and rerun-if-changed directives (#353)
- Add module-level documentation to src/turboquant/ (#361)
- "The Maestro Way" workflow guide — adapt output for onboarded projects (#369)

## [0.13.0] - 2026-04-16

### Added

- Context compaction adapter — apply TurboQuant to session prompts (#246)
- TurboQuant runtime toggle via feature flag (#252)
- System resource monitor in header status bar (#251)
- Token analytics — TurboQuant compression metrics (#249)
- TurboQuant A/B benchmark dashboard in TUI (#253)
- Consistent navigation system with breadcrumbs and back-stack (#342)
- `NavigationStack` struct with push/pop/peek/clear/breadcrumbs operations (#342)
- `list_labels()` and `create_label()` methods on `GitHubClient` trait (#348)
- `ensure_labels()` on `GhMaterializer` — auto-creates missing labels before issue creation (#348)

### Changed

- Replaced `confirm_exit_return_mode` with `NavigationStack` in `App` (#342)
- All `Esc` handlers now use `navigate_back_or_dashboard()` instead of manual mode assignment (#342)
- `ScreenAction::Push` / `ScreenAction::Pop` delegated to `navigate_to` / `navigate_back` (#342)

### Fixed

- Adapt materializer crashes when labels don't exist on target repo — HTTP 422 (#348)
- `AzDevOpsClient` updated with stub `list_labels()` / `create_label()` for trait compliance (#348)

### Documentation

- TurboQuant feature guide (#250)

## [0.12.0] - 2026-04-14

### Added

- TurboQuant config schema and feature flag (#242)
- PolarQuant core — Cartesian-to-polar vector transform (#243)
- QJL core — 1-bit Johnson-Lindenstrauss residual correction (#244)
- TurboQuant pipeline — compose PolarQuant + QJL (#245)
- Settings TUI — TurboQuant configuration tab (#247)
- Benchmarks and compression report CLI command (#248)
- [d]ismiss keybinding for Activity Log panel (#306)

### Changed

- Extracted icon mode detection into lib crate for cross-crate sharing (#307)
- Migrated SessionStatus symbols to centralized icon registry (#308)

## [0.11.1] - 2026-04-14

### Added

- Confirm exit dialog on `[q]` with Ctrl+C bypass (#318)
- Nerd Font icons for milestones and issues across all TUI views (#320)
- Project stats widget replacing dashboard header area (#323)

### Fixed

- Arrow key history no longer overwrites current prompt input (#317)

### Changed

- Extracted mascot + logo + repo info into reusable header brand widget with Nerd Font icons (#319)

## [0.11.0] - 2026-04-14

### Added

- Mascot companion system — core animation engine with Ratatui widget, dashboard panel widget, prompt bar companion, startup splash screen, and running session live feedback (#267, #268, #269, #270, #271)
- Unified PR workflow — session config for multi-issue PR creation, toggle in issue browser multi-select overlay, toggle in prompt composition with auto-detection (#301, #302, #303)
- Issue reference `#NNN` detection and highlighting in prompt text (#300)
- Comprehensive keybinding help overlay with searchable command list (#281)
- Context-sensitive inline keybinding hints per TUI mode (#282)
- Centralized icon registry with Nerd Font / ASCII dual variants (#286)
- Nerd Font icons for status bar header with ASCII fallback (#310)
- Standardized icons to Nerd Font set across TUI (#260)
- Marquee/carousel animation for overflowing issue names in issues tab (#262)
- Consolidated completion summary page for all finished sessions (#265)
- Redesigned context gauge with compact, retro-styled indicator (#266)
- Context-aware help bar with dimmed inactive keybindings (#259)
- Visual status transition effects — panel borders flash on state changes (#202)
- F-key status bar redesign with DOS-style layout and amber badges (#218)
- Session Complete summary popup is now toggleable/dismissable (#254)

### Fixed

- Issue browser preview now renders markdown with focus/scroll navigation (#289)
- Prompt composition text wraps correctly at box boundary (#263)
- Shift+Enter correctly inserts newline in prompt composition screens (#258)
- Markdown rendering wraps correctly in narrow session panels (#256)
- Grid layout panel selection indicator is now visually distinct (#257)
- Completed sessions are navigable/scrollable in grid view (#264)
- F-key bar no longer overlaps screen-specific keybindings at narrow widths (#280)
- MAESTRO logo last row alignment for T, R, O letters (#284)
- Milestone screen color hierarchy and selection visibility (#299)

### Changed

- Migrated all hardcoded icons to centralized icon registry (#287)

## [0.10.1] - 2026-04-11

### Added

- Changelog parser module (#237)
- What's New widget on HomeScreen (#238)
- ReleaseNotes screen with scrollable changelog (#239)
- Wire ReleaseNotes screen into App and screen dispatch (#240)

### Fixed

- Prompt history navigation — Up/Down arrows in the Compose Prompt screen now correctly recall previous prompts; history is always injected when creating `PromptInputScreen` (#232)
- Self-update asset resolution — asset names now use Rust target triples (e.g. `aarch64-apple-darwin`), checksum file resolves to `sha256sums.txt`, and `.tar.gz` archives are correctly extracted using the `flate2` + `tar` pipeline (#233)
- Ctrl+V paste causes flickering errors and app crash on Windows WSL (#235)

## [0.10.0] - 2026-04-10

### Added

- `maestro adapt` — onboard existing projects to maestro workflow (#87)
- `adapt` module scaffolding and data types (#88)
- Project scanner for `maestro adapt` Phase 1 (#89)
- Extend GitHubClient with `create_issue` and `create_milestone` (#90)
- Claude analyzer for `maestro adapt` Phase 2 (#91)
- Adaptation planner for `maestro adapt` Phase 3 (#92)
- Plan materializer for `maestro adapt` Phase 4 (#93)
- CLI integration and `cmd_adapt` entry point (#94)
- Tech debt catalog issue generation for `maestro adapt` (#95)
- AdaptWizard types and TuiMode variant (#207)
- AdaptScreen struct with Screen trait impl (#208)
- AdaptScreen rendering (#209)
- HomeScreen quick action for Adapt Project (#210)
- Wire AdaptScreen into App and screen dispatch (#211)
- Async adapt pipeline commands and data chaining (#212)
- End-to-end integration test for TUI adapt wizard (#213)
- PR Review screen with interactive TUI and markdown rendering (#229)

### Security

- Fix command injection via plugin system (#220)
- Fix argument injection via review dispatcher template variables (#221)
- Add checksum verification to auto-updater (#222)
- Remove crate-level `#![allow(dead_code)]` (#223)
- Fix worktree slug path traversal (#224)
- Fix state file race condition due to missing file locking (#225)
- Fix log file fallback panic on non-Unix platforms (#226)
- Replace `expect()` and `panic!()` in production code paths (#227)

## [0.9.0] - 2026-04-09

### Added

- Field-level validation with inline error messages for Settings screen (#75)
- Persistent background sessions with multi-window navigation (#63)

## [0.8.0] - 2026-04-09

### Added

- TOML serialization and write-back for Config (#70)
- Reusable TUI widget primitives for settings forms: TextInput, NumberStepper, Toggle, Dropdown, ListEditor (#71)
- SettingsScreen with tabbed section navigation across 11 config categories (#72)
- Settings widgets wired to Config fields across all tabs with sync-on-change (#73)
- Dirty state tracking, save (Ctrl+s), and reset (Ctrl+r) for Settings (#74)
- Live theme preview toggle in Settings theme tab (#76)
- Settings screen integration tests and help overlay (#77)
- Configurable Issues screen layout mode (vertical/horizontal) and density (default/comfortable/compact) in maestro.toml (#121)
- Layout and density settings wired to interactive Settings screen (#122)
- Feature flags display in Settings screen with name, state, source, and description (#146)

## [0.7.0] - 2026-04-09

### Added

- `maestro sanitize` CLI command for codebase health analysis (#106)
- Phase 1: Static dead-code scanner via `syn` AST parsing — detects unused functions, structs, enums, imports, modules, and files (#107)
- Phase 2: AI-powered code smell analyzer using Claude CLI — detects Fowler catalog smells (Feature Envy, Data Clumps, Primitive Obsession, Divergent Change, Shotgun Surgery, Duplicated Code) (#108)
- Phase 3: Multi-format report generator — terminal (colored), JSON (machine-readable), and Markdown output (#109)
- End-to-end sanitize pipeline with `--path`, `--output`, `--severity`, `--skip-ai`, `--model` flags (#110)
- Long Method heuristic (>50 lines warning, >100 critical) and Large Class heuristic (>200 lines warning, >400 critical) (#107)
- Interactive TUI sanitize results screen with two-panel layout, severity filtering, and j/k navigation (#111)
- `--skip-ai` flag to run static analysis only without spawning Claude CLI (#110)
- Graceful AI failure fallback — scan-only results reported if Claude CLI fails (#110)

## [0.6.2] - 2026-04-09

### Fixed

- Remove `--bare` flag from Claude CLI session invocation — fixes OAuth/Max plan authentication broken in Claude CLI v2.1.97 (#188)
- Add `maestro-prompt-history.json` to `.gitignore`

## [0.6.1] - 2026-04-09

### Added

- "Update Maestro" quick action in dashboard home screen (`[u]` keybinding) — triggers version check and self-update flow

### Fixed

- Release workflow: Homebrew tap update now checks out the tap repo directly instead of relying on repository_dispatch, fixing silent failures when the token lacked Contents permission
- Release workflow: use environment variables for all interpolated values (GitHub Actions security best practice)

## [0.6.0] - 2026-04-09

### Added

- Token consumption tracking: capture granular token metrics (input, output, cache read, cache write) from Claude CLI stream-json output (#161)
- Token analytics dashboard (`[t]` keybinding) with per-session breakdown, cache hit ratio, and cost-per-kToken (#161)
- Token Report entry in Dashboard Quick Actions menu (#161)
- Prompt history persistence to disk with Up/Down arrow navigation in prompt input screen (#170)
- Configurable `max_prompt_history` (default: 100) in `maestro.toml` (#170)
- Automatic retry for hollow/failed session completions with configurable `hollow_max_retries` (default: 1) (#171)
- Hollow retry screen (Retry/Skip/View Logs) when auto-retries are exhausted (#171)
- Custom prompt input when selecting an issue for session launch (#99)
- Shared prompt overlay for multi-selected issues (#130)
- Work queue planner with dependency validation (#65)
- File conflict predictor for pre-launch validation (#66)
- Queue confirmation screen with conflict warnings (#67)
- Sequential session executor for work queues (#68)
- Granular CI check-run details from `gh pr checks` (#123)
- CI monitor TUI widget — live progress box for PR checks (#124)
- CI monitor integration into issues screen and session detail (#125)
- PR merge conflict detection after queue execution (#138)
- Conflict resolution suggestions in completion summary (#139)
- Conflict resolver session launcher from completion summary (#140)

### Fixed

- Detect and flag "hollow" session completions (zero cost, zero files, no tool calls, <30s) with visual warnings across all TUI views (#169)

### Changed

- Decompose oversized files into focused modules under 500-line limit (#172-#179)
- CI file size lint enforcing 500-line max per `.rs` file (#172)
- Parser `parse_stream_line` now returns `Vec<StreamEvent>` for multi-event extraction (#161)
- `RetryPolicy` extended with `hollow_max_retries` field and `from_config` constructor (#171)
- `session_label` helper visibility changed to `pub(crate)` for cross-module reuse

## [0.5.3] - 2026-04-08

### Added

- Feature flag registry and store with `Flag` enum and `FeatureFlags` runtime store (#141)
- Cargo `[features]` for compile-time gating of experimental modules (#142)
- `[flags]` config section in `maestro.toml` for per-project feature flag overrides (#143)
- `--enable-flag` and `--disable-flag` CLI args on the `Run` subcommand for runtime flag overrides (#143)
- `FeatureFlags` wired into `App` struct — three features gated behind runtime flags (#145):
  - `Flag::AutoFork` gates auto-fork on context overflow
  - `Flag::CiAutoFix` gates automatic CI fix session spawning
  - `Flag::ContinuousMode` gates continuous mode activation
- `PendingPr` and `PendingPrStatus` structs for tracking failed PR creation attempts (#159)
- `PrRetryPolicy` with exponential back-off (default 3 attempts) and `OrphanBranch` recovery (#159)
- PR creation retry loop and manual trigger for stuck PRs (#159)

### Fixed

- `gh` CLI auth failure detection with clear error surfacing to user (#158)
- Milestone issue browser no longer shows closed issues (#150)

## [0.5.2] - 2026-04-07

### Added

- Markdown-to-ratatui rendering module (#133)
- Syntax highlighting for fenced code blocks (#137)
- Markdown rendering wired into panel and fullscreen views (#136)
- Thinking animation spinner with elapsed metadata (#134)
- CI monitor TUI widget for live PR check status (#124)
- File conflict predictor for pre-launch validation (#66)
- Work queue planner with dependency validation (#65)
- Granular CI check-run details from `gh pr checks` (#123)
- Custom prompt input overlay for issue session launch (#99)

### Fixed

- Completion summary trapping navigation — Esc and [i] don't escape (#148)

### Documentation

- Self-upgrade instructions added to README

## [0.5.1] - 2026-04-07

### Added

- Self-upgrade via CLI/TUI with user confirmation (#118) — async version check on startup via GitHub Releases API, non-blocking upgrade banner, binary download with backup/rollback, restart confirmation
- New `src/updater/` module with `UpdateChecker` trait, `Installer`, `RestartBuilder`, and `UpgradeState` state machine
- Security hardening: download URL allowlist (GitHub domains only), 120s timeout, 200MB size limit, rollback error logging

### Fixed

- Milestone filter persists on "All Issues" view when switching between milestone and non-milestone contexts (#117)

### Detailed Changes

### Self-Upgrade via CLI/TUI with User Confirmation (#118)

- `src/updater/mod.rs` — `UpgradeState` enum (Hidden, Available, Downloading, ReadyToRestart, Failed) state machine; `ReleaseInfo` struct; `is_trusted_download_url()` validates download URLs against an HTTPS allowlist of GitHub domains; `GITHUB_REPO` and `MAX_DOWNLOAD_SIZE` constants
- `src/updater/checker.rs` — `Version` struct with semver parsing (strips `v` prefix, handles pre-release suffixes); `UpdateChecker` trait returning `Option<ReleaseInfo>` from a single API call; `GitHubReleaseChecker` production impl hitting `/releases/latest` with 5s timeout; `parse_releases_response()` for JSON parsing with pre-release filtering
- `src/updater/installer.rs` — `Installer` struct with `install_with_backup()` (reads original, writes backup, replaces binary, sets permissions, rolls back on failure with logged errors); `download_and_install()` with URL validation, 120s timeout, and 200MB Content-Length guard; `restart_with_same_args()` uses POSIX `execvp()` on Unix
- `src/updater/restart.rs` — `RestartBuilder` and `RestartCommand` pure data structs for testable restart command construction without side effects
- `src/tui/app.rs` — `upgrade_state: UpgradeState` field added to `App`; `TuiDataEvent::VersionCheckResult` and `TuiDataEvent::UpgradeResult` variants; `handle_data_event()` arms for state transitions
- `src/tui/mod.rs` — `spawn_version_check()` spawns async version check before event loop; `spawn_upgrade_download()` spawns binary download on user confirmation; key handlers for `[u]` upgrade, `[Esc]` dismiss, `[y]` restart, `[n]` skip restart
- `src/tui/ui.rs` — `draw_upgrade_banner()` renders state-specific banners: blue "UPDATE" for available, yellow "DOWNLOADING" for in-progress, green "READY" for restart confirmation, red "ERROR" for failures

### Milestone Filter Persists on All Issues View (#117)

- `src/tui/mod.rs` — `handle_screen_action()` now always constructs a fresh `IssueBrowserScreen` when navigating to "All Issues" from a non-milestone context, instead of reusing a stale screen that retained a milestone-scoped filter from a previous navigation
- `src/tui/screens/issue_browser.rs` — `set_issues()` now calls `reapply_filters()` after replacing the issue list so that any active milestone filter is correctly applied to the newly delivered data rather than being silently dropped

## [0.5.0] - 2026-04-07

### Added
- Mandatory dependency chain and graph guardrail for issue/milestone creation (#113)
- [f] Fix action to completion overlay for failed gates (#104)
- Enhanced real-time session activity feedback — thinking, streaming, tool details (#102)
- Submit prompt with Enter key, Shift+Enter for newlines (#101)
- Dashboard suggestion refresh after session completion (#86)
- Continuous work mode — auto-advance to next ready issue (#85)
- Post-session activity log with cost summary and next actions (#84)
- Return to dashboard after session completion instead of exiting (#83)
- Work suggestions and quick commands (#35)
- Session launch with worktree isolation from TUI (#36)
- Provider auth verification and user context (#34)
- Standardized issue templates with Definition of Ready (#53)
- Onboarding preflight check — `maestro doctor` (#49)
- CI error detection and auto-fix loop (#41)
- Auto-fmt, clippy, and test completion gates (#40)
- Live GitHub data fetching and session launch from TUI (#46, #47, #48)
- Interactive TUI screens — dashboard, issue browser, milestone view (#31, #32, #33)
- Session prompt guardrails with auto-detected language (#43)

### Performance
- Benchmark session parser throughput (#19)

### Documentation
- Man page and shell completion installation guide (#18)

### Testing
- TUI rendering snapshot tests (#16)
- Integration test suite for end-to-end session lifecycle (#15)

### Detailed Changes

### Mandatory Dependency Chain and Graph Guardrail for Issue and Milestone Creation (#113)

- `.claude/CLAUDE.md` — Critical Premise #5 added: "DEPENDENCY CHAIN AND GRAPH — NON-NEGOTIABLE"; rules require an explicit dependency graph for issues that have blockers, and for milestones consisting of multiple issues; DOR table updated to mark `Blocked By` as required for both Feature and Bug issues
- `.github/ISSUE_TEMPLATE/feature.yml` — `Blocked By` field set to `required: true`; new `Dependency Graph` textarea field added (optional) for documenting ASCII dependency graphs when creating multi-issue features or epics
- `.github/ISSUE_TEMPLATE/bug.yml` — `Blocked By` field set to `required: true` with placeholder guidance to use "None" if there are no dependencies
- GitHub v1.0.0 milestone updated via API to include dependency graph section in its description

### Add [f] Fix Action to Completion Overlay for Failed Gates (#104)

- `src/gates/types.rs` — `GateResult` derives `Serialize`/`Deserialize` (round-trip support for persisting gate results on the session)
- `src/session/types.rs` — `GateResultEntry` struct added (`gate`, `passed`, `message`) as a lightweight, session-local mirror of `gates::types::GateResult` that avoids a cross-module dependency; `gate_results: Vec<GateResultEntry>` field added to `Session` (serde default, persisted to `maestro-state.json`); `issue_number` and `model` fields were already present and are now surfaced in the completion overlay
- `src/tui/app.rs` — `GateFailureInfo` struct added (`gate_name`, `message`) carrying per-gate failure detail for the overlay; `CompletionSessionLine` extended with `gate_failures: Vec<GateFailureInfo>`, `issue_number: Option<u64>`, and `model: String` fields; `CompletionSummaryData::has_needs_review()` method added — returns `true` when any session line has `NeedsReview` status; `build_completion_summary()` populates `gate_failures` by filtering `session.gate_results` for failed entries and mapping them to `GateFailureInfo`; gate results are persisted onto `ManagedSession` during gate execution in `check_completions()`; `spawn_gate_fix_session()` method added — reads `gate_failures` from a `NeedsReview` `CompletionSessionLine`, constructs a fix prompt via `build_gate_fix_prompt()`, creates a new `Session`, and adds it to the pool; `build_gate_fix_prompt()` private function constructs a structured unattended prompt embedding the issue number and per-gate failure messages
- `src/tui/ui.rs` — `draw_completion_overlay()` extended: per-session gate failure lines are rendered below the error summary with a `✗ <gate_name> <message>` format in warning/error colors; `[f] Fix` keybinding is appended to the keybindings bar only when `summary.has_needs_review()` returns `true`
- `src/tui/mod.rs` — `CompletionSummary` key-intercept branch extended with an `[f]` handler: collects all `NeedsReview` sessions from `completion_summary`, calls `app.spawn_gate_fix_session()` for each, clears the summary, and transitions to `Overview` mode

### Enhanced Real-Time Session Activity Feedback (#102)

- `src/session/types.rs` — `StreamEvent::Thinking { text }` variant added to represent extended thinking blocks emitted by Claude; `command_preview: Option<String>` field added to `StreamEvent::ToolUse` to carry the first ~60 characters of a Bash command for richer activity messages
- `src/session/parser.rs` — `parse_assistant_event()` now matches `"thinking"` message type and emits `StreamEvent::Thinking { text }`; Bash tool input is inspected for a `"command"` key and its value is stored as `command_preview` (truncated at a safe char boundary with a `…` suffix when longer than 60 characters); non-Bash tools always receive `command_preview: None`
- `src/session/manager.rs` — `SessionManager` gains `thinking_start: Option<Instant>` field; on the first `Thinking` event the clock starts and `"Thinking..."` is logged to the session activity; when any non-Thinking event follows, the elapsed duration is logged as `"Thought for Xs"` and `thinking_start` is cleared; `ToolUse` activity messages are now richer: file-touching tools show the file path, Bash tool shows `$ <command_preview>`, other tools show the tool name with the file path when available; `ToolResult` messages include elapsed time since the matching `ToolUse` started
- `src/tui/activity_log.rs` — `LogLevel::Thinking` variant added; rendered in `theme.accent_success` (green), visually distinct from `Info`, `Tool`, `Warn`, and `Error`
- `src/tui/app.rs` — `StreamEvent::AssistantMessage` text chunks are no longer forwarded to the global activity log (anti-flood); `StreamEvent::Thinking` is handled silently in the event router — thinking state is tracked per-session via `current_activity` in `manager.rs` without generating a global log entry
- `src/session/logger.rs` — `Thinking` arm added to the file logger: emits `[HH:MM:SS] THINKING: <text>` lines to the per-session log file for offline inspection

### Submit Prompt with Enter Key, Shift+Enter for Newlines (#101)

- `src/tui/screens/prompt_input.rs` — `Enter` now submits the prompt and launches a session (previously `Ctrl+S`); `Shift+Enter` inserts a newline in the prompt body (previously `Enter`); `Ctrl+S` removed as a submission keybinding; keybinds bar updated to show `Enter: Submit` and `Shift+Enter: New line`

### Dashboard Suggestion Refresh After Session Completion (#86)

- `src/tui/screens/mod.rs` — `ScreenAction::RefreshSuggestions` variant added; triggers a suggestion reload from the dashboard without a full navigation round-trip
- `src/tui/screens/home.rs` — `loading_suggestions: bool` field added to `HomeScreen`; when `true`, the suggestions panel renders a `"Loading..."` placeholder instead of stale data; `set_suggestions()` clears the flag on delivery; `R` (uppercase) key binding added — emits `ScreenAction::RefreshSuggestions` for on-demand manual refresh
- `src/tui/app.rs` — `transition_to_dashboard()` now sets `loading_suggestions = true` on the `HomeScreen` and queues `TuiCommand::FetchSuggestionData` so suggestions are always up-to-date when returning from a completed session; the `SuggestionData` data event clears the flag after delivery
- `src/tui/mod.rs` — `RefreshSuggestions` branch added to `handle_screen_action()`: sets `loading_suggestions = true` and queues `FetchSuggestionData`; `CompletionSummary` dismiss path delegates to `transition_to_dashboard()` which now handles the refresh automatically
- 8 new tests across `home.rs`, `app.rs`, and `tui/mod.rs`: cover default flag state, flag cleared by `set_suggestions()`, `R` key emitting the correct action, `transition_to_dashboard()` setting the loading flag and queuing `FetchSuggestionData`, and `RefreshSuggestions` action wiring in the event handler

### Continuous Work Mode (#85)

- `src/continuous.rs` — new `ContinuousModeState` and `ContinuousFailure` structs; state machine that tracks current issue, completed/skipped counts, and accumulated failures; `on_issue_completed()`, `on_issue_failed()` (pauses the loop), `skip()`, and `resume()` transition methods
- `src/cli.rs` — `--continuous` / `-C` flag added to `maestro run`; when set, maestro auto-advances to the next ready issue after each session completion
- `src/main.rs` — `--continuous` flag wired through `setup_app_from_config()`; forces `max_concurrent = 1` when continuous mode is active to ensure sequential issue processing
- `src/tui/app.rs` — `TuiMode::ContinuousPause` variant added; `continuous_mode: bool` field on `App`
- `src/tui/mod.rs` — `ContinuousPause` key-intercept overlay added: `[s]` skips the failed issue and advances, `[r]` retries the issue, `[q]` quits the continuous loop
- `src/tui/ui.rs` — `ContinuousPause` render branch added with pause overlay showing failure details; status bar indicator displays continuous mode state (current issue number, completed count, skipped count)
- `src/work/assigner.rs` — `mark_pending()` transitions a work item back to `Pending` status; `mark_pending_undo_cascade()` cascades the undo to all dependent items in the dependency graph

### Post-Session Activity Log with Cost Summary and Next Actions (#84)

- `src/tui/app.rs` — `CompletionSessionLine` gains `pr_link: Option<String>` and `error_summary: Option<String>` fields; `build_completion_summary()` populates `pr_link` by matching the session's `issue_number` against `pending_pr_checks` (resolved to a full `https://github.com/{repo}/pull/{N}` URL when a repo slug is available, otherwise `#N`) and falls back to `ci_fix_context.pr_number`; `error_summary` is set only for `Errored` sessions — it picks the last activity-log entry whose message starts with `"Error:"` or `"E:"` (or the last entry as a fallback) and truncates it to 80 characters with a trailing `...`
- `src/tui/ui.rs` — `draw_completion_overlay()` extended with two new rendering sections: PR links are appended to the session row as underlined, `accent_info`-colored spans; error summaries are rendered on a dedicated indented line in `accent_error` color; the dismiss hint is replaced with a full keybindings bar: `[i]` Browse issues, `[r]` New prompt, `[l]` View logs, `[q]` Quit, `[Esc]` Dashboard — all keys styled with `theme.keybind_key`
- `src/tui/mod.rs` — `CompletionSummary` key-intercept branch extended with three new handlers: `[i]` clears the summary, creates a loading `IssueBrowserScreen`, queues `FetchIssues`, and transitions to `IssueBrowser` mode; `[r]` clears the summary, creates a `PromptInputScreen`, and transitions to `PromptInput` mode; `[l]` clears the summary and transitions to `Overview` mode (activity log view); scroll keys `j`/`k`/Up/Down delegate to `panel_view` for log scrolling within the overlay

### Return to Dashboard After Session Completion (#83)

- `src/cli.rs` — `--once` flag added to `maestro run`; when set, maestro exits after all sessions complete (preserves previous behaviour for CI and scripting use cases)
- `src/tui/app.rs` — `TuiMode::CompletionSummary` variant added; `CompletionSummaryData` struct and `CompletionSessionLine` struct hold the per-session summary shown in the overlay; `once_mode: bool` field on `App` controls exit-vs-return behaviour; `build_completion_summary()` collects session outcomes; `completion_summary` field stores the active overlay data; `return_to_dashboard()` transitions from the overlay back to `Dashboard` mode and refreshes suggestions
- `src/tui/mod.rs` — `CompletionSummary` intercept branch added to the key-event handler (any key dismisses the overlay); exit path now checks `once_mode`: exits immediately when `true`, otherwise builds the summary and transitions to `CompletionSummary` mode; `Dashboard` mode is restored on dismiss
- `src/tui/ui.rs` — `TuiMode::CompletionSummary` render branch added; `draw_completion_summary()` renders a centred overlay with per-session outcome rows and a dismiss prompt
- `src/main.rs` — `once_mode` propagated from the parsed CLI flag into `App` via `setup_app_from_config()`

## [0.4.0] - 2026-04-06

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
