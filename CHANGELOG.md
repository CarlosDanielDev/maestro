# Changelog

All notable changes to Maestro are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> v0.27.0 was reserved during planning and never released. The version
> sequence is v0.25.0 → v0.25.1 → v0.26.0 → v0.28.0 → v0.28.1.

> The `### Migration` group is a project-specific extension covering
> automatic state/config migrations and any user-action steps required on
> upgrade. Empty Migration groups are omitted.

## [Unreleased]

## [0.31.2] - 2026-08-14

> **Poka-yoke pass.** Four destructive TUI keypresses that fired with no
> confirmation are now mistake-proofed.

### Fixed
- **PRD `[R]` reset** now requires a second `R` to confirm before deleting the local PRD file; the footer shows a confirm warning while armed and any other key cancels. Previously a single `R` deleted the file irreversibly.
- **Kill-session confirm** no longer treats `Enter` as "kill" — only `y` confirms, matching the overlay's `[y]`/`[n]` hint. A reflexive `Enter` (the confirm/default key everywhere else) no longer destroys a session.
- **Adapt follow-up** number keys `1-9` now only move the selection; `Enter` is the sole launch key. Previously a stray digit immediately spawned a billable session.
- **Adapt wizard cache-clear** (`x`/`Del`) now requires a second press to confirm before wiping the resumable cache; the keybind hint reflects the armed state.

## [0.31.1] - 2026-08-11

### Security
- Completion-gate `Command` and plugin `run` commands now execute as a parsed argv vector instead of through `sh -c`, closing a CWE-78 local command-injection / RCE vector — a crafted gate or plugin command string could inject a second process via `;`, `|`, `$(...)`, or backticks. Commands are tokenized with `shlex` (quote-aware, so quoted arguments such as `--body 'a b c'` survive intact) and the program is executed directly; shell operators (pipes, redirects, globs, `$VAR`) are treated as literal arguments by design. Supersedes #648.

## [0.31.0] - 2026-08-11

> **Insight static map + role-routed dispatch.** `maestro insight scan` emits a
> versioned maintenance map of the codebase, and L1 sub-agent dispatch now
> routes each role to its bound provider end-to-end.

### Added
- **`maestro insight scan`** — a static maintenance-map extractor that emits a versioned `docs/insight/scan.json`: extractor scaffold, schema, `repo_stats`, and module map (#1009); feature extraction across 4 surfaces plus coverage stats (#1010); and design-system extraction — palette, icons, and the Clawd mascot frames — parsed from real source via `syn`, with infallible extractors (missing or unparseable source yields an empty section, never a panic) (#1011).

### Changed
- L1 sub-agent dispatch now routes each role to its bound provider (`RoleBinding.agent` → `fallback_agent` → factory default), matching the L2 routing shipped in #881. (#897)
- Headless team launch now populates the per-agent provider map on the production L1 dispatch context, so `ProductionSchedulerRunner` fires each role's bound binary instead of always the factory default. The map builder (`build_agent_provider_map`) moved to `commands::agent_provider_map`, shared by the TUI session pool and the headless launch path. (#1000)

### Fixed
- Team Picker now lists `[teams.*]` teams defined in `maestro.toml` — `populate_team_wizard_data` merges inline project-config teams into the loader, which previously showed only built-in and filesystem-tier presets. (#897)
- Interaction sessions now require a second confirm before quitting with unsaved work, so an in-progress turn is not lost to an accidental quit. (#1008)

## [0.30.0] - 2026-06-13

> **Interactive Iteration Sessions.** Interaction is no longer a separate
> concept — it is a regular `Session` that, instead of terminating after its
> one-shot run, transitions to a kept-alive `Interactive` state and accepts
> follow-up turns through the same pipeline, provider routing, and telemetry.
> Multi-provider chat (Claude, Codex, Qwen, OpenCode, Ollama, MiniMax) follows
> from that unification.

### Added
- Interactive iteration sessions — launch an issue in Interaction mode and the session runs its flow, then stays alive as a live chat transcript instead of terminating; follow-up turns resume the same agent conversation. The unification landed across phases #946 (real issue prompt + appendix at launch), #947 (follow-ups reuse the normal resumed-turn pipeline), #948 (kept-alive `Interactive` status), #949 (PR keeps the session alive), and #950 (Interaction screen as a view over the live `Session` + session switcher rejoin).
- Provider-routed interaction turns — interactive follow-ups route through the selected agent and resolve the per-agent model, so chat works with any configured provider, not just Claude (#929).
- Native in-session diff reviewer (gitui-derived) — review the session's changes read-only inside the TUI without dropping to a terminal (#918).
- Interaction transcript redesign — opencode-style bordered message cards plus scroll rework (mouse wheel + PageUp/Down + jump-to-latest) (#987, #988).
- Interaction sessions user guide at `docs/guides/interaction-sessions.md` (launch combos, keymap, lifecycle, troubleshooting) + README feature bullet (#743).
- Launch-option checkboxes (`Produce PR`, `Interaction`) extended to the multi-issue launch overlay and the free-form prompt screen, with the same Tab/Space/Enter keymap and `[behavior.launch]` defaults as the single-issue dialog; `UnifiedSessionConfig`/`PromptSessionConfig` carry the values (#919).
- Structured `[INTERACTION]`/`[TEARDOWN]` activity-log lines pinned in `src/work/activity.rs` + tracing spans (`interaction.launch/turn/terminator/teardown`) around every lifecycle stage (#742).
- Claude interactive (PTY) transport for the 2026-06-15 subscription cutoff (milestone v0.30.5): new `transport = "headless" | "interactive"` field on `[agents.<id>]` (claude-only, TUI Settings row included); the interactive arm drives the real Claude Code REPL on a `portable-pty` pseudo-terminal pinned to a pre-generated `--session-id`, reads structured events by tailing the session transcript JSONL (`transcript_parser.rs`), scrubs every `ANTHROPIC_*` var from the child env so it cannot fall back to API billing, and parks one live child per conversation between turns; spike report at `docs/spikes/2026-05-claude-interactive-transport.md`, migration guide at `docs/guides/claude-transport.md` (#747 #749 #750 #751 #752).
- Startup warning from 2026-05-15 when an enabled claude agent still uses the headless transport — names the agent ids; silence with `MAESTRO_SILENCE_TRANSPORT_WARN=1` (#750).
- `AgentRequest.resume_session_id` + `AgentRunResult.session_id`: conversation resume is now part of the provider contract; headless claude maps it to `--resume <id>` and captures the bound id from the stream, so `InteractionSession::send_turn` is transport-agnostic (#751).
- `src/work/pr_marker.rs` — `PrMarker` struct (`pr_number`, `owner`, `repo`, `issue_number: Option<u64>`, `ts`) + `MarkerError` enum; `write_atomic` (.tmp + rename, no partial-write corruption) + `read` (tolerant of legacy markers that lack `issue_number`, emits `tracing::warn`); `pushup_marker.rs` now delegates to `PrMarker::read` instead of an inline private struct, so the `/pushup` marker schema is the single source of truth. The `/pushup` shell step now writes `"issue_number":%d` into the marker. 6 integration tests in `tests/pr_marker_roundtrip.rs` cover the round-trip, legacy tolerance, and concurrent-write safety (#735).


### Changed
- A `/pushup` PR no longer closes an interactive session (Phase 4 of the unified-interactive-sessions design, #949, spec §4.4): the marker sets `Session.pr_linked` and posts a "PR #N created — session stays open" `System` turn — no wipe, no navigation; mid-stream markers defer the announcement to the turn boundary (#936) so the transcript never interleaves. The destructive worktree wipe (#740, safe-gated) now runs ONLY on explicit quit: `Ctrl+W` confirm interrupts any in-flight turn, terminates the session, wipes off-thread (#941) behind a "wiping worktree…" banner with the input locked, then shows the Terminated banner and auto-navigates back. `#741`'s auto-wipe/auto-nav-on-PR are removed; `#739` is a pr_linked notifier; `InteractionLifecycleEvent` and `CloseReason::PrCreated` are retired (`TransitionReason::PrLinked` stays deserializable for #948-era state files).
- Interactive sessions now stay alive after their flow settles (Phase 3 of the unified-interactive-sessions design, #948): new kept-alive `SessionStatus::Interactive` + `Session.settled_from` (the one-shot outcome: Completed/FailedGates/NeedsPr/Errored) + in-memory `turn_state`. The interception lives in `Session::transition_to` itself — a single choke point covering every terminal-status site, present and future (spec §8). Failure is no longer terminal for chats: settle from `Errored`/`FailedGates` keeps the conversation open for discuss + retry on the same resume id. The parallel `InteractionSession`/`InteractionState` domain machine is retired — the transcript persists on `Session.turns`, the launch flag on `Session.produce_pr`, the deferred `/pushup` terminator on `ManagedSession.queued_terminator` (fires as `Killed`/`PrLinked`; PR-keeps-alive arrives with #949), and the dead never-hydrated `MaestroState.interactions` persistence field is dropped. The Interaction screen keeps a view-local copy of the 3-state enum (`screens/interaction/view_state.rs`) until #950 makes it a view over the live `Session`.
- Interactive turns now run through the normal session pipeline (Phase 2 of the unified-interactive-sessions design, #947): the first turn spawns a real `SessionMode::Interactive` session in the pool and follow-ups resume the bound conversation (`--resume <agent_session_id>`, allowlist-validated), so every turn records the same call-log/cost/token telemetry as a one-shot turn. New `Session.agent_session_id` + `session_bound` lifecycle sentinel capture the provider conversation id for all sessions; the `interaction_turn.rs` bare-spawn loop is removed and `TurnEvent` moved to `session/interaction.rs`. Interactive sessions are exempt from the one-shot completion machinery (gates/auto-PR/teardown, per-turn desktop/Slack noise, #327 PR auto-detect) pending the kept-alive `Interactive` status in Phase 3 (#948).
- Interaction worktree teardown now runs off the UI thread (`spawn_blocking`); the screen shows a "wiping worktree…" banner while git runs and applies the result asynchronously — a wedged git no longer freezes the TUI (#941).

## [0.29.5] - 2026-05-29

Orchestration & provider polish. Unified cost / token / quota observability across all five agent providers (Claude, Codex, OpenCode, MiniMax, Ollama) with a budget pre-spawn gate and provider rollup dashboard. The Teams settings tab is now schema-driven with editable `role_overrides` and `extends` cycle detection. The per-agent call-log viewer gains hook-output events, a live-tail follow toggle, and opt-in disk persistence. Plus team-wizard Launch fixes and a batch of TUI completion / hollow-retry / breadcrumb fixes.

### Added
- Per-provider cost / token / quota emission (milestone v0.29.5 Level 1): the Claude stream-json parser computes `cost_usd` per assistant frame from new `claude_pricing.rs` (Opus / Sonnet / Haiku 4.x; cache-read at 10% of input, cache-write at 125%); Codex's `parse_turn_completed` computes cost from new `codex/pricing.rs` (gpt-5-codex, gpt-5, o3); OpenCode's parser falls back to `opencode/pricing.rs` when telemetry reports `cost: 0`; the shared OpenAI-compatible SSE parser now extracts `prompt_tokens` / `completion_tokens` / `prompt_tokens_details.cached_tokens` from the final frame; Ollama emits a `ContextUpdate` derived from the new `[agents.<id>].num_ctx` config; MiniMax gains a file-locked 5-hour sliding-window quota (`~/.maestro/minimax-quota.json`) with a pre-spawn gate that warns at 80% and refuses at 95% unless the new `--force-quota` CLI flag is set (#771 #772 #773 #774 #775).
- Per-frame token count clamped at `TOKEN_COUNT_CAP = 100_000_000` across all four parsers (Claude, Codex, OpenCode, OpenAI-compatible SSE); values above the cap clamp and emit a new `StreamEvent::Warning { code: "token_count_clamped", message }` variant (#846).
- Ollama `num_ctx` surfaced in TUI Settings — new `FieldSchema` entry (Int 0..1_000_000, step 1024) visible only for Ollama-kind agents; `docs/configuration.md` agents table updated with the `num_ctx` row (#844).
- MiniMax `--force-quota` spawns emit a structured `StreamEvent::Warning { code: "quota_forced" }` event; `QuotaState` bumped to schema v2 (v1 files promoted via read shim, `forced_count = 0`); `MinimaxQuota::record_forced` / `forced_count()` API added; home-screen stats bar shows a "QUOTA: forced N in window" badge when non-zero (#845).
- Budget pre-spawn projection trait + provider rollup view-model (foundation for #776 follow-ups #849/#850): `src/budget/projector.rs` (pure projection fns), `src/budget/quota_snapshot.rs` (`QuotaSnapshot` trait), `src/tui/token_dashboard/provider_rollup.rs` (per-provider rollup view-model); `check_pre_spawn()` + `PreSpawnDecision` added to `src/budget.rs`; `pub limit()` / `used_in_window()` accessors on MiniMax quota (#848).
- Token dashboard now renders per-provider rollup (cost / context / quota) — first user-visible surface of the #776 budget observability umbrella. Foundation from #848; consumed via `provider_rollup::build_provider_rows` (#849).
- Pre-spawn budget gate modal (`[y]es/[n]o/[s]kip` chords) + 80% global budget banner — completes the #776 budget observability umbrella alongside #848 (foundation) and #849 (rollup view).
- `cargo run -- cost` now prints a per-provider rollup section (`=== Per-provider breakdown ===`) grouped by `agent_id` with a `(free)` marker for zero-cost providers (Ollama-local, MiniMax-sponsored), above the per-session list — closes AC #4 of the v0.29.5 observability umbrella (#769).
- Token dashboard Quota column is live for MiniMax sessions: the dashboard command opens `~/.maestro/minimax-quota.json` best-effort and wires `Arc<MinimaxQuota>` through new `App.minimax_quota` + `App::with_minimax_quota` to `draw_token_dashboard_with_quota`, replacing the `NoQuotaSnapshots` placeholder from #850 (#769).
- v0.29.5 observability umbrella closed: unified cost / token / quota tracking across all five agent providers (Claude, Codex, OpenCode, MiniMax, Ollama) — verifies the work landed across #770–#776 and the Level 1.5/2 hardening (#844, #845, #846, #848, #849, #850) (#769).
- feat(tui/settings): wire Teams tab through schema renderer with bindings round-trip adapter (#803).
- feat(tui/settings/teams): `role_overrides` schema slot — `ROLE_OVERRIDE_FIELDS` (5 fields: agent, mode, model_override, prompt_addendum, fallback_agent) added as 5th slot in `TEAMS_ENTRY_FIELDS`; empty maps no longer emit a bare `[teams.<id>.role_overrides]` header on save; existing on-disk `role_overrides` survive a save cycle via `EntryState.passthrough`; full editor deferred to a follow-up issue (#872).
- feat(tui/settings/teams): `role_overrides` is now fully editable in the Teams tab — `FieldKind::Map { entry_fields }` on an entry field now builds a real nested `DynamicMapWidget` (replacing the prior read-only `TextInput` placeholder); `[a]` opens an "Add role" modal scoped to the inner editor, `[d]` removes the active role, and `Down/Up/Tab/BackTab` cooperatively walk inner focus via the existing `try_focus_next`/`try_focus_prev` seam. Inner widget owns its own writeback through `serialize_to_toml`; `EntryState.passthrough` remains as defense-in-depth for `FlattenedMap`/`VecOfStruct` entry fields whose editors are not yet lifted. `display_name_for` now maps `*role_overrides → "role"` so the modal title reads "Add role". Empty nested DynamicMap omits the key entirely from the writeback so no bare `role_overrides = {}` header lands on disk (#901).
- `src/orchestration/team_role_overrides.rs` — soft cross-table validator: `RoleOverrideField` enum (Agent / Mode / FallbackAgent), `RoleOverrideWarning` struct, `validate_role_overrides` checks each team's `role_overrides` map against the live agent and mode sets; empty or whitespace values are treated as "inherit" (no warning); wired into `settings/mod.rs::save_config`; Save proceeds on soft warnings and the banner title appends the first structured path on the next render (#908).
- `src/tui/screens/settings/schema_tab/widgets/dynamic_map_breadcrumb.rs` — renders a `teams.<id> → role_overrides → <role>` breadcrumb in the nested editor's header when focused; per-crumb truncation via `truncate_with_ellipsis` (≤ 3 labels skips middle-elision); wired into `dynamic_map_draw.rs::draw` (#908).
- feat(tui/settings/teams): inline `ValidationFeedback::warning` next to offending `role_overrides.<role>.<field>` TextInputs in the nested editor; reuses the existing TextInput validation slot, no theme/keybind changes; warning text sanitised via `sanitize_for_terminal` before render (#909). The inline message now enumerates the configured `[agents.*]` / `[modes.*]` ids (capped at 6 with `(+N more)` overflow; empty set falls back to `no agents configured` / `no modes configured`) so users can pick a valid value without leaving Settings (#912 folded into the same PR).
- `FieldKind::FlattenedMap` and `FieldKind::VecOfStruct` entry-field placeholders in `entry_state.rs` lifted to live widgets: `FlattenedMap` → `WidgetKind::DynamicMap`; `VecOfStruct` → `WidgetKind::DynamicRows`; `EntryState.passthrough` field deleted; `to_toml_filtered` empty-omission rule extended to cover all three dynamic kinds (#908).
- Team Wizard Launch: `Ctrl+V` paste on IssuePicker step — new `tui::screens::team_wizard::clipboard` module + `parse_pasted_issue_token` parser; accepts bare numbers, `#NNN`, and GitHub issue URLs (#875).
- Team Wizard Launch: autocomplete from `issue_metas` on IssuePicker step — new `LaunchPayload.autocomplete_focus` field; new snapshot `launch_issue_picker_autocomplete_80x24` (#876).
- Per-agent call-log viewer: `[L]` in the Detail screen opens a new `TuiMode::CallLog(Uuid)` view backed by `src/tui/call_log/` (`mod.rs` formatters, `state.rs` scroll state, `draw.rs` renderer); `Session.call_log` (capped at 500 entries via `append_call_log`) persists `CallLogEntry` records with `render_event_payload` 10 KB per-field cap; 5 insta snapshots added (#868).
- `extends` cycle detection in Teams cross-entry validation: `validate_extends` (`src/orchestration/team.rs`) promoted from a shallow dangling-parent check to full colored-DFS cycle detection (self-loop, multi-hop); the reported path is rotated to its lexicographically smallest member so the error is deterministic (``teams `a → b → a` form a cycle``). Surfaces at both `Config::load` (boot) and the in-TUI Settings save banner through the same `Result` (#873).
- `StreamEvent::HookResponse { hook_name, exit_code, stdout, stderr }` + `CallLogKind::HookResponse`: hook subprocess output now appears in the per-agent call log alongside tool calls and assistant turns. `PluginResult` carries `exit_code`/`stdout`/`stderr`; `fire_plugin_hook` routes a `HookResponse` through the session pipeline for the owning session; `stdout`/`stderr` capped at 10 KB before persistence. The `tests_passed` / `tests_failed` / `pr_created` hooks resolve the owning session via `Pool::find_by_issue_mut` so their output also lands in the call log (#887, #914).
- Call-log live-tail follow mode: `[f]` in the Call Log pane toggles follow-tail, which auto-advances the cursor to the newest entry as the log grows (footer shows `[f] Follow: ON/off`); any manual move (`j`/`k`/`↑`/`↓`/`g`/`G`) cancels it. `reconcile_follow_tail` runs once per event-loop tick so the renderer stays pure (#886).
- `[sessions] call_log_persist` config flag (default `false`, memory-only): gates whether each session's `call_log` is serialized to `maestro-state.json`. Enforced in code at the single `StateStore::save` boundary (clone-and-clear when off; the live pool keeps the log for the viewer). The call log captures assistant text, Thinking blocks, errors, bash commands, and hook stdout/stderr — all sensitive — so persistence is opt-in (#888).

### Fixed
- Team Wizard Launch: added `IssuePicker` step (Step 2/8) so users can enter an issue number when selecting the Issue input kind; also adds `manual_issue_input` to `LaunchPayload` and renames the step-total guard to `launch_step_total_is_eight` (#805).
- Team Wizard agent picker: `populate_team_wizard_data` now excludes `enabled = false` agents; new `build_known_agents_from_config` pure helper extracts the filter logic (#806).
- Team Wizard Launch: `ScreenAction::LaunchTeam` variant added to `screen_dispatch`; dispatcher fan-out unblocks Wizard Step 6/8 (real `SessionManager::run_team` wiring deferred to follow-up per architect blueprint) (#877).
- Session manager: `files_touched` counter now case-insensitive — opencode emits lowercase tool names (`read`, `edit`, `write`, `glob`, `grep`, `patch`, `multiedit`) and was being dropped by the prior PascalCase filter. New `is_file_touching_tool` helper unifies Claude (PascalCase) and opencode (lowercase) conventions; Agent Graph "Waiting for first file edit" + Overview "0 files" counters now tick for opencode sessions (surfaced during bundle PR #880 manual QA).
- OpenCode pricing fallback now prices cache-read and cache-write tokens (previously ignored) and adds a DeepSeek arm covering `opencode-go/deepseek-v4-pro` and the rest of the deepseek-chat / v3 / v4 family at the published DeepSeek public rates ($0.27 / $1.10 / $0.07 / $0.27 USD per million tokens). Cache-heavy sessions (e.g. 99.9% cache-hit on a long-running deepseek run) no longer report `$0.00` in the token dashboard. Refactored `opencode/pricing.rs` to use the same `ModelPrice { input, output, cache_read, cache_write }` shape `claude_pricing.rs` uses (surfaced during bundle PR #880 manual QA).
- Team Wizard Launch: Esc on `LaunchSuccess` / `LaunchFailed` now pops the wizard (closes back to Welcome) instead of retreating to `Executing`. The previous behavior combined with the Esc-on-Executing swallow stranded the wizard at Step 6/8 with no forward path. Added a `launch_team_action_transitions_wizard_to_launch_success` integration test in `input_handler.rs` that exercises the full `handle_screen_action(LaunchTeam)` → `apply_launch_result(Ok)` → `LaunchSuccess` path to lock the state-machine contract (surfaced during bundle PR #880 manual QA).
- Team Wizard Launch: `issue_metas` cache now eagerly warms on wizard entry — pushing `TeamWizard` (via Landing menu or `PushTeamWizard`) queues a `TuiCommand::FetchIssues`, and the data-handler **replaces** the wizard's cache on every Issues fetch (treats the fetch as authoritative so re-entering the wizard gets a freshly-warmed cache instead of stale numbers). The #876 autocomplete suggestions now render on the first visit; the prior undocumented "open Issue Browser first" prereq is gone. An activity-log entry (`"Issue cache warmed: N issue(s) available for autocomplete"`) is emitted on each warm so the user can verify the fetch landed. Three new data-handler tests cover the warm path, the replace-on-fetch semantics, and the activity-log emission (surfaced during bundle PR #880 manual QA via screen-recording analysis).
- Settings tabs: empty `ListEditor` (every `FieldKind::StringList` field — `min_agents`, `bindings`, `allowed_tools`, etc.) now renders a discoverable empty-state. Unfocused empty list shows `(empty)` in `theme.text_muted` below the label; focused empty list shows `(empty — [a] Add to start)` in `theme.accent_info` italic, replacing the prior near-invisible muted `[a] Add [d] Delete` hint that surfaced in Settings → Teams freshly-added entries (`bindings:` field looked broken — see PR #899 manual QA screen recording). Non-empty path unchanged. Four new unit tests in `src/tui/widgets/list_editor.rs` lock the four state combinations (#900).
- Completion-summary recovery `[q]` now returns to the prior mode instead of forcing Overview: the failed-gates recovery modal's `[q]` handler called `navigate_back_or_dashboard()` instead of hardcoding `TuiMode::Overview`, so dismissing a completion modal opened from a Detail view returns to Detail (restoring its `[l]`/`[L]`/`[k]` chord set) rather than dropping to Overview; falls back to Dashboard on an empty stack, never ConfirmExit (#894).
- Retrying session no longer becomes an unkillable zombie: a session finalized while still `Retrying` (its retry spawned as a separate session) lingered in the finished list with a non-terminal `Retrying` status, rendering as a live "RETRYING …" row forever that the active-only kill path reported as "Session not found or already finished". `SessionPool::finalize` now advances a `Retrying` session to a terminal `Killed` status before moving it to finished.

### Changed
- Interaction transcript redesigned as opencode-style bordered cards (`src/tui/screens/interaction/history.rs`): each turn now renders inside a rounded box — header `╭─ role · HH:MM ───╮` colored by role (user=`accent_info`, agent=`accent_success`, system=`text_secondary`), body via `render_markdown` with syntect code highlighting, footer `╰───╯`; streaming turns show a `…` marker in the header and no footer. Long body lines are truncated to inner card width. New helpers: `role_word`, `truncate_spans`, `box_body_line`, `header_line`, `footer_line`; `build_lines` now takes a `width` param; `role_prefix` removed. 7 interaction snapshots regenerated (#987).
- `src/tui/screens/settings/schema_tab/widgets/dynamic_map.rs` (721 LOC) split into a `dynamic_map/` directory: `mod.rs` (struct + handle_input + accessors + draw delegate, 300 LOC), `visibility.rs` (kind-aware field visibility helpers, 71 LOC), `focus.rs` (focus walks + cooperative inner delegation, 157 LOC), `modals.rs` (Add/Remove modal lifecycle + tab nav, 126 LOC), `sizing.rs` (desired_height + row-height table, 113 LOC); old `dynamic_map.rs` deleted and its allowlist entry removed (#908).
- Schema validators on `ROLE_OVERRIDE_FIELDS` remain `None`; the cross-table check lives at the typed-Config layer (next to `validate_extends`), mirroring the approach taken in #803 (#908).
- Team Wizard Launch dispatch now walks the scheduler's level DAG via the new `session::team_runner::run_team` instead of flattening every level into a single `LaunchSessions` push (which dropped `max_parallel` and bypassed per-team agent binding). The dispatcher queues a new `TuiCommand::RunTeam` carrying the built `Scheduler` and the app-default `agent_id`; the command pump spawns the runner which acquires permits from a `Semaphore::new(max_parallel)` per level, fans each level out via a `TeamLauncher` trait, and short-circuits on intra-level failure (downstream levels are recorded as `skipped_due_to_upstream`). Per-issue L2 sessions now carry the team's Implementer-binding `agent_id` (falling back to `fallback_agent`, then to `app.selected_agent_id()`); the placeholder `tracing::warn!("LaunchTeam fanned out via LaunchSession path …")` line is gone. Wizard advancement to `LaunchSuccess`/`LaunchFailed` now waits for `TuiDataEvent::TeamLaunchResult(Result<(), String>)` posted by the runner; on failure the wizard's banner names the first failing issue (`"Issue #N failed: <reason>"`). New module `src/session/team_runner.rs` (TeamLauncher trait, run_team fn, TeamOutcome) + TUI bridge `src/tui/team_runner_glue.rs::RealTeamLauncher`; tests at `src/session/team_runner.rs` cover level-drain ordering, partial-failure short-circuit + naming, downstream skip, and semaphore-capped concurrency. True L1 sub-agent multi-provider routing (Implementer vs. Reviewer vs. Docs picking different binaries inside one issue) remains a follow-up; the L2 session itself carries one agent_id (#881).

### CI
- `deny.toml`: name-only skip for the `shlex` duplicate that surfaces only on Linux/musl CI dependency resolution (the committed macOS lockfile dedupes to one; a fresh Linux resolve pulls a second 1.x via `cc` build-deps). Both versions are advisory-clean. Matches the existing `windows-targets` / `wit-bindgen` Linux-CI skips.

## [0.29.0] - 2026-05-21

Schema-driven settings — the entire Settings screen is now rendered from a single declarative schema (`FieldSchema` + `FieldKind`) instead of hand-coded per-tab modules. `[agents]`, `[modes]`, and `[[sessions.completion_gates.commands]]` gain in-TUI add/remove/reorder via the new `DynamicMapWidget` and `DynamicRowsWidget` primitives. `maestro.toml` saves preserve comments, blank lines, and key order via `toml_edit`. `docs/configuration.md` is now auto-generated from the schema and guarded by a CI drift check. Slash-command guardrails (`/auto`, `/implement`, `/pushup`) gain programmatic gates that survive context resets.

### Added
- Schema-driven settings renderer behind a feature flag — one declarative `FieldSchema` + `FieldKind::{Bool, Int, Float, String, Enum, List, Map, VecOfStruct, FlattenedMap, NestedTable}` registry now drives every Settings tab, replacing per-tab `tabs/*.rs` modules with one renderer (#713, #714).
- `FieldKind::FlattenedMap` schema variant — entries live at the parent table level directly; powers new `AGENTS_TABLE` and `MODES_TABLE` schemas. New `SettingsTab::Agents` (index 7) and `SettingsTab::Modes` (index 8) variants wire `[agents]` and `[modes]` through the schema renderer; Theme/Layout/Flags/TurboQuant/Advanced indices shifted by 2. `[sessions.completion_gates]` is now a `NestedTable` wrapper exposing `enabled` and a `commands` row table (`VecOfStruct` of `[[sessions.completion_gates.commands]]`) inside the Sessions tab. `save_config` calls `agents.validate()` before writing — a missing `agents.default` target surfaces as a Save banner error. `DynamicMapWidget::with_clock` filters non-table scalar siblings (e.g. `agents.default`) to prevent mis-parsing. `truncated_titles` helper trims the sub-tab strip when 12+ entries are present. New files: `src/config/schema/dynamic.rs`, `src/tui/screens/settings/tabs/agents.rs`, `src/tui/screens/settings/tabs/modes.rs`, `src/tui/screens/settings/tests/agents_tab.rs`. Note: `[teams.<id>]` wiring is deferred to #803 (milestone v0.29.5) due to `TeamConfig.bindings: #[serde(flatten)]` requiring a sync-time adapter (#792).
- `DynamicMapWidget` and `DynamicRowsWidget` — two new TUI widget primitives for editing key-value maps and ordered lists inside the schema-driven settings renderer; both support Add/Remove modals, 5-second undo (`UndoBuffer`), and Alt+↑/↓ row reorder (#791).
- `FieldKind::Map` and `FieldKind::VecOfStruct` schema variants + `Presentation` hint — schema vocabulary for dynamic-cardinality sections, consumed by the new widget primitives (#789).
- Dynamic-section design lock (`docs/superpowers/specs/2026-05-19-dynamic-config-editing.md`) — drives the v0.29.0 implementation of Agents / Modes / completion-gates editing in the TUI (#719).
- `maestro.toml` saves now preserve comments, blank lines, key order, and any sections not modeled by the `Config` struct — powered by a `toml_edit` overlay merge in the new `src/config/overlay.rs` module (#712).
- `toml_edit` comment-preserving round-trip extended to dynamic sections (`[agents.*]`, `[modes.*]`, `[[sessions.completion_gates.commands]]`) so add/remove/reorder operations don't strip comments from the user's `maestro.toml` (#790).
- Schema-driven autogen for `docs/configuration.md`: 10 AUTOGEN marker blocks are now auto-populated from the config schema registry via `src/config/schema/docs_render.rs`; a drift guard (`src/integration_tests/docs_gen.rs` / `docs_gen_no_drift`) runs in CI under the new `docs-gen` job, and `scripts/regenerate-docs.sh` provides a convenience wrapper for local regeneration (#717).
- Auto-generated `docs/configuration.md` sections for dynamic config tables — `[agents.<id>]`, `[modes.<id>]`, and `[[sessions.completion_gates.commands]]` blocks are rendered from the same schema as the TUI, so docs and TUI never drift (#793).
- Float-precision support in the schema renderer — `FieldKind::Float` gained a `display_scale: u32` field, `NumberStepper` gained `display_divisor` + `display_value()`, unblocking the Budget tab's migration from the legacy `×10` integer storage trick to true floating-point display (#785).
- Agents tab entry fields grouped into Common / Subprocess / HTTP sections with kind-aware collapse — non-matching subprocess/HTTP fields are hidden when the agent kind is the other type, so only relevant fields render (#811).
- Design redesign of Agents/Modes/completion-gates UX (umbrella spike #807) — locked design + manual QA matrix that #808/#809/#810/#811 implement against, gating #792's final wire-through (#807).
- Programmatic guards for `/auto`, `/implement`, `/pushup` slash commands — DOR, blockers, contracts, RED/GREEN gates, and milestone-graph maintenance are now enforced via the `.maestro/hooks/implement-gates.sh` bash gate + `subagent-gatekeeper` JSON report, so behavior survives context resets across long sessions (#813).
- Spike report on schema-driven TOML editor TUI feasibility (#711) — produced the go/no-go that informed the FieldKind/FieldSchema shape adopted by #713 (#711).

### Fixed
- Settings schema-tab modals (Add/Remove) now use the standard double-bordered themed block instead of a green-filled popup (#808).
- `DynamicMapWidget` and `DynamicRowsWidget` no longer wrap themselves in a bordered `Block`; they now render a flat `<label>:` header (styled with `theme.text_secondary` + BOLD) followed by the sub-tab strip or table. `field_height` in `draw.rs` reduced by 1 row per widget kind (DynamicMap: +3 with `.max(7)`; DynamicRows: 8) to reclaim the removed border row. `DynamicRowsWidget::insert_row` also mirrors the Add-modal identifier into the first String column (so a typed `fmt` shows up as the row name instead of vanishing), and the focused-row highlight switches from `theme.accent_info` to the standard `theme.selection_fg/bg` REVERSED used by the rest of the settings rows (#809).
- `DynamicMapWidget` entry-switching rebound from `Ctrl+←/→` (which collides with terminal-level word-jump chords) to the dedicated `[` and `]` keys, matching the rest of the settings keybind vocabulary (#810).

### Changed
- `config`: TOML migrations now preserve comments, blank lines, and key order via `toml_edit`. The internal helper module exposes a shared `ensure_field` primitive for future single-key migrations (#718).
- `config/schema`: `FieldKind` and `DefaultValue` enums are now marked `#[non_exhaustive]`, preventing external crates and future match arms from silently exhausting the variants. Marker-only refactor; no behavior change (#796).
- Budget tab in TUI settings is now rendered by the schema-driven renderer (same path as all other tabs except Flags and Theme/Advanced multi-table). `per_session_usd` and `total_usd` display correctly as floating-point values (`5.5`, `12.5`) — the legacy `×10` integer storage trick is gone. `tabs/budget.rs` collapsed from 27 LOC to 7 LOC. Covered by 8 new parity tests in `src/tui/snapshot_tests/settings_budget_parity.rs` (#785).
- Project tab in TUI settings migrated to the schema renderer as the first proving-ground migration (#715).
- Remaining Settings tabs (excluding Budget — see #785) migrated to the schema renderer; legacy `tabs/*.rs` modules collapsed into one-liner registrations against the schema registry (#716).
- `agents_graph` migration (originally #710) rewired on top of `toml_edit` + the new schema rather than the legacy custom serializer, eliminating an entire class of comment-strip bugs (#718).
- Agents/Modes/Teams/completion_gates.commands wired through the schema renderer end-to-end — Settings now manages every dynamic section via the same widget pipeline as scalar fields (#792).

## [0.28.1] - 2026-05-18

Documentation refresh + hook/skill hygiene milestone. User-facing docs audited end-to-end against the v0.28.0 surface, hook scripts relocated to the agent-agnostic `.maestro/hooks/` path, and the legacy Claude-Code-only `/simplify` skill retired in favor of the canonical `maestro` command.

### Added
- `docs/teams-cookbook.md` — task-oriented cookbook for team workflows (#675).
- Consolidated CHANGELOG entries for v0.25.0 / v0.25.1 / v0.26.0 / v0.28.0 and rewrote GitHub release bodies in user-facing tone (#677).
- `tests/legacy_skills_removed.rs` regression guards preventing the legacy `/simplify` skill from reappearing (#760).

### Changed
- README and ROADMAP audited and refreshed against the v0.28.0 surface (#672).
- GitHub Wiki content audited for accuracy and currency (#673).
- `docs/configuration.md` reference rewritten; CLI `--help` text audited against documented options (#674).
- Agent documentation verified end-to-end against actual agent definitions (#676).
- Hook scripts relocated from `.claude/hooks/` to `.maestro/hooks/` to reflect their agent-agnostic role. The `.claude/hooks/` path is preserved as a relative symlink to `.maestro/hooks/` for one release (#759).

### Deprecated
- `.claude/hooks/` directory path. The symlink will be removed in the next minor release. Update any external tooling, dotfiles, or local scripts to reference `.maestro/hooks/` directly. Affected scripts: `implement-gates.sh`, `preflight.sh`, `notify.sh`, `notify.ps1`, `sentinel-path.sh`, `parse_gatekeeper_report.py`, `parse_idea_triager_report.py` (#759).

### Removed
- Deprecated Claude-Code-only `/simplify` skill. The canonical source is `.maestro/templates/commands/simplify.md`, rendered to `.claude/commands/simplify.md` and other provider targets via `maestro sync-templates` (#760).

## [0.28.0] - 2026-05-16

Templates layer — slash commands, agent rules, and subagent lists now render from a single canonical source under `.maestro/templates/`, with drift detection in CI and a `maestro sync-templates` command to regenerate everything in one shot.

### Added
- `maestro sync-templates` CLI subcommand with drift detection — regenerate all provider artifacts in one shot and fail CI on drift (#706).
- HTTP-provider runtime template injection — canonical commands rendered into HTTP-generic providers at runtime (#707).
- HTTP-generic provider rendering rules — link-free subagent tables, inline sub-task headers (#705).
- Codex provider rendering rules — `target_dir()=None`, inline sub-task headers, provider-neutral hook paths (#704).
- Claude provider rendering rules with a byte-identical regression test suite (#703).
- Canonical command specs for `/implement`, `/pushup`, `/plan-feature`, `/simplify` under `.maestro/templates/commands/` (#702).
- Render engine and `AgentProvider::template_rules()` trait extension (#701).
- Canonical templates layer scaffolded at `.maestro/templates/` — core fragments (premises, tdd-cycle, dependency-graph), manifest skeleton, README (#700).
- `maestro init` now scaffolds the same `.maestro/templates/` reference tree into newly initialized projects (#708).

### Changed
- `subagent_list` placeholder now derives from the filesystem manifest; hand-rolled constant retired (#728).
- Sandbox-aware include reader extracted for provider rule reuse (#727).
- `regen_claude_baselines` test relocated to an example/xtask binary so `cargo test` no longer regenerates artifacts as a side-effect (#729).

### Migration
- New Cargo dependency: `reqwest = "0.12"` (default features off, `rustls-tls`) — pulled in by the HTTP-provider runtime. Relevant for restricted-network and air-gapped builds.

## [0.26.0] - 2026-05-12

Team orchestration — define a team of agents, hand them a milestone, and maestro coordinates the work. The orchestration layer schedules issues by dependency level, dispatches them to the right agent, and surfaces progress in the TUI graph view.

### Added
- Multi-agent orchestration foundation: L1 subagent dispatch + cost estimate, L2 per-issue orchestrator state machines, L3 cross-issue scheduler, team wizard TUI, and `maestro team {list,new,launch,manage,explain}` CLI subcommands with built-in preset docs under `docs/teams/` (#660, #661, #662, #663, #664, #665).
- `views.agent_graph_enabled = true` by default, with a TUI graph view of running sessions and an animated empty state when no sessions are active (#710, #692).
- CI error review-and-fix popup in the TUI (#695).

### Fixed
- Agent Graph no longer freezes after session completion (#694).

### Migration
- State-store schema bumped from version 0 to 1. Legacy state files migrate automatically on first load — run `maestro` once after upgrade; no manual action required.
- `agent_graph_enabled` is backfilled into older `maestro.toml` files on startup. The default value preserves prior behavior for users who never set the key.
- New Cargo dependency: `directories = "5"` — used to resolve user-config paths under the orchestration foundation. Relevant for restricted-network and air-gapped builds.

## [0.25.1] - 2026-05-06

Reliability patch.

### Added
- `cargo-dupes` code-duplication gate added to the CI pipeline (#620).

### Fixed
- Release script resilient to protected-branch release-PR edge cases (#685).

## [0.25.0] - 2026-05-06

Multi-agent foundation — maestro now drives Claude, Codex, Qwen, Ollama, Minimax, OpenCode, and any OpenAI-compatible endpoint behind a single workflow, with a doctor command to validate your setup and a per-session agent selector in the TUI.

### Added
- `AgentProvider` trait — pluggable interface for any CLI-based or HTTP-based agent backend (#547).
- New providers: Qwen Code (#548), Codex CLI (#589), Ollama for local cheap-model sessions (#612), Minimax for cloud cheap-model sessions (#616), OpenCode for 75+ AI backend sessions behind one provider (#617).
- `OpenAiCompatibleSseParser` extracted as a Level-1 building block for OpenAI-compatible streams (#652).
- Parser adapters for differing agent output formats (#552).
- Per-session agent selector in the TUI (#551).
- Multi-agent validation in `maestro doctor` (#550).
- Extended configuration schema for multi-agent setups (#549).

### Documentation
- Multi-agent setup and usage guide (#553).

## [0.24.1] - 2026-05-05

### Documentation
- docs: replace distorted README hero with snapshot-based render (#653)

## [0.24.0] - 2026-05-05

### Added
- feat(tui): rich EmptyState widget with action-oriented hints across all screens (#639)
- feat(tui): add shared BrailleSpinner widget for async loading states (#637)
- feat(tui): extract shared WizardFrame widget for design-system consistency (#636)
- feat(config): flip experimental.azure_devops default ON and retire the startup gate (#471)
- feat(tui): wizard label polymorphism — Iteration vs Milestone based on provider (#470)
- feat(adapt): iteration-path vs milestone branching in materializer for AzDO (#469)
- feat(init): interactive maestro init with provider auto-detection and AzDO prompts (#468)

### Fixed
- fix(roadmap): keep selected milestone in view when expanding/collapsing details (#638)
### Added
- feat(config): mark Azure DevOps stable by default and retire the `experimental.azure_devops` startup gate (#471)
- feat(init): add interactive provider detection with Azure DevOps prompts and `--non-interactive` GitHub defaults (#468)

## [0.23.0] - 2026-05-04

### Added
- chore(workflow): mechanize /pushup commit + PR scaffolding (#556)
- chore(workflow): DOR linter + condensed issue summary in pre-check hook (#555)
- chore(workflow): scripted milestone dependency-graph update for /pushup (#554)
- feat(azdo): implement CI status and merge operations via az pipelines and az repos pr (#466)
- feat(azdo): implement list_open_prs, get_pr, and submit_pr_review via az repos pr (#465)
- feat(azdo): implement list_labels and create_label via work-item tags (#464)
- feat(azdo): map create_milestone and list_milestones to AzDO Iterations (#463)
- feat(azdo): implement create_issue via az boards work-item create (#462)

### Fixed
- fix(azdo): port proactive duplicate prevention from GitHub client (#467)
### Added
- chore(workflow): add scripted milestone dependency-graph updater for `/pushup` (#554)
- chore(workflow): add `/implement` DOR lint fast path and condensed issue summaries (#555)
- chore(workflow): add `/pushup` commit and PR draft helpers (#556)

## [0.22.0] - 2026-05-04

### Added
- feat(doctor): provider-aware validate_provider_setup with AzDO checks (#461)
- feat(provider): extend RepoProvider trait with CI and Merge surfaces (#460)
- refactor(provider): wire create_provider() at all call sites; remove hardcoded GhCliClient::new() (#459)
- refactor(provider): rename GitHubClient trait to RepoProvider; neutralize shared types (#458)

## [0.21.0] - 2026-05-04

### Chore
- test: split app/tests.rs into co-located module test blocks (#405)
- test: add property-based tests for session/parser.rs (#406)

## [0.20.0] - 2026-05-04

### Added
- fix: gate Azure DevOps provider behind experimental.azure_devops flag (#404)
- feat: wire modes resolver into session spawn path (#402)

### Chore
- tech-debt: resolve TurboQuant CompressedContext.vectors dead code (#403)

## [0.19.0] - 2026-05-04

### Chore
- tech-debt: split prompt_input.rs into draw/types sub-modules (#401)
- tech-debt: split config.rs into domain sub-modules (#400)
- tech-debt: extract HTTP transport layer from provider/github/client.rs (#399)
- tech-debt: split settings/mod.rs into per-tab sub-modules (#398)

## [0.18.0] - 2026-05-03

### Changed

- tech-debt: extract `ScreenState` and `SessionConfig` sub-structs from `App` (#395)
- tech-debt: replace `unwrap()` with error propagation in `session/manager.rs` (#396)
- tech-debt: replace `unwrap()` with error propagation in `work/executor.rs` and `state/store.rs` (#397)
- chore: tech debt catalog superseded by broken-out items in v0.18.0–v0.21.0 (#407)

## [0.17.1] - 2026-05-03

### Added

- feat(github): thread configured `project.repo` into production `GhCliClient` shellouts via `--repo`, with validation and bare-client fallback when config is unavailable or invalid (#565)
- feat(tui): redesign welcome page with symmetric LazyVim-style layout (#582)

### Fixed

- fix(security): sanitize Slack and PowerShell injection in `notify.sh` (#583)
- fix(security): sanitize Linux notify-send markup and option-flag injection (#586)

### Changed

- refactor(security): replace regex JSON parsing in `notify.sh` with `jq` (#587)

### Documentation

- docs(security): warn about `maestro.toml` supply-chain risk in `SECURITY.md` (#584)

## [0.17.0] - 2026-05-02

### Added

- spike: explore graph visualization of active agents in the ratatui canvas; established the design space and selected the canvas-based approach (#513)
- feat(settings): `views.agent_graph_enabled` toggle in `maestro.toml` gates the agent-graph view at startup (#525)
- feat(tui): productionize agent-graph layout — ConcentricLayout with aspect-ratio normalization and phase correction for deterministic, non-overlapping agent positioning (#526)
- feat(tui): agent-graph integrated into the session render dispatcher behind the `views.agent_graph_enabled` settings flag (#527)
- feat(tui): `[g]` keybinding toggles between agent-graph and panel view at runtime (#528)
- feat(tui): loading and activity pulse animations on graph nodes and edges during active subagent dispatches (#529)
- spike: agent personalities — role taxonomy (Orchestrator, Reviewer, Docs), ASCII/nerd-font sprite designs, and visual-identity guidelines; documented in `docs/adr/002-agent-personalities.md` (#536)
- feat(session): `Role` enum with `derive_role` classifier and `Session::role` field; maps session subagent type to Orchestrator / Reviewer / Docs identity (#538)
- feat(tui): role-appropriate ASCII/nerd-font personality sprites rendered on agent nodes in the agent-graph view (#539)
- feat(tui): role-colored chip on subagent dispatch entries in the activity log — Orchestrator (blue), Reviewer (yellow), Docs (green) (#543)

### Fixed

- fix(tui): agent-graph sprite rows now render contiguously on every viewport from 80×24 up to 200×60 (#576)
  - Pre-fix `draw_sprite_on_canvas` hard-coded `ROW_STEP = 0.1` and `X_OFFSET = -0.078` in canvas
    units. Both values were calibrated to the 80×24 floor; on larger viewports the canvas-cell
    height shrinks (≈ 0.034 at 60 inner rows) but the constants stayed fixed, so consecutive
    sprite rows landed on non-adjacent buffer rows. The 6-row sprite read as 4 visually
    disconnected chunks (hat / body / torso / legs) and occupied ≈ 60% of canvas height on a
    typical session-window viewport.
  - `draw_sprite_on_canvas` now takes `inner_cols` and `inner_rows` and derives `row_step` and
    `x_offset` from ratatui's canvas-to-cell mapping (`2.0 / (inner_rows - 1)` and
    `-2.5 * 2.0 / (inner_cols - 1)`). Each sprite row maps 1:1 to a terminal row; the 6-cell
    row stays centered on `cx` within ±1 cell across viewports.
  - `LABEL_RADIUS_SPRITE = 0.40` was replaced with `(2.5 + 1.5) * cell_h` computed at the call
    site so the agent's `#NNN` label sits a constant one to two cells outside the sprite top
    regardless of viewport size; the old constant was correct only at 80×24.
  - Three new size-pinned snapshot tests (`agent_sprite_contiguous_at_80x24`,
    `agent_sprite_contiguous_at_120x40`, `agent_sprite_contiguous_at_200x60`) lock the visual
    on the smallest, midsize, and large supported viewports. Three behavioral assertions
    (`sprite_no_gap_between_rows_at_*`, `sprite_height_within_30_percent_of_inner_rows_at_120x40`,
    `sprite_horizontally_centered_within_1_cell_at_*`) verify the geometry programmatically.
  - All existing nerd-font sprite snapshots were re-baselined: the sprite is now compact
    (6 cells) instead of stretched (≈ 18 cells on large viewports) and the label sits closer
    to the sprite on midsize and large viewports.
  - Decision documented in `docs/adr/002-agent-personalities.md` § Addendum (#576).

- fix(tui): agent-graph file marker rectangle removed; label now anchors at edge endpoint (p.y) eliminating the one-cell vertical gap between edge and label (#569)
  - Pre-fix a 0.04×0.04 `Rectangle` marker sat at the file node position and the label was
    printed 0.08 canvas units below — roughly one terminal row — leaving a visible gap between
    the incoming edge and the label, making the label look orphaned from its edge in both Block
    and Braille rendering modes.
  - Fix: the marker rectangle is removed entirely and `ctx.print(lx, p.y, …)` anchors the label
    exactly at the edge endpoint. The label's leading glyph now serves as the visual node; the
    edge terminates inside it and the gap is impossible by construction.
  - The marker carried no semantic information (no status, no animation, no role color), so its
    removal loses nothing. Agent nodes are unchanged — their sprite/rectangle encodes role and
    status that a text label alone cannot. If a future feature needs a per-file status icon,
    the recommended pattern is to prepend a glyph character to the label (e.g. `⚠ main.rs`).
  - Snapshot baselines refreshed across all `agent_graph_*` tests. New regression snapshot
    `file_node_marker_and_label_visually_connected` pins the 1-agent+3-files case.
  - Decision documented in `docs/adr/001-agent-graph-viz.md` § Addendum (#569).

- fix(tui): agent-graph file labels now grow outward from the marker and truncate with an ellipsis when they would overshoot the canvas (#568)
  - Pre-fix `ctx.print(p.x, p.y - 0.08, …)` always anchored the label's leftmost cell at the
    marker, so files on the right half of the ring overflowed off the right border (truncating
    or disappearing) and files on the left half extended rightward into the graph interior,
    overlapping edges and the agent sprite.
  - New `place_file_label(p, label, inner_cols)` helper in `src/tui/agent_graph/label_placement.rs`
    decides per-side: right-half markers (`p.x > 0.05`) anchor at the marker so the label grows
    rightward; left-half markers (`p.x < -0.05`) right-anchor at the marker so the label grows
    leftward; markers in the `|p.x| ≤ 0.05` center band stay centered.
  - When a label is wider than the available outward span (`inner_cols * (1 - |p.x|) / 2 - 1`
    cells), it is truncated to `available - 1` chars and an ellipsis is appended, preserving the
    leading characters so long snapshot-test filenames stay recognizable from the prefix.
  - Snapshot baselines refreshed across `agent_graph_*` and `agent_graph_dispatcher`. New
    regressions `file_labels_grow_outward` and `file_label_truncates_with_ellipsis` pin the
    per-side and truncation behavior.

- fix(tui): agent-graph `#NNN` label no longer collides with outbound edges that radiate south of the agent sprite (#567)
  - Pre-fix the issue-number chip was always painted at `(p.x, p.y - 0.35)` (nerd-font) /
    `(p.x, p.y - 0.08)` (ASCII) — directly on top of any edge running south. With one agent and
    two files the layout places the second file at exactly 270°, so the chip sat on the edge
    every time.
  - New `src/tui/agent_graph/label_placement.rs` module computes the angle of every outbound
    edge from the agent and places the label at the midpoint of the largest angular gap between
    them (north when there are no edges). The anchor is shifted leftward by
    `(1 − cos θ) / 2 × label_width` cells, so east-pointing labels extend rightward (clear of
    the western edge bundle) and west-pointing labels extend leftward — instead of always
    centering and pulling the label back into the edge it was meant to avoid.
  - Snapshot baselines refreshed across `agent_graph_*`, `agent_graph_dispatcher`, and
    `agent_personalities`. New regression snapshots
    `agent_label_does_not_overlap_south_edge{,_nerd_font}` pin the corrected placement.

- fix(daemon): session WIP work is now committed before post-completion gates run, so model edits survive gate failure, crash, or manual worktree removal (#562)
  - Root cause: the daemon ran post-completion gates against the worktree before ever committing the
    model's file edits. A gate failure (or any subsequent crash / `rm -rf`) could still destroy
    every file the model had written, even after #558 kept the worktree directory on failure.
  - `GitOps` trait gains three new methods: `backup_wip(branch)` creates a sentinel WIP commit
    (`WIP: <branch> — auto-backup before gates`), `amend_clean_and_push(branch)` amends that commit
    with the final message and force-pushes on success, and `head_is_wip_backup(branch)` detects
    whether HEAD is still a WIP sentinel (used by recovery tooling).
  - `completion_pipeline.rs` now calls `App::backup_wip_before_gates()` immediately after the
    session reaches `Completed`, before `gate_runner.run_gates()`. The old
    `git_ops.commit_and_push()` block is replaced by `App::amend_or_commit_and_push()` which
    amends the WIP commit on full success.
  - On gate failure the WIP sentinel commit remains at HEAD; the retained worktree (from #558) is
    therefore always recoverable with a plain `git reset --soft HEAD~1`.
  - `sanitize_log()` moved out of `auto_pr.rs` into the new `src/util/sanitize.rs` module
    (`pub fn`, crate-public) so the new `completion_git.rs` and any future consumer can re-apply
    the same input-sanitization that fixed #514 LOW-1 at the new log-message call sites
    (security analyst H-1 remediation).
  - Flag-injection hardening: `--` separator added before the branch positional in all `git push`
    invocations; `commit_and_push` now refuses branch names starting with `-`.

  **New files:** `src/git_mock.rs`, `src/git_tests.rs`,
  `src/tui/app/completion_git.rs`, `src/integration_tests/wip_backup.rs`.

- fix(tui): "Session Failed Gates" recovery modal replaces the completion overlay when gates failed (#560)
  - This is the TUI half of the post-completion gate failure recovery story: #558 retained the
    worktree on gate failure (daemon layer), and this change makes that worktree actionable from
    inside the TUI. #559 fixed noisy `remove_label` error logs that accompanied gate failures.
  - When `SessionStatus::FailedGates` is present in the completion summary, the completion overlay
    now renders an amber-bordered "Session Failed Gates" modal instead of the standard success
    overlay. The success-path modal (`[i] Browse  [r] New Prompt  [l] Logs  [d] Dashboard  [q] Quit`)
    is unchanged when all gates passed.
  - Five new keybindings on the failed-gates modal:
    - `[s]` — opens `$SHELL` rooted at the retained worktree via `ShellLauncher` trait
    - `[g]` — re-runs the post-completion gates against the retained worktree (`App::retry_completion_gates`)
    - `[r]` — spawns a `/implement #N --continue` resumption session against the worktree
    - `[v]` — opens a full-screen paged viewer of the gate stderr (`GateOutputViewer`)
    - `[q]` — closes the modal and returns to Overview (NOT ConfirmExit)
  - `Session` gains `worktree_path: Option<PathBuf>` with `#[serde(default)]`; the completion
    dispatcher sets it at the `FailedGates` transition so the recovery modal can display and act on
    the path. The `CompletionSessionLine` type mirrors this field.
  - `draw_completion_overlay()` made `pub(crate)` to allow direct invocation from snapshot tests.
  - 32 new tests across 6 TDD cycles bring the total to 4 295 (was 4 263).

  **New files:** `src/tui/shell_launcher.rs`, `src/tui/app/gate_retry.rs`,
  `src/tui/screens/gate_output_viewer.rs`, `src/tui/snapshot_tests/completion_overlay.rs`.

- fix(github): `remove_label` no longer logs an Error when the label is absent from the repo (#559)
  - Root cause: on gate failure the completion path called `remove_label("maestro:in-progress")`
    against repos whose label set never included that label. The `gh` CLI exits non-zero with
    `Label 'maestro:in-progress' not found` on stderr, which the client surfaced as a user-visible
    Error-level activity-log entry — noise with no remediation action.
  - New private predicate `is_label_not_found_error(stderr, label)` detects the pattern.
    `remove_label` now matches on the predicate: a matching error is swallowed and re-emitted at
    `tracing::debug!` level; all other errors still propagate normally.
  - 7 new unit tests in `src/provider/github/client.rs` cover the predicate against
    matching, mismatched-label, issue-not-found, auth-shape, case-mismatch, URL-form,
    and empty-stderr inputs.

- fix(session): post-completion gate failure no longer destroys uncommitted model edits (#558)
  - Root cause: on any gate failure (clippy, tests, etc.) the completion dispatcher called
    `git worktree remove --force`, silently deleting all uncommitted work the model had
    written to the worktree between the session ending and the gates running.
  - New terminal `SessionStatus::FailedGates` is set on the session instead of tearing
    down. The dispatcher in `App::check_completions` routes `FailedGates` to
    `SessionPool::finalize_retain_worktree` (new), which moves the session to the
    finished bucket and releases file claims but does NOT remove the worktree at
    `.maestro/worktrees/issue-NNN/`.
  - An activity-log entry `Worktree retained at <path> for recovery` at `LogLevel::Warn`
    is emitted so the path is visible in the TUI. Recovery affordance (TUI action to
    resume from the retained tree) is tracked in sister issue #560.

  **State-file format bump (v0.17.0):** `SessionStatus` gains the `"failed_gates"` serde
  variant. A v0.16 binary deserializing a v0.17 `maestro-state.json` will fail to parse
  the file. Delete `maestro-state.json` before downgrading.

  **API changes (in-tree only):**
  - New: `SessionStatus::FailedGates` — terminal variant; `is_terminal()` returns `true`
  - New: `SessionPool::finalize_retain_worktree(id)` — moves session to finished without
    removing the worktree
  - New: `SessionPool::worktree_exists(slug)` — delegates to the underlying
    `WorktreeManager`; used in tests to assert retain vs. teardown behaviour
  - Renamed: `SessionPool::on_session_completed` → `SessionPool::finalize_and_teardown`
    (all in-tree callers updated; public only to integration-test crate)

  **Sister issues:** #559 (label-update logs error on gate failure), #560 (TUI recovery
  affordance — depends on #558), #562 (auto-commit before gates run).

### Added

- feat(session): classify subagent dispatches and surface their Role (#542)
  - `StreamEvent::ToolUse` gains a `subagent_name: Option<String>` field populated by `extract_subagent_name()` for the three known dispatcher tools (`Agent` / `Task` / `Skill`). Plain tool calls (`Read`, `Edit`, `Bash`, …) leave it as `None`.
  - `extract_subagent_name()` strips ASCII control characters and caps the captured name at 80 visible chars (security analyst remediation: an unsanitized subagent_type that contains a literal `\x1b` would corrupt the TUI activity-log render).
  - `role_for_subagent_name(&str) -> Option<Role>` lookup mapping the 7-entry maestro subagent registry: `subagent-architect` and `subagent-master-planner` → Orchestrator; `subagent-gatekeeper`, `subagent-qa`, `subagent-security-analyst`, `subagent-idea-triager` → Reviewer; `subagent-docs-analyst` → Docs. (Gated `#[allow(dead_code)]` until consumed by #543.)
  - `tui::activity_log::ToolMeta` gains `subagent_name` so the activity log carries the dispatched subagent identity alongside the tool name.
  - `tui::app::event_handler` formats `Dispatching <name>` for tool-use events with a known subagent name, instead of the bare `Using Agent` label.
  - 23 new tests: 6 parser tests (extraction + sanitization + length cap), 8 role-lookup tests covering each registry entry, 2 ToolMeta storage tests, 2 stream-event round-trip tests, plus 5 baseline tests for ergonomics. 2 new snapshot tests in `src/tui/snapshot_tests/activity_log_dispatch.rs` pin the `Dispatching <name>` rendering and the unchanged plain-tool path.

### PR #544 review follow-ups — auto-PR guards + workflow hardening (#545 — 2026-04-30)

Eight commits closing out the post-#544 review queue. Grouped by priority:

**P0 — correctness fixes that restore behavior users already depended on:**

- `PendingPr.last_error` is removed and consolidated into `last_errors: VecDeque<String>` (cap 3). State files written by older builds migrate cleanly via a `#[serde(from = "PendingPrLegacy")]` backward-compat shim — no manual migration needed, no data loss. A canonical `awaiting_pending_pr` test fixture makes the serde contract explicit.
- `transition_to_permanently_failed` is extracted from `process_pending_pr_retries` into its own function, making the cap-3 bail-out path independently testable.
- `App::new` rehydration now surfaces the cap-coincidence note and the `AUTH_RECOVERY_HINT` constant on startup when orphan pending PRs are detected.

**P1 — human-in-the-loop safety and `gh pr create` hardening:**

- `/implement` Step 4 DOR remediation now requires the explicit `--auto-comment` flag to auto-post the gatekeeper-drafted comment and apply the `needs-info` label. Without the flag the proposed action is printed to stderr for human review and the command stops — posting LLM-emitted text to a public issue is a non-recoverable action. Add `--auto-comment` to the invocation to restore the previous auto-post behavior.
- `create_pr` gains `validate_body()` (rejects empty title; enforces `GH_BODY_MAX_BYTES` limit). Eight `GhCliClient` argv builders gain the `--repo` flag rollout via `GhCliClient::with_repo()`.

**P2 — `/pushup` last-PR-created marker and idempotent issue close:**

- After a successful `gh pr create`, `/pushup` Step 4 writes a single-line JSON marker to `~/.maestro/last-pr-created`. A running maestro TUI polls this file once per `check_completions` tick; on a fresh write it enqueues `TuiCommand::PrCreated` and triggers `/review`. The marker is consumed-once — maestro deletes it after dispatch. Malformed JSON logs a Warn entry and is deleted.
- `/pushup` Step 6.5 issue-close is now fully idempotent: if the issue is already `CLOSED` (previous run succeeded up to this point) the close + comment are skipped with a log message, not an error.

**P3 — XDG sentinel path chain and hook test coverage:**

- The `/tmp/maestro-current-gate-dir` sentinel for `$GATE_LOG_DIR` persistence is migrated to the XDG-aware chain: `$XDG_RUNTIME_DIR` → `$HOME/.cache/maestro` → `${TMPDIR:-/tmp}`. The chosen path is printed as `sentinel: <path>` on stdout. The legacy `/tmp` path is still written for back-compat with any automation that reads it directly. New helper script `.claude/hooks/sentinel-path.sh` encapsulates the resolution logic; the `/implement` Step 2 recovery snippet now walks the same three-candidate chain. New test files `.claude/hooks/tests/test-sentinel.sh` and `.claude/hooks/tests/test_parse_gatekeeper_report.py` cover the sentinel and gatekeeper-parser behaviors respectively.

**CI — self-host smoke workflow:**

- `.github/workflows/self-host.yml` is the single guard that would have caught the 2026-03-20 `gh pr create --json number` wire-format regression six weeks earlier. On `workflow_dispatch` it builds the maestro binary on a clean runner, creates an ephemeral sandbox repo, runs maestro headlessly against a fixture issue, asserts a PR is opened, then deletes the repo. Manual trigger only for v1; PR-trigger follows once stable. **Requires two repository secrets:** `MAESTRO_SELFTEST_PAT` (fine-grained PAT with Contents:RW, Issues:RW, Pull requests:RW, Administration:RW scoped to the sandbox owner) and `MAESTRO_SELFTEST_OWNER` (the sandbox org/user). The workflow refuses to run if `MAESTRO_SELFTEST_OWNER` is unset — without this guard a misconfigured run could create a test repo under the main org.

### Fixed (auto-PR + workflow root-cause batch — 2026-04-30)

This batch responds to a multi-audit review of why users were abandoning
maestro after auto-PR silently failed for six weeks. Root cause:
**no test, lint, gate, or CI step exercises the assembled binary against
the real `gh` CLI.** Every test in `provider/github/` mocks the trait;
production argv was never asserted against `gh`'s actual flag surface.

- **fix(github): drop invalid `--json number` flag from `gh pr create`** (P0). Live since 2026-03-20; every auto-PR + every Shift+P manual retry failed with `unknown flag: --json` before opening a network connection. Real PRs now parse the `https://.../pull/<N>` URL `gh` prints on stdout via `parse_pr_number_from_create_output`. 7 unit tests cover happy path, edge cases, errors.
- **feat(github): argv-capture seam (`gh_argv` module)** (P0 root-cause guard). Every `GhCliClient` method now builds its argv via a pure function in `src/provider/github/gh_argv.rs`. 19 snapshot tests lock the wire format. Future flag bugs in `pr create`, `issue view`, `pr review`, `api`, etc. produce a snapshot diff that forces reviewer attention. The whole class of bug that hid for 6 weeks is now caught at unit-test time.
- **fix(tui): persist `pending_prs` across restart** (P0). `MaestroState.pending_prs` was schema-persisted but `App::new` re-initialized empty and `sync_state` never wrote it. Crash recovery scenario from the #521 issue body silently broken: `App::new` now rehydrates from state and logs a Warn-level activity entry on startup if any orphan retries are pending. `sync_state` mirrors `pending_prs` to disk every tick.
- **fix(tui): `gh_auth_ok=false` enqueues a `PendingPr`** (P0). The "auth was missing then restored" headline scenario from #521 was silently broken: the auth-skip path returned without leaving anything for Shift+P to recover. New `defer_pr_for_missing_auth` deposits a `PendingPr { status: AwaitingManualRetry, last_error: "GitHub auth missing — run \`gh auth login\` then press Shift+P" }` and logs an action-oriented Warn entry.
- **feat(tui): Shift+P exit path** (P0). PendingPr gains `last_errors: VecDeque<String>` (cap 3) and `manual_retry_count: u32` (lifetime cap 5). After 3 byte-identical errors, the entry transitions to a new `PendingPrStatus::PermanentlyFailed` and the activity log surfaces "PR retry stuck on identical error 3× — file a bug" with the captured stderr. Manual retries past the lifetime cap also transition to `PermanentlyFailed`. Stops users from infinite-looping on a deterministic failure.
- **fix(tui): `Shift+P` now appears in the Overview hint bar** (#521 follow-up). The keybinding was wired into the F1 help registry but not the inline hint bar; users had no in-app signal it existed. New `HINT_OV_RETRY_PR` in `OVERVIEW_HINTS_BASE` and `OVERVIEW_HINTS_WITH_GRAPH`.
- **fix(workflow): `/pushup` step ordering + idempotency**. Reordered: milestone PATCH (was Step 6.5) now runs BEFORE issue close (was Step 6) so a PATCH failure leaves the issue open and `/pushup` can be safely re-run. Milestone bullet replace now anchored to `^• #<N>` (no longer mangles prose like "depends on #521 because…"); detects already-stamped entries (`• ✅ #<N>`) and skips. Sequence-line replace token-bounded; `(COMPLETED ✅)` roll-up idempotent.
- **fix(workflow): `implement-gates.sh` non-interactive flags**. `read -r` interactive prompts couldn't work under Claude Code's Bash tool — silent abort. Adds `--dirty-tree-action=stash|abort|ask` (default `ask`, but auto-aborts when stdin is not a TTY with an explicit error message). `/implement` gains `--continue` and `--restart` flags so Step 5's idempotency prompt is resolvable headlessly. `$GATE_LOG_DIR` now persisted via `/tmp/maestro-current-gate-dir` sentinel so subsequent Bash tool calls can recover it.

### Added

- feat(tui): `Shift+P` keybinding to manually trigger PR creation when auto-PR was skipped (#521)
  - Recovery path for sessions whose auto-PR didn't fire — GitHub auth was missing then restored, retries exhausted, AC4 detection saw a stale PR, or `pending_completions` were rehydrated after a crash.
  - Wired into `handle_global_shortcuts` in `src/tui/input_handler.rs`; eligibility is `app.pending_prs` membership for the selected session's `issue_number` (the codebase's encoding of "session needs PR"). Gated by `is_text_input_mode` to avoid colliding with prompt input.
  - Calls `App::trigger_manual_pr_retry`, which mutates the matching `PendingPr` (status → `RetryScheduled`, `attempt` → 0, `next_retry_at` → now) so the next `process_pending_pr_retries` tick picks it up. The AC4 preflight from #514 still runs inside that cycle, so manual triggers cannot create a duplicate PR.
  - Help overlay updated in `src/tui/navigation/keymap.rs` (Session Control group).
  - 6 new tests: 3 in `src/tui/app/pr_retry.rs` cover the state mutation + log entry + isolation between unrelated `PendingPr`s; 3 in `src/tui/input_handler.rs` cover the keypress path (eligible → fires; no pending → noop; text-input mode → noop).

### Fixed

- fix(tui): auto-PR pipeline reliability and observability (#514)
  - **Root cause**: `pending_issue_completions` was held only in memory; `App::sync_state()` did not persist it. A maestro shutdown between session-end and the next `check_completions` tick silently dropped the auto-PR work, leaving branches on origin without a corresponding PR.
  - `MaestroState` gains `pending_completions: Vec<PendingIssueCompletion>` with `#[serde(default)]` so the field round-trips through `maestro-state.json` without breaking existing state files. `App::new()` rehydrates in-flight completions from persisted state; `sync_state()` mirrors them back on every tick.
  - PR URL is now printed to the activity log on success: `"PR #N created: https://github.com/owner/repo/pull/N"`.
  - Existing-PR detection: `list_prs_for_branch()` is called before attempting creation; when an open PR already exists the existing URL is surfaced and creation is skipped.
  - General-error path appends a manual recovery hint (`gh pr create --base <base> --head <branch>`) to the activity log.
  - In-process idempotency via `App::attempted_pr_issue_numbers: HashSet<u64>` prevents duplicate attempts within a single maestro run.
  - Every previously-silent gate (`auto_pr=false`, `github_client=None`, `worktree_branch=None`) now emits an explicit activity-log entry at the appropriate log level.
  - `GitOps` trait gains `has_commits_ahead(branch, base)` with `CliGitOps` impl and `MockGitOps` extension; all git CLI calls hardened with a `--` flag-prefix guard.
  - New module `src/tui/app/auto_pr.rs` owns the pipeline; `issue_completion.rs` is now a thin entry point (~150 LOC). 8 behavior tests in `src/tui/app/auto_pr_tests.rs` cover the new acceptance criteria.
  - Deferred to a follow-up: explicit zero-commit detection at session-end (AC3).

- fix(tui): detect zero-commit sessions and skip empty-PR creation (#520)
  - Closes the AC3 deferral from #514: `auto_pr::run_auto_pr` now calls `git_ops.has_commits_ahead(worktree_path, branch, base_branch)` between the AC4 PR-already-exists preflight and the issue-resolution step.
  - On `Ok(false)`: logs a Warn-level "No commits found — skipping PR creation. Branch: <branch>" entry, queues a Critical desktop notification, and returns without calling `create_pr` or pushing to `pending_prs`.
  - On `Err(_)`: emits a `tracing::warn!` and proceeds optimistically with the existing `create_pr` flow (the existing error-handling path takes over).
  - `App` gains a `git_ops: Box<dyn GitOps>` field (production wires `CliGitOps`; tests inject `MockGitOps` via `with_git_ops`). `on_issue_session_completed` and `run_auto_pr` gain a `worktree_path: Option<PathBuf>` arg threaded from `PendingIssueCompletion`.
  - Two new behavior tests: `auto_pr_zero_commits_skips_pr_with_visible_message` and `auto_pr_git_check_error_falls_through_to_create_pr` (the latter pins the AC3 fallthrough contract).

### Changed

- ci(audit): grant `checks:write` permission to the Cargo Audit workflow job; triage RUSTSEC-2026-0002 (#517)

### Documentation

- docs: README.md overhaul reflecting current project state, features, and install instructions (#331)
- docs: GitHub Wiki — feature guides, architecture overview, and usage documentation (#330)

## [0.16.1] - 2026-04-29

Milestone "v0.16.1" — Idea Triage Foundation (idea-inbox funnel with the consultive `subagent-idea-triager`, `/triage-idea` slash command, parse hook, and Idea issue template) plus v0.16.0 carry-overs: tech-stack auto-detection in `maestro init`, milestone-health wizard, Discord release notifications, atomic auto-updater rewrite, desktop-notification fix, and an actionlint workflow-lint CI gate. Closes #482, #483, #484, #485, #486, #487, #499, #500, #505, #507, #510.

### Added

- feat(init): auto-detect project tech stack on `maestro init` and `maestro init --reset` (#505)
  - `src/init/` — detection layer: `DetectedStack` enum (Rust, Node, Python, Go), `ProjectDetector` trait, `FsProjectDetector` probes marker files (`Cargo.toml`, `package.json`, `pyproject.toml`/`requirements.txt`/`setup.py`, `go.mod`)
  - `src/init/template.rs` — per-stack command defaults (`build_command`, `test_command`, `run_command`); `render_template()` writes a complete `maestro.toml`
  - `src/init/merge.rs` — `merge_toml()` adds detected keys that are absent from an existing file without touching user-set values; TOML comments are not preserved (library limitation)
  - `src/init/walk.rs` — `find_project_root()` walks ancestors for a known marker file
  - `--reset` flag on `maestro init`: re-runs detection and merges results into the existing `maestro.toml`
  - Settings → Project tab gains a **Reset Settings (re-detect project stack)** row that triggers the same detection and merge flow from the TUI
  - `[project]` in `maestro.toml` gains optional `language`, `languages`, `build_command`, `test_command`, `run_command` fields written by detection; polyglot repos record all stacks under `languages`, with the first in canonical order (Rust → Node → Python → Go) driving the active commands
  - `src/integration_tests/init.rs` — integration tests covering fresh write, idempotent guard (exit 2 when file exists and `--reset` is absent), merge-preserves-user-keys, and polyglot detection

- feat(tui): Milestone Review wizard (`h` on landing, `M` on dashboard) — selects a GitHub milestone, checks every open issue for DOR readiness and dependency-graph coherence, shows an inline diff of the proposed corrected milestone description, and writes the patch to GitHub on user confirmation (#500)
  - `src/milestone_health/` — pure analysis layer: DOR checker (`dor.rs`), dependency-graph parser / level-computer / cycle-detector (`graph.rs`), deterministic patch generator (`patch.rs`), aggregated report type (`report.rs`)
  - `src/tui/screens/milestone_health/` — TUI wizard: state-machine reducer (`state.rs`), per-step rendering (`draw.rs`), line-pair diff view (`diff.rs`), anomaly and missing-field formatters (`format.rs`)
  - `src/integration_tests/milestone_health_wizard.rs` — 9 end-to-end tests against `MockGitHubClient`
  - `GitHubClient` trait extended with `patch_milestone_description`; `GhCliClient` impl uses `gh api ... --method PATCH`; Azure DevOps stub returns `bail!`

- feat(ci): Discord `#releases` notification on new version tag (#507)
  - `.github/workflows/release.yml` gains a `notify-discord` job that POSTs a message to the channel webhook after a successful release build
  - Pre-release tags (alpha, beta, rc, pre, dev, canary, smoketest) are suppressed and do not trigger a notification
  - `workflow_dispatch` dry-run mode (`dry_run: true`) prints the payload to the Actions log instead of POSTing; use `target_tag` to simulate any tag
  - Requires `DISCORD_WEBHOOK_URL` repo secret; optional `DISCORD_WEBHOOK_URL_TEST` for staging smoke tests

- feat(tui): copy focused agent-tab response to clipboard via keybinding in session overview (#482)

- feat: Idea Triage Foundation — upstream idea-inbox funnel (#483, #484, #485, #486)
  - `.github/ISSUE_TEMPLATE/idea.yml` — 5 required textareas (the itch + Q1-Q4 honesty checks) + Q5 vision-alignment dropdown; auto-applies labels `idea` and `needs-triage`
  - `.claude/agents/subagent-idea-triager.md` — consultive triage subagent (no auto-mutation; emits structured JSON report)
  - `.claude/commands/triage-idea.md` — slash command wiring with confirmation prompts before any GitHub mutation
  - `.claude/hooks/parse_idea_triager_report.py` — output-contract parser hook with fixtures
  - End-to-end smoke-tested with a throwaway issue

### Fixed

- fix(notifications): desktop notifications now fire when a session completes or errors (#487)

- fix(updater): atomic binary replace via `BinaryReplacer` trait (#499)
  - Tempfile + atomic rename + `.bak` rollback; `flock`-based `UpdateLock` with `O_NOFOLLOW` + `O_CLOEXEC` rejects symlinked locks
  - Typed `UpdateError` variants surface in the TUI banner verbatim
  - `download_and_install` parallelizes binary fetch + `SHA256SUMS` via `try_join!`
  - 18 new tests; existing tests migrated to typed-error signatures

### Changed

- chore(ci): actionlint workflow-lint gate; retire per-issue `verify-issue-NNN.sh` convention (#510)
  - `.github/workflows/ci.yml` gains an `actionlint` job that lints every workflow YAML on push/PR (strict severity, surfaces shellcheck `info` findings)
  - Pinned-version binary download + sha256 verify (no bootstrap-via-curl); `permissions: {}`, `persist-credentials: false`
  - Cross-cutting CWE-78 invariant step: no `secrets.*` interpolation outside `KEY: ${{ secrets.NAME }}` env/with assignments
  - `scripts/verify-issue-{485,507}.sh` deleted; `/implement` spec gains a "Binding-gate selection" subsection — future CI-only changes use tool-specific binding gates wired into `ci.yml` instead of per-issue verify scripts

## [0.16.0] - 2026-04-25

Milestone "PR Review Automation & Interactive PRD" — automated PR review flow with slash command integration, dangerously-skip-permissions bypass mode for power users, interactive PRD management, milestone roadmap visualization, and the opt-in caveman compressed-prose skill. Closes #321, #322, #327, #328, #329, #481, #490 across PRs #488, #489, #491.

### Added

- `src/settings/` — `SettingsStore` trait, `FsSettingsStore` atomic writer, and `CavemanModeState` enum; surfaces caveman mode as a Space-toggleable row in the TUI Settings screen with four visual states (ExplicitTrue, ExplicitFalse, Default, Error) and a title-bar status flash on save (#490)
- `src/tui/screens/settings/caveman_row.rs` — render helper for the caveman-mode row; `tests/settings_caveman.rs` integration tests and five insta snapshot tests in `src/tui/snapshot_tests/caveman_row.rs` (#490)
- `.claude/skills/caveman/SKILL.md` — opt-in compressed-prose skill that drops articles, fillers, and transitional prose while preserving code, paths, JSON/TOML, identifiers, and quoted text verbatim; gated by `behavior.caveman_mode` in `.claude/settings.json` (#481)
- `behavior.*` namespace in `.claude/settings.json` reserved for non-security style/UX toggles; documented in `.claude/CLAUDE.md` (#481)
- `src/prd/` — `Prd` model, `PrdStore` JSON persistence, and `PrdExporter` markdown export for the interactive PRD flow (#321)
- `src/review/types.rs`, `parse.rs`, `audit.rs`, `apply.rs`, `bypass.rs` — review pipeline types, PR-comment parser, audit log, patch applicator, and bypass guard (#327, #328)
- `src/session/pr_capture.rs` — `PrCapture`: intercepts stream-json to detect `/review` PR comments (#327)
- `src/commands/slash.rs` — `SlashCommandRunner`: executes slash commands against a PR and feeds results to the review pipeline (#327)
- `src/tui/screens/roadmap/` (`mod.rs`, `dep_levels.rs`) — roadmap screen foundation with dependency-level grouping and sequence visualization (#329)
- `src/tui/screens/bypass_warning.rs` — confirmation overlay displayed when `--bypass-review` is active (#328)
- `src/tui/widgets/bypass_indicator.rs` — F-key bar badge warning that the review council is disabled (#328)
- `docs/api-contracts/review-comment.json` — JSON Schema (Draft 2020-12) for the `maestro-review` block embedded in `/review` PR comments (#327)
- `docs/api-contracts/README.md` — convention guide for the contracts directory
- `--bypass-review` global CLI flag (session-only) in `src/cli.rs` (#328)

## [0.15.2] - 2026-04-24

Milestone "Pixel-art mascot sprites" — replaces the hand-authored Unicode block-character mascot with 1-bit pixel-art sprites rendered via half-block (`▀ ▄ █`) encoding. Adds a `[tui].mascot_style` config key so the legacy ASCII block art stays available as a fallback. Closes #473, #474, #475, #476.

### Added

- `[tui].mascot_style = "sprite" | "ascii"` config key (default `"sprite"`) controls which mascot renderer is active (#473)
- Six 128×128 pixel-art sprite `.bin` files (`conducting`, `error`, `happy`, `idle`, `sleeping`, `thinking`) embedded via `include_bytes!` in `src/mascot/sprites.rs` with compile-time length assertions (#474)
- `MascotStyle` enum (`Sprite` | `Ascii`) in `src/mascot/mod.rs`; `sprite()` accessor and test-only `pixel()` MSB-first unpacker (#474)
- `MascotWidget::with_style()` builder, aspect-preserving `render_sprite()` path (nearest-neighbor downscale that fits the largest `2:1` sub-rect of the caller's area), and unchanged `render_ascii()` path (#475)
- `should_show_dashboard_mascot_panel()` and `dashboard_mascot_layout()` style-aware gate and size helpers in `src/tui/ui.rs` (#476)

### Changed

- `MascotFrames` renamed to `AsciiMascotFrames`; constants `MASCOT_ROWS` / `MASCOT_WIDTH` replaced by `MASCOT_ROWS_ASCII` / `MASCOT_WIDTH_ASCII` (#476)
- `HomeScreen::set_mascot()` and `LandingScreen::set_mascot()` now accept a `MascotStyle` parameter (#476)
- `App` struct gains `mascot_style: MascotStyle` field, hydrated in `apply_config()` (#476)

## [0.15.1] - 2026-04-24

Milestone "Wizard text-editing follow-ups" — closes two usability gaps that emerged during manual testing of the v0.15.0 wizards: the Issue Wizard's AI Review step now offers a one-key "improve with AI" action, and every wizard text field is now a `tui-textarea` widget with full cursor/selection/undo support.

### Added

- Issue Wizard `AiReview` step gains an `i: improve with AI` keybinding that launches a second `claude --print` call using the just-generated critique as guidance, then shows a before/after diff the user can accept or discard atomically; turns the critique into a lift instead of a wall of text (#450)

### Changed

- Both wizards (Issue, Milestone) migrated from hand-rolled `String` buffers to `tui-textarea` widgets — full cursor movement (arrows/Home/End), word-wise jumps (Ctrl+Left/Right), selection (Shift+arrows), word-wise delete (Ctrl+W), and undo/redo (Ctrl+Z/Ctrl+Y) out of the box; single-line `Title` field configured to strip `\n` on input; ~300 LOC of duplicated buffer/paste/sanitization code deleted (#447)

### Fixed

- CI: collapsed a `KeyCode::Enter` if-chain into a match guard to satisfy clippy's `collapsible_if` lint

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
