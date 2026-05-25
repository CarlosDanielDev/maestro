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

## 2026-05-23 — v0.29.5 cross-milestone handoff bundle #806/#875/#876/#877

v0.29.5 bundle (user authorized PR-isolation override for context budget): one PR, four Closes refs, architect+QA blueprint per scope; disabled-agent filter (#806), Ctrl+V paste (#875), autocomplete (#876), LaunchTeam dispatch fan-out (#877); R3 (real run_team) was descoped to follow-up and landed in #881.
