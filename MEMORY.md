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

## 2026-06-10 — Overnight batch: v0.30.5 complete + v0.30.0 partial (PR #991)

**Worked on:** milestones v0.30.5 (Subscription Transport) and v0.30.0 (Interactive Iteration Sessions), one branch (`feat/v0.30-batch-transport-unification`), one commit per issue.

**Completed (PR #991, Draft pending manual QA):** #747 #749 #750 #751 #752 (v0.30.5 complete — 2026-06-15 cutoff workaround shipped), #941 #742 #919 #918 #743, plus a security fix (resume_session_id allowlist). #936/#988/#953 closed directly (already on main via PR #990).

**Decisions:**
- PTY transport (#749): transcript-JSONL tailing chosen over screen-scraping; session id pinned with `--session-id` so the transcript path is deterministic pre-spawn (spike #747 GREEN). tmux fallback rejected as primary (external dep, no child ownership); stub feature `claude-tmux` kept.
- #751: PTY children are PARKED between turns keyed by session id; reuse gated on `resume_session_id` matching, so one-shot runs never share a REPL context. Provider injected per turn (InteractionSession stays serde).
- **Unification phases #947–#950 (+#935/#929/#930) deferred** — spec 2026-06-04 §6 mandates one PR per phase, and Phase 2's telemetry parity needs Phase 3's Session merge. Rejected: bundling them half-done into the overnight PR.
- #918: shipped without syntect (AC allowed); `o` escape hatch = shell at worktree via ShellLauncher, not $EDITOR suspend.

**Next-session priorities:** merge-gate PR #991 manual QA; then #947 → #948 → #949 → #950 in separate PRs; verify chat renderer sanitizes `StreamEvent::Unknown.raw` (security review informational).
