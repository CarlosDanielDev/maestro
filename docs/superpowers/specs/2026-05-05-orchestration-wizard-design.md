# Orchestration Wizard — Design

**Date:** 2026-05-05
**Status:** Draft (post-brainstorming, pre-implementation-plan)
**Author:** Carlos Daniel (with Claude Opus 4.7 in brainstorming mode)
**Scope:** Sibling milestone to v0.25.0 multi-agent. Adds the coordination layer that uses the providers v0.25.0 ships.

## 1. Summary

A new layer on top of v0.25.0 multi-agent that lets the user compose **teams** of AI agents (mixing roles with provider backends), save them as portable **presets**, and **launch** them on issues or milestone slices with dependency-aware scheduling.

The top-level orchestrator is intentionally minimal-context: it routes and supervises, it does NOT read issue bodies or reason about content. Reading and reasoning happen in isolated subagent sessions on whichever providers the team binds them to.

## 2. Locked design decisions (from brainstorming)

| # | Question | Pick | Implication |
|---|---|---|---|
| Q1 | Team launch model | **B** — orchestrator + subagent routing | Maestro intercepts subagent dispatch; one main session at top, N subagent sessions on possibly-different providers |
| Q2 | Data model richness | **C** — layered | Minimal default (`role = "agent_id"`); optional `[role_overrides.<role>]` sub-table for richness |
| Q3 | Scheduler placement | **C** — hybrid | Rust owns cross-issue scheduling; LLM-per-issue owns per-issue supervision |
| Q4 | Purpose taxonomy | **C** — closed primitives + open compositions | Four primitives (pipeline, fan-out, single-pass, verdict-only) typed in Rust; user compositions on top |
| Q5 | Persistence | **C** — tiered | Built-in (binary) + user (`~/.config/maestro/teams/`) + project (`.maestro/teams/` ∪ inline `[teams.*]` in `maestro.toml`) |
| Q6 | Modes interaction | **B** — teams reference modes | Teams bind `(agent, mode, optional overrides)` per role; `[modes.*]` keeps working unchanged for solo sessions |

## 3. Architecture

Three layers:

```
┌────────────────────────────────────────────────────────────────┐
│ Layer 3: Cross-issue scheduler (Rust, in maestro)              │
│ Reads ## Blocked By per issue → topo-sorts → dispatches        │
│ per-issue runs respecting the dependency graph + concurrency.  │
│ Owns: launch plan, retry policy, worktree lifecycle.           │
└──────────────────────────┬─────────────────────────────────────┘
                           │ spawns one per leaf-ready issue
                           ▼
┌────────────────────────────────────────────────────────────────┐
│ Layer 2: Per-issue LLM orchestrator (Claude/etc, scope=1 issue)│
│ Tight system prompt: "delegate, do not read the issue body."   │
│ Calls subagents via Task() tool with role tags. Bounded ctx.   │
│ Owns: per-issue task graph, subagent supervision.              │
└──────────────────────────┬─────────────────────────────────────┘
                           │ Task(role=Reviewer, ...)
                           ▼
┌────────────────────────────────────────────────────────────────┐
│ Layer 1: Subagent dispatch (maestro intercepts)                │
│ Looks up team's role→agent binding, spawns provider session    │
│ with the bound mode, returns structured result to L2.          │
│ Owns: per-role provider routing, mode resolution.              │
└────────────────────────────────────────────────────────────────┘
```

### New code surface

| Path | Purpose |
|---|---|
| `src/orchestration/` (new) | L3 scheduler, primitive runners, team resolver |
| `src/orchestration/primitives/` (new) | One Rust file per primitive; each owns a `run()` state machine |
| `src/config/teams.rs` (new) | Three-tier loader (built-in → user → project), `extends` resolution, validation |
| `src/tui/screens/team_wizard/` (new) | Compose + launch wizard; mirrors `issue_wizard` / `milestone_wizard` pattern |
| `src/tui/agent_graph/model.rs` (extend) | Third node kind for team membership |
| `src/state/types.rs` (extend) | `TeamRun` and `IssueRunState` records |
| `src/cli/team.rs` (new) | `maestro team {new,launch,manage}` subcommands |

### What does NOT change

- `[modes.*]` schema and behavior — teams reference modes by name.
- Solo-session launch flow — runs without a team, exactly as today.
- v0.25.0 `[agents.*]` — teams consume `agent_id` from there; no schema overlap.
- Worktree lifecycle — one worktree per issue; cross-issue worktrees isolated; existing cleanup rules apply.

## 4. Data model

### Primitives (Rust, closed enum)

```rust
// src/orchestration/primitives/types.rs
pub enum Primitive {
    Pipeline,     // roles run sequentially; output[N] feeds input[N+1]; final → PR/commit
    FanOut,       // roles run in parallel; orchestrator merges; single final output
    SinglePass,   // one role; one shot; typed output
    VerdictOnly,  // N roles contribute; orchestrator returns typed JSON verdict (no code change)
}

pub enum TeamInput {
    Issue { number: u64 },
    MilestoneSlice { milestone: u64, issues: Vec<u64> },
    IdeaInbox,
    FileSet(Vec<PathBuf>),
}

pub enum TeamOutput {
    Pr        { number: u64, branch: String },
    NewIssues { numbers: Vec<u64> },
    Comment   { issue: u64, body: String },
    AdrDraft  { path: PathBuf },
    Verdict   { json: serde_json::Value },
    Commit    { sha: String, branch: String },
}
```

Each primitive declares which `TeamInput` and `TeamOutput` variants it accepts. Layer 2 receives a state machine derived from the primitive — that's how the LLM orchestrator's scope stays bounded.

### Team preset schema (TOML)

**Minimal form:**
```toml
# ~/.config/maestro/teams/cheap-coder.toml
extends     = "default-coder"     # → built-in
implementer = "ollama"
reviewer    = "opencode"
docs        = "minimax"
```

**Rich form (optional `role_overrides.<role>`):**
```toml
# .maestro/teams/sec-focused-coder.toml
extends = "cheap-coder"           # chain: project → user → built-in

[role_overrides.reviewer]
agent           = "opencode"
mode            = "review-strict"     # → resolves into [modes.review-strict]
model_override  = "openrouter/deepseek/deepseek-coder-v2"
prompt_addendum = "Be terse. Focus on CWE-78 and CWE-89."
fallback_agent  = "claude"            # used by L2's retry_with_different_agent
```

**Inline in `maestro.toml`:**
```toml
[teams.this-repo-coder]
extends     = "default-coder"
implementer = "ollama"
```

### Tier resolution

1. Load built-ins from binary-embedded TOML (`include_str!`).
2. Load user tier: `~/.config/maestro/teams/*.toml` (alphabetical; last-wins per-name within tier).
3. Load project tier: `.maestro/teams/*.toml` ∪ `[teams.*]` in `maestro.toml`.
4. Resolution priority on duplicate names: project > user > built-in.
5. `extends` chains must form a DAG (cycle → hard error at load time).
6. Cross-tier extension is allowed (project file extends a built-in by name).
7. Result: `HashMap<String, ResolvedTeam>` where every binding is merged through its `extends` chain and validated against its primitive's contract.

### Built-in seed list (v1, ship 5)

| Built-in | Primitive | Roles bound (default = claude) | Output |
|---|---|---|---|
| `default-coder` | `pipeline` | Implementer → Reviewer → Docs | `Pr` |
| `default-researcher` | `verdict-only` | Implementer + Reviewer (parallel) | `Verdict` (idea-quality JSON) + optional `NewIssues` |
| `default-triager` | `verdict-only` | wraps existing `subagent-idea-triager` flow | `Verdict` (promote/park/archive JSON) |
| `default-reviewer` | `single-pass` | Reviewer | `Comment` |
| `default-docs` | `single-pass` | Docs | `Commit` |

Each ships as a binary-embedded TOML and is rendered in `docs/teams/` as the canonical reference.

### Validation rules (load-time, hard errors)

- Every `agent` reference must exist as an enabled `[agents.<id>]` (from v0.25.0 #549).
- Every `mode` reference must exist as `[modes.<id>]`.
- Every required role for the chosen `Primitive` must be bound.
- `extends` cycles → error.
- `model_override` is validated against the agent's known model list when the agent declares one.
- `min_agents` (built-in field): the loader refuses to mark a team as runnable if any agent in `min_agents` is missing or unhealthy on the current machine. This is what makes `default-coder` a portable starting point — it requires only `claude`.

### Cross-platform path resolution

Use the `directories` crate (or whichever crate maestro already pulls in) for the user tier:
- Linux: `$XDG_CONFIG_HOME/maestro/teams/` → `~/.config/maestro/teams/`
- macOS: `~/Library/Application Support/maestro/teams/`
- Windows: `%APPDATA%\maestro\teams\`

Snapshot test asserts the resolver picks the right path per OS.

## 5. Execution flow

### L3 — Cross-issue scheduler

```
INPUT:  TeamInput (typically MilestoneSlice or single Issue)
        + Team { name, primitive, role_bindings, ... }

STEP 1  Fetch each issue's body via gh; parse `## Blocked By` section.
STEP 2  Edge classification:
          IN-SLICE         → real edge in DAG
          CLOSED-EXTERNAL  → drop (dep already satisfied)
          OPEN-EXTERNAL    → block; surface to user
STEP 3  Cycle check (Tarjan SCC). Any SCC > 1 node → hard error.
STEP 4  OPEN-EXTERNAL handling. Default: same-milestone → auto-add to selection;
        cross-milestone → cancel and ask.
STEP 5  Topological levels (Kahn's algorithm).
STEP 6  Concurrency packing (default cap = 3, configurable via
        [concurrency.team_max_parallel]).
STEP 7  Render plan in wizard preview. User confirms.
STEP 8  For each level: spawn ≤ cap Layer-2 runs. On Layer-2 result:
          Success → mark issue done; if level done → unlock next level.
          Failure → mark issue failed; downstream stays blocked.
STEP 9  Persist a single TeamRun record with sub-records per issue.
```

**Concurrency:** obeys existing `[concurrency]` config + new optional `[concurrency.team_max_parallel]` cap (default 3).

**Failure semantics:** scheduler does NOT auto-retry across issues. A failed issue blocks downstream until human intervenes via the dashboard.

**Live re-evaluation:**
- Issue closes successfully → re-run STEP 2 to unblock peers.
- New `Blocked By` appears mid-run → no auto-react; user hits `r` (re-plan) on the dashboard, which re-runs STEP 1–6 against the pending set.

### L2 — Per-issue LLM orchestrator

```
SYSTEM PROMPT (compiled from primitive + team):
  You orchestrate a `<primitive>` team for issue #N. You do NOT read the issue body.
  Your only verbs are: Task(role=...), GhPrCreate (pipeline only), ReportFailure(reason).
  After each Task returns, decide: continue / retry-different-provider / fail.
  Output a single typed result when the primitive's state machine reaches the terminal state.

TOOLS AVAILABLE:
  - Task(role, instructions)       ← only verb that does work
  - GhPrCreate                     ← only at end of pipeline
  - ReportFailure(reason)
  Forbidden: Read, Edit, Write, Bash, Grep — all delegated to subagents.
```

**Cost discipline:** L2 system prompt ≈ 200 tokens. Each `Task()` returns a structured summary ≤ 300 tokens. Typical 3-round issue: ~2-3k tokens at L2 regardless of issue size. The "minimal-context orchestrator" rule's explicit cost knob.

**L2 is mandatory per issue, even for `single-pass` primitives.** Uniform supervision (retry decisions, malformed-result handling) is worth the ~2k-token tax.

### L1 — Subagent dispatch

When L2 calls `Task(role=Reviewer, instructions=...)`:

```
1. Look up active team's binding for `Reviewer`.
2. Resolve [agents.<agent_id>] (from v0.25.0 #549).
3. Resolve [modes.<mode>] for system prompt + tools + permissions.
4. Spawn a real provider session (existing v0.25.0 ManagedSession path).
5. Stream events through the appropriate parser (#552 / #652).
6. Capture structured result typed by the role's contract.
7. Return to L2 as the Task() return value.
```

**Subagent isolation:** each subagent session sees only its own system prompt + L2's instructions for that one Task() call. Subagents do NOT see other subagents' prompts or outputs. L2 is the only thing with the "whole picture" — and it's bounded to one issue.

**Provider transparency:** L2 never knows what provider Reviewer ran on; Task() returns are normalized.

## 6. Multi-issue dependency planning

### Source of truth: `## Blocked By`

Per `.claude/CLAUDE.md` §4, every issue MUST carry a `## Blocked By` section. The scheduler reads it directly — no separate "deps" config.

### DAG construction

See L3's STEP 1–6 above. Three notes:

1. **STEP 4's default for same-milestone OPEN-EXTERNAL** is "auto-add to selection." Friendly but can balloon a "I picked 3 issues" launch into a 12-issue run via transitive deps. Conservative alternative: always cancel and ask. Default chosen for ergonomics.
2. **Plan rendering** annotates issues: ✅ closed (informational), 🔒 waiting, ⚠ ignored OPEN-EXTERNAL, ❌ pre-flight fail. ❌ disables the confirm button until resolved.
3. **The scheduler doesn't infer non-`Blocked By` deps** (file overlap, branch conflicts). Worktrees handle file isolation; PR queues handle branch ordering. The team layer trusts those existing mechanisms.

## 7. Wizard UX

### Entry points

| Trigger | Lands on |
|---|---|
| TUI top menu — new entry `[t] Teams` | Wizard home (Compose vs Launch vs Manage) |
| Issue browser, key `t` on a row | Launch flow → issue pre-selected |
| Milestone screen, key `t` | Launch flow → milestone pre-selected (issues pre-checked from graph) |
| CLI `maestro team launch <preset> --issue N` | Launch flow's confirm screen, headless |
| CLI `maestro team new` | Compose flow, step 1 |

### Compose flow (mirrors `milestone_wizard` pattern)

File layout: `src/tui/screens/team_wizard/{mod.rs,draw.rs,types.rs,ai_suggest.rs}` — exact match to existing wizards.

State machine steps:
1. **Source** — start blank or extend an existing preset.
2. **Primitive** — pick `pipeline` / `fan-out` / `single-pass` / `verdict-only`. Locked if extending.
3. **Roles** — for each required role, pick `agent_id` from a doctor-filtered dropdown.
4. **Overrides (optional)** — for any role, expand to set `mode`, `model_override`, `prompt_addendum`, `fallback_agent`. Skip-able with `n`.
5. **Save** — pick name, pick tier (user vs project), confirm. Validation runs here.

### Launch flow

1. **Team** — pick a preset. Defaults to most-recently-launched on this project.
2. **Input** — primitive determines the picker.
3. **Plan preview** — render the DAG with levels, concurrency cap, cost estimate.
4. **Confirm + Execute** — fires off Layer-3 scheduler. Wizard hands off to dashboard.

### Manage flow (in v1)

Edit / delete user-tier presets through the TUI. Could ship as v2 with v1 doing "edit by hand," but included for completeness — costs ~30% of the wizard TUI.

### Cost estimate (in v1)

Internal "estimated tokens per primitive run" table per provider. Approximate; clearly labeled as such.

### Empty state / first-run

User installs `maestro` fresh. Wizard home shows `Active presets: 5 built-in`. Compose → Source step shows the 5 built-ins as extension targets. Launch → Team step shows them as runnable (assuming `default-coder`'s `min_agents = ["claude"]` is satisfied). User can launch a team on minute one without any team config.

### Keybindings

- `t` — open Teams wizard from any screen that lists issues/milestones.
- Inside Teams wizard: `c/l/m` mode switch, `Tab` cycle, `Enter` next, `b` back, `n` skip-optional, `q` cancel. Mirrors existing wizard verbs.

## 8. Failure modes and recovery

### Failure taxonomy

| Layer | Failure class | Example |
|---|---|---|
| Pre-flight | Validation | `extends` cycle; agent disabled; missing `mode` ref; OPEN-EXTERNAL dep |
| L1 | Provider | Ollama not running, MiniMax 401, Codex non-zero exit, parser malformed |
| L1 | Result | Tool returned, but structured payload doesn't match the role's expected shape |
| L2 | Self-error | LLM session errored, hit max-iterations, returned malformed final output |
| L3 | Plan-stale | An in-flight issue's deps changed; OPEN-EXTERNAL disappeared |
| External | Infra | GitHub API down, gh auth invalid, worktree creation failed |

### Recovery verbs

**L1:** retry-once on transient errors (connection refused, 429, timeout); hard errors fail immediately. Result is a typed `SubagentError` returned to L2.

**L2:** LLM-driven recovery within issue. Three responses to a failed `Task()`:
- `retry_with_different_agent(role)` → re-dispatches to the role's `fallback_agent` binding.
- `retry_with_clarification(role, hint)` → same agent, augmented prompt.
- `fail_up(reason)` → surrender this issue, propagate to L3.
Budget: max 3 recovery attempts per role per issue, then auto fail_up.

**L3:** issue fails → mark `failed` in state store; downstream issues stay `blocked` (status, not failed). No auto-retry. Notify and surface to dashboard.

**Human:** dashboard verbs on a failed TeamRun row:
- `[r] retry` — re-launch this issue, clean slate.
- `[s] swap-team` — pick a different preset and retry.
- `[u] unblock-downstream` — mark issue as "skip", unlock peers (last-resort).
- `[a] abort-run` — terminate the entire TeamRun, clean worktrees.

### Persisted state

Extend `src/state/types.rs`:

```rust
pub struct TeamRun {
    pub id: Uuid,
    pub team_name: String,
    pub started_at: DateTime<Utc>,
    pub plan: Vec<Vec<IssueNumber>>,           // levels
    pub state: HashMap<IssueNumber, IssueRunState>,
}

pub enum IssueRunState {
    Queued,
    InFlight { session_id: Uuid, started_at: DateTime<Utc> },
    Succeeded { output: TeamOutput },
    Failed { reason: String, attempts: u8 },
    Blocked { blocking: Vec<IssueNumber> },
}
```

On startup, maestro reads open TeamRuns, reconciles `InFlight` records with running session PIDs (existing pattern), and either resumes or marks orphans as `Failed`. Idempotency: a TeamRun in any state can be re-opened on the dashboard; verbs are always available.

### Pre-flight is sacred

All validation runs before Layer 3 launches the first issue:
- TOML cycle check.
- Every `agent_id` resolves and is healthy (call `doctor` once).
- Every `mode` resolves.
- Every required role for the primitive is bound.
- Every selected issue has parseable `## Blocked By` (or "None").
- DAG is acyclic.

A pre-flight failure produces a single human-readable diagnostic with the offending field path. The user can fix and re-launch without losing the issue selection.

### Out of scope

- Auto-recovery from infrastructure outages (GitHub API down) — surfaced; no auto-retry.
- Cross-team learning — failures in TeamRun A don't influence team config B.
- Predictive failure prevention — no "we predict reviewer will fail." Empirical only: fallback fires on actual failure.

## 9. Testing strategy

| Layer | Test type | Coverage |
|---|---|---|
| Tier loader | Unit + insta snapshot | Built-in TOML parses, user-tier path resolution per OS, `extends` chain merge, cycle detection rejects |
| Primitives | Unit | Each primitive's state machine: success path, every recovery branch, max-iteration cap |
| L3 scheduler | Unit + property | Topo-sort correctness over generated DAGs, OPEN-EXTERNAL classification, level packing under varying concurrency caps |
| L2 orchestrator | Integration with mock providers | LLM orchestrator runs end-to-end against a fake `Task()` tool returning canned subagent results; assert state-machine transitions |
| L1 dispatch | Integration | Mock provider sessions; verify mode resolution, prompt assembly, structured-result capture |
| Wizard | TUI snapshot tests | All compose-flow steps, all launch-flow steps, every error inline, narrow-terminal truncation, Manage screen |
| State store | Unit + insta | TeamRun JSON round-trip, `IssueRunState` transitions, restart reconciliation |

### Fixtures (committed)

- `tests/fixtures/teams/` — five built-in TOML files duplicated for golden testing.
- `tests/fixtures/blocked_by/` — sample issue bodies covering: `None`, single dep, multi dep, malformed, missing.
- `tests/fixtures/team_runs/` — pre-built `TeamRun` JSON for restart-reconciliation tests.

### Snapshot policy (insta, per Rust guardrails §7)

- Every wizard screen: 80×24, 60×20, 120×40.
- Every dashboard team-row state: ✅ / ❌ / 🔒 / ⚠ / in-flight.
- Plan preview at 3 levels with mixed states.

### Merge gates

- `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` (mandatory).
- All five built-in team TOML files MUST parse, validate, and resolve cleanly on a fresh machine — CI smoke test.
- Cycle-detection fixtures in `tests/fixtures/teams/cycles/` MUST all reject.
- Pre-flight failure messages MUST be human-readable (no `{:?}` debug spew); enforced by snapshot test on each error variant.

### Out of scope

- Real-provider integration tests (cost-prohibitive; reserved for `maestro doctor`).
- E2E "team launches against a live GitHub repo" (manual smoke before each release).
- Fuzz testing of LLM outputs (canned bad payloads in fixtures, not generative fuzzing).

## 10. Dependencies on other work

- **Hard:** v0.25.0 must close (or at least #547 + #549 + the providers used in built-ins) before this work can land. Built-in `default-coder` requires `[agents.claude]`, which is v0.25.0 #549.
- **Soft:** v0.25.0 #550 (doctor) makes the wizard's "filter to enabled-and-healthy agents" step nicer; without it, the wizard would have to run health checks itself.
- **None on v0.26.0** (CI quality gates).

## 11. Decisions deferred

The brainstorming session resolved six load-bearing forks. The following remain open and are explicitly deferred to the implementation plan:

- **Naming conventions** for primitive verbs in L2's tool list (`Task` vs `Dispatch` vs `Delegate`). Bikeshed; pick during plan writing.
- **Cost estimate computation** — exact formula and per-provider tables. Approximate; refine in implementation.
- **`maestro team manage` UX details** — exact screen layout. Mirrors existing TUI patterns.
- **Migration story** for users who already maintain `[modes.*]` configs and want to "lift" them into team presets. Likely an `assist` slash command in v2.
- **Discord / external notification integration** for TeamRun status — out of v1 scope; reuse existing `notifications` system.

## 12. Risks

1. **L2 cost discipline drifts** — if "minimal-context orchestrator" turns into "actually I read the issue body to make a decision," L2's token use balloons. Mitigation: tool-set restriction at the API level (no Read/Edit/Write/Bash/Grep), max-iteration cap, and a per-TeamRun cost telemetry that flags outliers.
2. **`extends` chain debugging** — when a team behaves unexpectedly, tracing why a binding came out the way it did across three tiers and inheritance can be painful. Mitigation: `maestro team explain <name>` CLI command that prints the resolved table with provenance per field.
3. **Cross-issue dep cascades** — STEP 4's auto-add default can balloon scope unexpectedly. Mitigation: plan preview always shows the final selection count vs the user's original count; visual delta.
4. **Built-in drift on maestro upgrade** — if `default-coder` changes between releases, user presets that `extends = "default-coder"` may break silently. Mitigation: built-ins versioned (`default-coder@v1`); user presets pin a version; upgrade path warns when pin is stale.
5. **Provider-specific failure modes leaking past L1** — e.g., Ollama returning `model_not_pulled` as a 404 vs Claude returning a tool-error format. L1 normalizes, but novel providers may surface unfamiliar errors. Mitigation: the `SubagentError` enum is exhaustive over known cases; `Other(String)` catches the rest with full preserved context for debugging.

## 13. Out of scope (v1)

- Real-time team coordination (subagents talking to each other mid-run).
- Cross-machine team execution (a team running across two machines via SSH).
- Team marketplaces / preset sharing via a registry.
- Team-aware subagent learning (memory persistence per team across runs).
- Auto-tuning team bindings based on past success rates.

## 14. Open questions for the user

1. Do you want the `Manage` mode of the wizard in v1, or push to v2?
2. Do you want the cost estimate in the plan preview in v1, or push to v2?
3. STEP 4's auto-add default for same-milestone OPEN-EXTERNAL deps — keep or flip to "always cancel and ask"?

(All three were tentatively answered "yes / yes / keep" during brainstorming based on user's "go" responses; reconfirm here so they get captured in the plan.)
