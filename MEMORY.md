# MEMORY.md

Decision log for `maestro`. Read at the start of every session. Append on any significant decision: what, why, what was rejected.

Format:

```
## YYYY-MM-DD — short title

**Decided:** <what>
**Why:** <reason>
**Rejected:** <alternative + why it lost>
```

Session summaries (added when I say "session end" / "wrapping up" / "let's stop here") use:

```
## YYYY-MM-DD — session summary

- Worked on:
- Completed:
- In progress:
- Decisions made:
- Next session priorities:
```

---

## 2026-05-23 — teams bindings adapter RESERVED_KEYS coupling

**Decided:** `RESERVED_KEYS` in `src/tui/screens/settings/schema_tab/teams_bindings.rs` is the single place that maps `TeamConfig`'s `#[serde(flatten)]` scalar keys (extends, primitive, min_agents) against the free-form role-binding keys. Any future change to `TeamConfig`'s non-binding fields (add/remove/rename a struct field that is NOT a binding) requires updating `RESERVED_KEYS` — otherwise those keys appear as `role=agent` items in the TUI.
**Why:** `#[serde(flatten)]` merges all top-level scalar keys into the same map; there is no runtime way to distinguish them without an explicit allow/deny list. The const keeps the coupling in one file and is called out in the directory-tree comment.
**Rejected:** Storing the list on `TeamConfig` itself (adds a doc-only method to a config struct) and deriving it from the `FieldSchema` slice (the schema slice is defined in a different module and creates a circular read dependency at const-eval time).

## 2026-05-21 — golden-rules canonicalisation + provider entry points

**Decided:** Single canonical at `.maestro/templates/core/golden-rules.md`. Each provider entry point (`.claude/CLAUDE.md`, `.codex/AGENTS.md`, `AGENTS.md`, `GEMINI.md`) embeds the canonical between `<!-- BEGIN GOLDEN-RULES -->` / `<!-- END GOLDEN-RULES -->` markers. Drift enforced by `scripts/check-rules-drift.sh` + a `rules-drift` CI job.

**Why:** One source of truth for the rules; any agent (Claude, Codex, Gemini, Aider, Copilot, any future provider) gets the same expectations. CI catches drift so providers stay in sync without manual checks.

**Rejected:** (a) Wire `maestro sync-templates` into the render pipeline for provider entry files — too much Rust work for a docs concern, defer until rules change frequently enough to justify. (b) Duplicate content into each provider without a drift gate — silent rot is the failure mode that bit us before with snapshots.

## 2026-05-25 — role_overrides TUI: schema-locked in v0.29.x, editor deferred

**Decided:** Ship the `role_overrides` schema slot (PR-A, #872) as read-only in v0.29.x. The `ROLE_OVERRIDE_FIELDS` const and the 5th `TEAMS_ENTRY_FIELDS` entry are locked; `EntryState.passthrough` preserves existing on-disk data through a save cycle. A full inline editor is a separate follow-up issue (PR-B).
**Why:** Schema slot + round-trip safety can ship independently. The editor widget is non-trivial and was deferred to keep PR-A small and reviewable.
**Rejected:** Shipping both schema slot and editor in one PR — too large; risked blocking the round-trip fix on widget work.

## 2026-05-26 — role_overrides editor shipped in #901; EntryState.passthrough retained

**Decided:** Keep `EntryState.passthrough` as a defense-in-depth fallback for `FlattenedMap` and `VecOfStruct` entry-field kinds. No live schema exercises these paths today, but the path exists for any future lift.
**Why:** Dropping passthrough would couple lift-order to schema evolution; retaining it costs ~20 LOC and zero runtime overhead.
**Rejected:** Removing passthrough to slim the widget — would require a follow-up re-add whenever a new nested-map field kind is added to any schema.

## 2026-05-23 — v0.29.5 cross-milestone handoff bundle #806/#875/#876/#877

v0.29.5 bundle (user authorized PR-isolation override for context budget): one PR, four Closes refs, architect+QA blueprint per scope; disabled-agent filter (#806), Ctrl+V paste (#875), autocomplete (#876), LaunchTeam dispatch fan-out (#877); R3 (real run_team) was descoped to follow-up and landed in #881.

## 2026-06-12 — #949 PR-keeps-alive: notify + quit-wipe (PR #994)

**Decided:** `Session.pr_linked: Option<u64>` (number, not spec's bool — feeds the System turn + #950 banner). Flag sets instantly; announcement defers to turn boundary while streaming (`ManagedSession.queued_pr_notice`, #936 contract kept so the transcript never interleaves). Quit now WIPES the worktree (was: kept) — `cancel_inflight_turn` interrupts the provider child before git runs; #741 machinery re-pointed at quit (banner → spawn_blocking wipe → Terminated(UserQuit) → auto-nav). Deleted: `interaction_lifecycle.rs`, `CloseReason::PrCreated`, screen queued-terminator. `TransitionReason::PrLinked` kept deserializable for #948-era state files.
**Why:** Spec §4.4 locked PR-keeps-alive + teardown-on-quit; mid-stream System-turn insertion would break the streaming-agent-record invariant, hence deferral.
**Rejected:** quit keep/wipe two-choice modal (not in spec; spec decision 2 says teardown on quit); posting the announcement mid-stream by inserting before the streaming record (fiddly on both transcript copies).
**Notes for #950:** screen-as-view deletes `view_state.rs`; render `pr_linked` + `settled_from` banners from the live Session; closes #930 switcher. Turn-level cancel now has a real consumer (`cancel_inflight_turn`) — a mid-turn cancel key becomes feasible.

## 2026-06-12 — #948 state unify: transition_to choke point + InteractionSession retired (PR #993)

**Decided:** Settle interception lives INSIDE `Session::transition_to` (single choke point, guarded on `SessionMode::Interactive`), not at the call sites — spec §8's missed-branch risk solved structurally. `Killed` is the only non-intercepted terminal; `/pushup` terminator fires as `Killed`/`TransitionReason::PrLinked` until #949. Transcript persists on `Session.turns` (re-entry now survives restarts — the old `MaestroState.interactions` was dead persistence, never hydrated/synced; dropped). Screen keeps a view-local `view_state.rs` (InteractionState + CloseReason) until #950 — the one AC deviation, flagged on the PR.
**Why:** Per-site branching is exactly the failure mode the spec feared; the choke point covers future sites too. Retiring the screen enums now would drag Phase 5's screen-as-view into Phase 3.
**Rejected:** (a) #948a/#948b split — port compiled in one pass, split unnecessary. (b) Interception-only scope C — AC demanded the struct retirement; only the view enum stayed.
**Notes for #949:** repoint wipe_worktree to the quit path; PR marker → `pr_linked` notify instead of terminate (kills the `Killed`-skull cosmetic wart + the PrLinked reason path); #936 deferral semantics now live in `ManagedSession::settle_queued_terminator`.

## 2026-06-10 — #947 re-scoped: real pipeline Session for interactions (PR #992)

**Decided:** Phase 2 (#947) gives the interaction a REAL pool-registered `SessionMode::Interactive` `Session` driven by `ManagedSession` (Option C). First turn = normal `spawn`; follow-ups = new `send_followup_turn` (`--resume <agent_session_id>`, allowlisted). Telemetry parity by construction — records land on the pool `Session` via the existing `handle_event` funnel. `interaction_turn.rs` deleted; `TurnEvent` lives in `session/interaction.rs`. Interactive sessions exempt from one-shot completion machinery (gates/auto-PR/notifications/#327 PR-detect) and skipped by `find_by_issue_mut`; follow-ups make NO status transition (Completed→Spawning illegal) until #948 adds the `Interactive` status.
**Why:** The "normal resumed-turn pipeline" the issue presumed didn't exist — one-shot never set `resume_session_id` and dropped the provider conversation id. Building it once on `Session`/`ManagedSession` means #948 deletes `InteractionSession` without touching the turn path.
**Rejected:** (a) Order swap #948-first — retiring `InteractionSession` forces the turn path to move anyway; the "swap" is really one oversized PR. (b) Telemetry shadow `Session` inside `InteractionSession` — invisible to dashboards (parity dishonest) and deleted by #948 (throwaway, spec decision 3).
**Notes for #948:** settle interception closes the `session_bound`-after-`Completed` race; `settled_from` banner; enumerate all 4 terminal-status sites; delete `InteractionSession` + `upsert_interaction`/`clone_active_interaction` (quit path still uses them today). Milestone #57 graph needs ✅ for #947 after merge.

## 2026-06-10 — Overnight batch: v0.30.5 complete + v0.30.0 partial (PR #991)

**Worked on:** milestones v0.30.5 (Subscription Transport) and v0.30.0 (Interactive Iteration Sessions), one branch (`feat/v0.30-batch-transport-unification`), one commit per issue.

**Completed (PR #991, Draft pending manual QA):** #747 #749 #750 #751 #752 (v0.30.5 complete — 2026-06-15 cutoff workaround shipped), #941 #742 #919 #918 #743, plus a security fix (resume_session_id allowlist). #936/#988/#953 closed directly (already on main via PR #990).

**Decisions:**
- PTY transport (#749): transcript-JSONL tailing chosen over screen-scraping; session id pinned with `--session-id` so the transcript path is deterministic pre-spawn (spike #747 GREEN). tmux fallback rejected as primary (external dep, no child ownership); stub feature `claude-tmux` kept.
- #751: PTY children are PARKED between turns keyed by session id; reuse gated on `resume_session_id` matching, so one-shot runs never share a REPL context. Provider injected per turn (InteractionSession stays serde).
- **Unification phases #947–#950 (+#935/#929/#930) deferred** — spec 2026-06-04 §6 mandates one PR per phase, and Phase 2's telemetry parity needs Phase 3's Session merge. Rejected: bundling them half-done into the overnight PR.
- #918: shipped without syntect (AC allowed); `o` escape hatch = shell at worktree via ShellLauncher, not $EDITOR suspend.

**Next-session priorities:** merge-gate PR #991 manual QA; then #947 → #948 → #949 → #950 in separate PRs; verify chat renderer sanitizes `StreamEvent::Unknown.raw` (security review informational).

---

## 2026-11-10 — Strategy pivot: the gated wave engine is the product; engine unblocked

**Worked on:** full open-issue audit (82), t3code teardown (pingdotgg/t3code), setwave state assessment, setwave liveness fix, issue-list hygiene.

**Decisions:**
- **Pivot (center of gravity, not abandonment):** maestro stops competing with T3 Code on the multi-CLI control surface (they ship desktop+mobile+remote with a team; head-on is a losing race solo). The differentiated asset = the opinionated execution layer: DOR gates, TDD-enforced implement, merge-blocking judge, machine-checked dependency graphs — proven by setwave driving 11 merged maestro PRs (Oct 14–Nov 7). TUI work (UX Spine chain #826–#835) and the role-routing chain (#815–#821) deprioritized as commodity surface.
- **setwave stays the GitHub execution layer; maestro owns sessions/TUI + the method templates.** `define` vs gatekeeper DOR overlap resolved in setwave's favor (machine-checked beats prose); judge (merge-blocking) beats advisory review. Fold-in of the wave loop into the maestro product is the candidate next phase, NOT a Rust port of wave.py (4.9k tested lines — shell out or keep as plugin first).
- **Validation gate:** setwave has 1 external star. One focused attempt to get the methodology used outside Carlos's repos (publish a real wave log + sharp README); ~4–6 weeks; no external evidence → fall back to "personal tooling, fine" without guilt.
- **Port list from t3code (evidence in their repo, ranked):** context-budget CI gate on model-facing bytes (S–M), no-flaky-test gate rules (`#[ignore]`/`sleep` need a quarantine reason) (S), replay provider from recorded stream-json transcripts (M), typed per-provider capability structs (M–L), idempotent command receipts (M–L), per-turn hidden-ref checkpoints (L).

**Completed this session:**
- setwave PR #141 merged (168e578): watchdog no longer dies on first hit (`--watch` persistent + dedup, `--once` = old exit-1 contract), PR caches refreshed every cycle, `--transcripts` counts a thinking agent as working, pushed-without-PR wake says `gh pr create`, background-gate guard (5th judge) refuses `run_in_background`/shell `&`/nohup inside seats. TDD: 5 behaviors failed first. 606/606 tests, guarantees + skill-lint green. Fixes #140 #115.
- maestro guards: dead 0.6.1 cache pin (was blocking Bash calls, fail-open = unprotected) → repo checkout of merged main; all 5 guards active in project settings (backup `.bak-20261110T151919Z`).
- Issue hygiene: closed #818/#980/#982 as already-implemented (evidence comments); corrected 6 stale bodies (#817 #820 #832 #834 #1014 #1015); milestone graphs 60/63 ✅-updated. Audits found #964–#974 all still valid (PR #1028 shipped Input foundation only — children intact).

**Next-session priorities:** wave #1065 + #1050 + #1061 (three epics one item from done), then #1084 spike (ai-memory seam) → epic #1083; then the Security Audit milestone (19 disclosed, P0 #1045) as a 2-week wave; setwave release bump (plugin cache still 0.11.0; guards run from repo checkout meanwhile); README cutoff-note fix (README.md:19); ERRORS.md entry for the #1065 double-reopen pattern.

---

## 2026-11-10 (session 2) — October plan delivered: 4 PRs merged, 3 epics closed, engine running

**Executed via setwave (fixed engine, repo-checkout script):** #1087 (e2e interaction test, closes #1050) · #1088 + #1089 (TUI demo capture: script+test on main, webm+hero embed on gh-pages; closes #1061) · #1090 (Fleet board quota badges + glyph fix, closes #1065). Epics #1047, #1059, #1063 CLOSED. 14.3 GB reclaimed. Live page serves the video hero. Every PR: verify OK, judge verdict (pass/concerns), owner OK before squash merge, ledger applied.

**Decisions:**
- setwave fixes land in ~/projects/setwave (PRs #141 #142 merged); maestro consumes guards VENDORED at `.maestro/hooks/guards.py` via `$CLAUDE_PROJECT_DIR` (no out-of-repo pin). Plugin cache (0.11.0) is stale — release bump pending.
- Guard policy shipped through 3 security-review rounds: background/detach/reparse detection, git shim+global-option peeling, secret-read allowlist inversion. Judges false-fire on in-process subagents (no per-seat transcripts) — milestone messages are the real liveness signal; `--transcripts` only helps host-session seats.
- Stale branch handling WITHOUT `-D` (guard refuses): `git branch -m` to `archive/<name>` frees the dispatch name, preserves squash-shadowed commits.

**Still open (priority order):**
1. ai-memory wave: dispatch #1084 (spike, seam probe at config/agents.rs + headless.rs) → then #1083 children. Epic #1083 open, nothing run since Oct 19.
2. Security Audit milestone: 19 disclosed issues, P0 #1045 (workspace trust), untouched 2 months — next big wave.
3. Follow-up issues to FILE: ASCII-icons debug-build TUI stall (found in #1090 PR), Fleet Board privacy note (renders real ~/.claude history — #1089 excluded it), setwave gap: `verify`/`judge` cannot diff PRs against orphan branches (gh-pages, no merge base — #1089), setwave release bump (v0.11.1+).
4. Housekeeping: `wave cleanup --stale-days` for maestro-1065 seat (branch `story/1065-quota-badge` is pushed; local `story/1065-quota-routing` shadow reads unpushed); untracked junk at repo root (err/out-before-1075.txt, old settings backups) needs a yes to delete; README.md:19 stale billing-cutoff note.

---

## 2026-11-10 (session 3) — security wave 1 executed: 9 PRs merged, P0 in fix round

**Worked on:** filed 9 issues (maestro #1091 ascii-icons-stall, #1092 fleet-privacy-note, #1098 prompt_history-symlink, #1099 adapt-symlink-parents, #1102 slack-sink, #1107 env-denylist-extension; setwave #143 orphan-branch-diff, #144 release-0.11.1, #145 round-verdict-deadlock); ran Security wave 1 (11 dispatches: batch A #1031–#1034, batch B #1029/#1030/#1035/#1037, #1098 follow-up, P0 #1045); judged 11 PRs with 4 fix/envelope interventions.

**Completed:** 9 PRs squash-merged BY OWNER on GitHub (#1093–#1104): #1084 spike (ADR: option (a) shim + --no-autowire; plan dir plans/ai-memory-shim/) + Level-0 fixes #1029/#1031/#1032/#1033/#1034/#1035/#1037. #1105 conflict (predicted 1-line tests.rs mod) resolved by spawned merge agent (head 2a9fd7d, 12,537 tests green, GitHub MERGEABLE, judge pass). Milestone 65 + epic #1083 graphs ✅-updated post-merge. `wave cleanup` reclaimed 9 seats; `cargo clean` on 11 seat targets freed ~98 GB.

**In progress:** #1106 (P0 trust gate) in judge FAIL fix round — 4 blocking spawn-funnel findings (completion tick-path promotion, interaction sessions, team launch, env-only predicate bypass) ordered to seat; re-judge (--since) + merge ask follow. #1105 pass-verdict at 2a9fd7d awaiting owner squash.

**Decisions:** (1) engine = repo checkout `~/projects/setwave/scripts/wave.py`, never 0.11.0 cache; (2) audit issues dispatched as-is on missing Done when/ADR (owner-away skip path; ISSUE-NO-DONE-WHEN accepted, ledgers in PR bodies — owner must add `## Done when` headings to #1031–#1034, #1035–#1037, #1045 bodies someday); (3) consent gates must live OUTSIDE repo control (env switch / platform config dir) — doctrine from #1105's blocking finding, baked into #1045's dispatch; (4) owner merges via GitHub UI = per-delta OKs (bulk-merged 9 at 00:00Z); orchestrator merged nothing; (5) judge round-verdict envelope workaround for setwave#145: verdict carries whole-patch sha, judge reads delta — pre-empt in spawn brief; (6) MEMORY.md committed locally (ef957a9) to satisfy doctor's clean-checkout before dispatch; no push; (7) no AskUserQuestion (tmux killer) — all asks plain text; (8) seat-fanout hygiene: 10 parallel worktrees ≈ 10 GB target/ each — disk wall at 100% mid-gate; `cargo clean` done seats proactively next time.

**Next-session priorities:** (1) #1106 fix-round re-judge → final merge ask; (2) #1105 squash (pass verdict, mergeable, zero blockers); (3) housekeeping still pending owner OKs: maestro-1065 seat (push shadow branch to archive/ OR discard — explicit yes needed), root junk files (err/out-before-1075.txt, old settings .bak), README.md:19 stale billing note reword; (4) dedupe comment on #1032 (line ready, needs yes); (5) wave 2 candidates: #1036 (bypassPermissions default) → #1038/#1041/#1042 (gates cluster) → #1039/#1040 (infra) → #1046 (signing) + #1107; (6) unfiled observations needing owner read: notify.ps1 loose parser (`^([A-Z_]+)=(.*)$` + Set-Variable), tui::markdown perf flake under parallel load (known f9331f4), setwave doctor false-negatives on $CLAUDE_PROJECT_DIR guard paths.

## 2026-11-11 — wave 1 complete: all 11 merged; CI fires fought

**Owner merge run (explicit OK in conversation, order 1109→1106→1105):** #1109 (ETXTBSY flake fix) · #1106 (P0 trust gate, round-2 concerns: 4/4 blocking verified fixed) · #1105 (pass). main @ e0b058a; main CI queued on watch.

**CI fires fought (all resolved):** (1) #1105 File Size Lint — agents.rs hit 409/400 after the conflict-merge; fixed by extracting `validate_http_agent` into agents_remote_auth.rs (my miss: conflict-agent brief lacked check-file-size — add the full gate to any merge-resolution brief). (2) #1106 Coverage Tiers — NOT coverage: ETXTBSY flake in #1104's qwen probe test (Linux overlayfs copy-up under llvm-cov); fixed on main via #1109 (bounded 5×200ms retry on ExecutableFileBusy only). (3) Nightly red — runner shutdown-signal cancellation mid-mutation-suite (infra, rerun triggered, not code).

**Trust-gate note for this machine:** maestro's own maestro.toml carries gates commands (build/test/run) — first run of a NEW build in this repo will prompt once for workspace trust; `maestro trust` records it (user config dir, sha256-pinned).

**Still open:** #1108 (team_launch e2e test — judge's flip-to-pass condition), #1099/#1102/#1107 (post-merge hardening follow-ups), wave 2 candidates #1036 #1038–#1044 #1046, housekeeping a–d (1065 seat, junk files, README:19, dedupe comment), judge reset ~10:04 → --since rounds for #1105 (verdict predates file-size push) if wave ledger hygiene wanted.
