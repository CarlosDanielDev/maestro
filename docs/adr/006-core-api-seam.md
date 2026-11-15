# ADR 006 — maestro-core Public API Seam

- **Status:** Accepted
- **Date:** 2026-11-15
- **Tracking issue:** [#1303](https://github.com/CarlosDanielDev/maestro/issues/1303) (epic [#1302](https://github.com/CarlosDanielDev/maestro/issues/1302))
- **Feeds:** #1304 (workspace skeleton), #1305 (core behind the API), #1306 (TUI crate), #1307 (CLI slim)
- **Seed:** the `ScreenAction` contract in `docs/tui-design-system.md` §3 ("Screens never mutate the app")

---

## Problem

Maestro is one crate. Any file in `src/tui/` can import any internal type, and
does: the TUI reaches 33 top-level modules, the CLI (`src/main.rs`,
`src/cli.rs`, `src/cli_team.rs`, `src/commands/`) reaches 27. Before #1305
moves any file, the frontends need a written list of what they may consume,
or the split lands on a boundary made of `pub` soup.

---

## Decision

**State reads stay. Mutations go through an action enum. Events are added
only where polling forces a polling loop.**

The core API has four parts. Every other `pub` item in core is internal and
may change without notice once #1305 enforces the crate boundary.

| Part | What a frontend does | Today's symbols (seed) |
|------|----------------------|------------------------|
| **Session events** | subscribes to a channel | `session::manager::SessionEvent { session_id, event: StreamEvent }` over `mpsc::UnboundedSender` (TUI: `App::event_rx`); `session::interaction::TurnEvent` for interactive turns |
| **Costs** | reads cost from events and the budget | `StreamEvent::CostUpdate` / `Completed { cost_usd }`, `budget::BudgetEnforcer`, `orchestration::cost::estimate_cost_usd`, `agent_provider::quota` |
| **Config** | loads, reads, saves | `config::Config` (`load`, `find_and_load_in`), `config::schema` (schema-driven Settings), `flags::store::FeatureFlags`, `settings::SettingsStore` |
| **Actions** | sends a command, never mutates core state directly | a core `Action` enum cut from the core-facing `ScreenAction` variants (table below); the CLI sends the same variants |
| **State (read)** | reads a snapshot | `state::store::StateStore::load` → `state::types::MaestroState` (persisted to `maestro-state.json`) |

### Seam shape — why reads and actions, not events everywhere

The TUI already polls `MaestroState` and drains `event_rx` each tick
(`src/tui/mod.rs`, `while let Ok(evt) = app.event_rx.try_recv()`). Polling a
shared snapshot is cheap and already works, so state reads stay. Mutations
are the risky part — today a screen returns `ScreenAction::LaunchSession(..)`
and `screen_dispatch.rs` drives `SessionPool` (`src/session/pool.rs`, 2,274
lines) directly — so every mutation becomes one variant of a core `Action`
enum and core owns the effect. A new event type is added only when a frontend
would otherwise need its own polling loop for async work (the visible-feedback
rule in `docs/tui-design-system.md` §4); the session event channel already
covers sessions, costs and turns.

**Rejected — events for everything.** A full event bus means rewriting every
TUI screen that reads `MaestroState` today, for no new behavior. That is the
"no behavior change" line #1302 draws.

**Rejected — frontends poll a shared store for everything, including
mutations.** Writes from two frontends to one file have no owner and no
ordering; that is the coupling the split exists to remove.

### `ScreenAction` split

`ScreenAction` (`src/tui/screens/mod.rs:127`) mixes navigation and core
mutations. The split in #1305/#1306 keeps it as the screen contract and wraps
a core enum:

| Stays in `maestro-tui` (navigation / view) | Becomes core `Action` (mutation or core query) |
|--------------------------------------------|-----------------------------------------------|
| `None`, `Push`, `Pop`, `PushTeamWizard`, `OpenIssueWizardForMilestone`, `OpenFleetRow`, `PreviewTheme`, `LogActivity`, `OpenWorktreeShell` | `LaunchSession`, `LaunchSessions`, `LaunchPromptSession`, `LaunchUnifiedSession`, `LaunchTeam`, `LaunchQueue`, `LaunchConflictFix`, `LaunchCiFix`, `RetryHollow`, `ResumeOrLaunchIssue`, `AdoptSession`, `SendInteractionTurn`, `QuitInteraction`, `UpdateConfig`, `ResetSettingsFromDetection`, `NormalizeAgentConfig`, `StartAdaptPipeline`, `SubmitPrReview`, `CheckForUpdate`, `FetchPrDetail`, `RefreshSuggestions`, `OpenInteractionDiff` |

Only the right column's payload types that live under `src/tui/` move to
core with the enum: `SessionConfig`, `PromptSessionConfig`,
`UnifiedSessionConfig`, `ConflictFixConfig`, `CiFixConfig`
(`src/tui/screens/mod.rs`; `SessionConfig` also has a twin in
`src/tui/app/types.rs`). `TeamInput` (`orchestration::types`) and
`AdaptConfig` (`adapt`) already live in core modules. The exact enum name and
shape is #1305's call; this ADR fixes only which variants cross the seam.

---

## API sketch — what each frontend consumes

Measured 2026-11-15 on `origin/main` (`bebde64`) over non-test code
(`#[cfg(test)]` modules, `*test*.rs` files and comments excluded). Counted:
every `use` tree, expanded (`use crate::a::{B, c::D}` gives two paths), and
every inline path that names a top-level module — `crate::<module>::…`
anywhere, bare `<module>::…` in the crate root `src/main.rs` (where
`adapt::cmd_adapt`, `config::run_startup_migration`, `insight::run_cli` and
`sanitize::cmd_sanitize` are called without `crate::`), and bare paths in
files that `use crate::<module>;`. Top-level modules are the `mod` lines of
`src/main.rs`. A module both frontends reach is core API. Symbols listed are
the ones used, not the whole module.

### Both frontends → core API

| Module | TUI uses | CLI uses |
|--------|----------|----------|
| `session` | `SessionEvent`, `ManagedSession`, `pool::SessionPool`, `interaction::{TurnEvent, TurnRecord, TurnState}`, `logger::SessionLogger`, `discovery`, `adopt`, `fork` | `types::{Session, SessionStatus}`, `logger::SessionLogger`, `cleanup::CleanupManager`, `worktree::{WorktreeManager, GitWorktreeManager}`, `role`, `image` |
| `config` | `Config` (`load`, `find_and_load_in`), `AgentsConfig`, `LayoutConfig`, `schema::*`, `plan_agent_config_upgrade` | `Config`, `LoadedConfig`, `ResolvedAgentConfig`, `NotificationsConfig`, `AgentKind`, `run_startup_migration`, `agents_remote_auth`, `transport_warning` |
| `state` | `store::StateStore`, `types::{MaestroState, TeamRun, IssueNumber}`, `file_claims`, `progress`, `prompt_history` | `store::StateStore`, `types::MaestroState` |
| `orchestration` | `scheduler::Scheduler`, `team::{ResolvedTeam, TeamConfig}`, `loader::Loader`, `dag`, `preflight`, `cost::estimate_cost_usd` | `scheduler::Scheduler`, `team::ResolvedTeam`, `loader::Loader`, `dag`, `dispatch`, `primitives` |
| `agent_provider` | `quota::*`, `claude_pricing::UNPRICED_MODEL_CODE`, `types::AgentHealthCheck` | `AgentProvider`, `AgentProviderFactory`, the per-vendor providers, `minimax` |
| `provider` | `RepoProvider`, `create_provider`, `types::{Issue, Milestone, PullRequest}`, `github::*` | `create_provider`, `types::Issue`, `github::client` |
| `work` | `assigner::WorkAssigner`, `queue::WorkQueue`, `executor::QueueExecutor`, `types::{WorkItem, WorkStatus}`, `conflicts`, `activity` | `assigner::WorkAssigner`, `types::WorkItem`, `conflicts` |
| `budget` | `BudgetEnforcer`, `projector::BudgetProjector`, `quota_snapshot`, `sanitize` | `BudgetEnforcer::new` |
| `adapt` | `AdaptConfig`, `analyzer`, `planner`, `materializer`, `scanner`, `prd_source::PrdSource`, `suggestions`, `knowledge` | `cmd_adapt`, `cmd_prd`, `AdaptConfig`, `PrdConfig`, `prd_source::PrdSource` |
| `sanitize` (non-screen part) | `screen::SanitizeScreen` only — see TUI only | `cmd_sanitize`, `SanitizeConfig`, `OutputFormat`, `Severity` |
| `flags` | `Flag`, `FlagSource`, `store::FeatureFlags` | `Flag`, `store::FeatureFlags` |
| `review` | `dispatch::ReviewDispatcher`, `council`, `auto_review`, `bypass::BypassSource`, `types` | `bypass::BypassSource` |
| `gates` | `runner::{GateRunner, GateCheck}`, `types::{CompletionGate, GateResult}` | `UntrustedGateRunner` |
| `notifications` | `dispatcher::NotificationDispatcher`, `types`, `slack`, `desktop` | `dispatcher::NotificationDispatcher` |
| `settings` | `SettingsStore`, `CavemanModeState` | `FsSettingsStore` |
| `modes` | `resolve_mode_for_labels`, `resolve_session_mode_config`, `mode_from_labels` | `resolve_mode_for_labels`, `resolve_session_mode_config` |
| `prompts` | `PromptBuilder`, `resolve_guardrail` | `PromptBuilder::build_issue_prompt_with_images` |
| `plugins`, `models`, `continuous`, `turboquant`, `init`, `util` | `PluginRunner`, `ModelRouter`, `ContinuousModeState`, `turboquant::adapter::TurboQuantAdapter`, `init::{walk, render_or_merge}`, `util` formatting/validation helpers | `PluginRunner`, `ModelRouter::new`, `ContinuousModeState::new`, `turboquant::{pipeline, types}`, `init::{render_or_merge, scaffold, ignore}`, `util::validate_branch_name` |

### TUI only

`prd`, `milestone_health`, `updater`, `changelog`, `git` (`GitOps`,
`CliGitOps`), `system` (`ResourceMonitor`) — domain or I/O work with no
ratatui; **core**, reached today only through TUI screens.

`mascot`, `icons`, `icon_mode`, `sanitize::screen` — presentation. The rest
of `sanitize` (scanner, `cmd_sanitize`, report types) is core: the CLI calls
it (row above). Two hold
ratatui code outside `src/tui/` (`src/mascot/widget.rs`,
`src/sanitize/screen.rs`); **these move to `maestro-tui`** (or split the
widget out) — core must not depend on ratatui.

### CLI only

`doctor`, `templates`, `insight` (`insight::run_cli`) — **core** (no
frontend code; the TUI may use them later).

### Frontend ↔ frontend edges (must be cut by #1305–#1307)

| Edge | Where | Resolution |
|------|-------|------------|
| TUI → `commands::setup` (`setup_app_from_config_with_trust`, `build_notification_dispatcher`, `DEFAULT_PLUGIN_TIMEOUT_SECS`) | `src/tui/app/{review,session_lifecycle,interaction_pipeline}.rs`, `src/tui/screen_dispatch.rs` | app bootstrap that is not CLI parsing moves to core (or into `maestro-tui` if it builds an `App`) |
| TUI → `commands::agent_provider_map` | `src/tui/app/mod.rs` | moves to core (it maps config to providers) |
| CLI → `tui::{run, app::App, app::TuiMode, app::TuiCommand, screens::home, screens::HomeScreen}` | `src/commands/{dashboard,run,resume,setup}.rs` | allowed: the binary launches the TUI. Narrow to one entry point (`maestro_tui::run` plus a launch-options struct) so the CLI stops building `HomeScreen` and pushing `TuiCommand` itself |
| CLI → `tui::activity_log::LogLevel`, `tui::app::work_assigner`, `tui::clipboard::strip_ansi` | `src/commands/{run,setup,logs}.rs` | core types living in the TUI tree; move to core |

---

## Consequences

- #1305 makes the tables above the `pub` surface of `maestro-core`: the
  "Both frontends", "TUI only" (core part) and "CLI only" sections, including
  every CLI entry point listed (`adapt::cmd_adapt`, `adapt::cmd_prd`,
  `sanitize::cmd_sanitize`, `insight::run_cli`, `doctor::*`,
  `config::run_startup_migration`, `templates`). Anything not listed may
  become `pub(crate)`. If #1305 finds another symbol a frontend calls, it is
  added to the surface with a one-line edit to this ADR in the same PR —
  never made `pub(crate)` and broken.
- `maestro-cli` depends on `maestro-core` and on `maestro-tui` through one
  launch entry point — the epic's "frontends depend only on core" holds for
  everything except starting the TUI.
- No code moves in #1303; no visibility tricks (out of scope — the split
  enforces it mechanically).

---

## Amendments from #1305 (core move)

- **Surface additions** — symbols the frontends call that were `pub(crate)`
  or private, now `pub`: `budget::sanitize`, `config::{agents_remote_auth,
  default_max_prompt_history, schema::{dynamic, PROJECT_TABLE, BUDGET_TABLE,
  schema_for_config}}`, `doctor::check_agent_config_schema_in`,
  `notifications::{desktop, slack::validate_webhook_url}`,
  `orchestration::loader::write_preset_file`,
  `provider::github::ci::{local_gate_for_check, build_ci_fix_prompt}`,
  `session::pool::interaction_appendix`, `SessionPool::quota_routed_provider`,
  `templates::render_command_in`. Test doubles the root crate's tests use
  (`git::MockGitOps`, `agent_provider::test_fakes`, `FakeNotifier`, ...)
  are behind the `test-util` feature, not part of the API.
- **Theme settings are config.** `[tui.theme]` data (`SerializableColor`,
  `ThemePreset`, `ThemeConfig`, `ThemeOverrides`) and `MascotStyle` live in
  `config` (`config::theme`, `config::tui`); the TUI re-exports them and keeps
  the resolved `Theme` and all rendering. Colours are core's own
  `config::theme::ThemeColor`; `tui::theme::to_color` maps them to ratatui.
  Core does not depend on ratatui; `scripts/check-layers.sh` fails on any
  ratatui (or other terminal-UI / frontend crate) reference in core's
  source or manifest.
- **`icons` and `icon_mode` are core**, not presentation:
  `session::types::SessionStatus` resolves its own icon through them, and
  neither touches ratatui.
- `sanitize::screen` moved to `src/tui/screens/sanitize.rs`.
- `gates::runner` gains `impl GateCheck for Arc<T>`, so a frontend can keep
  an `Arc` to a runner it hands over (the orphan rule forbids frontends from
  writing that impl).
- Narrowing `pub` to only the tables above is left to a follow-up (the
  #1305 ADR stub's "interface hardening"); this move keeps every item that
  was already `pub` as `pub`.
