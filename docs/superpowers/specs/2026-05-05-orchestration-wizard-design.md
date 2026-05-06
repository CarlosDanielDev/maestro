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

### L2 provider constraint (load-bearing)

**L2 MUST run on a provider that supports structured tool-call dispatch.** In v1, this means **Claude only** — `Task()` is a Claude Code primitive, not a generic LLM API concept. Codex / Qwen / OpenCode / Ollama / MiniMax cannot host L2 in v1; they can host subagents (L1) freely.

L2's provider is independent of the team's role bindings. A team can bind every subagent role to a non-Claude provider, but L2 itself always spawns a Claude session.

Concrete rules:

- Every built-in preset declares `min_agents = ["claude", ...]` — Claude is mandatory for any team to be runnable.
- `doctor` reports "team system unavailable" if no `claude` agent is healthy, and the team wizard refuses to enter the launch flow.
- The pre-flight failure taxonomy (Section 8) gains: `L2ProviderUnavailable` — surfaced when no Claude agent is healthy.
- A future `[orchestrator]` config key (out of v1 scope, see §13) will allow configuring L2's provider once a non-Claude provider with comparable tool-call semantics ships.

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
    /// Set of issues, possibly spanning multiple milestones. The scheduler fetches
    /// each issue's milestone at edge-classification time (Section 5 STEP 2) to
    /// classify deps as same-milestone vs cross-milestone. The `primary_milestone`
    /// field is the milestone the user *selected from* in the wizard — it's the
    /// reference point for "same-milestone" auto-add behavior in STEP 4.
    IssueSet { primary_milestone: Option<u64>, issues: Vec<u64> },
    IdeaInbox,
    // FileSet variant deferred to v2 (see §13).
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

### Role output contracts (load-bearing)

Each role produces a typed return value when its `Task()` call completes. L1 enforces the contract; L2 trusts it. Without these contracts the "Task() returns ≤ 300 token structured summary" cost discipline and the "L1 SubagentError when payload doesn't match shape" failure class are both undefined.

```rust
// src/orchestration/contracts.rs
pub enum SubagentResult {
    /// Implementer produced a code change.
    CodeChange {
        files_touched: Vec<PathBuf>,
        summary: String,        // ≤ 200 tokens
        commit_sha: Option<String>,
    },
    /// Reviewer produced findings on a diff or PR.
    ReviewFindings {
        verdict: ReviewVerdict, // Approved | RequestChanges | Comment
        findings: Vec<Finding>, // each ≤ 50 tokens; total cap = 8 findings
    },
    /// Docs produced documentation changes.
    DocsChange {
        files_touched: Vec<PathBuf>,
        summary: String,        // ≤ 100 tokens
    },
    /// Triager / Researcher returned a verdict.
    Verdict {
        decision: String,       // primitive defines the enum (e.g., promote|park|archive)
        rationale: String,      // ≤ 150 tokens
        new_issues: Vec<NewIssueDraft>,  // optional; empty = no new issues
    },
    /// Generic single-pass result that doesn't fit the above.
    Generic { json: serde_json::Value },  // schema validated against role's declared shape
}
```

Each `Role` declares which `SubagentResult` variants it can produce. L1 returns `SubagentError::ResultShapeMismatch { expected, got }` if the provider's output cannot be parsed into the declared variants for that role. The full schema lives at `src/orchestration/contracts.rs`; this enum is the v1 surface and may grow as new primitives are added.

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

**Built-in (binary-embedded), shown for reference:**
```toml
# include_str!("teams/default-coder.toml")
extends     = ""              # no parent — built-in is a root
primitive   = "pipeline"
min_agents  = ["claude"]      # team-system-wide: every team needs claude (L2)
                              # plus the built-in's actual binding agents
implementer = "claude"
reviewer    = "claude"
docs        = "claude"
```

`min_agents` must include `"claude"` on every built-in (and is enforced for user/project teams too, per §3 L2 constraint). Additional entries declare which subagent agents must also be healthy for the team to be runnable.

### Tier resolution

1. Load built-ins from binary-embedded TOML (`include_str!`).
2. Load user tier: `~/.config/maestro/teams/*.toml` (alphabetical; last-wins per-name within tier).
3. Load project tier: `.maestro/teams/*.toml` ∪ `[teams.*]` in `maestro.toml`.
4. Resolution priority on duplicate names: project > user > built-in. **Name collision is whole-entry replacement, not field-level merge** — when two tiers define the same preset name, the higher-priority tier wins entirely; the lower-priority entry is discarded *before* `extends` resolution. The winning entry's `extends` chain then resolves against the post-collision name map. There is no field-level merge across tiers on collision.
5. `extends` chains must form a DAG (cycle → hard error at load time).
6. Cross-tier extension is allowed (project file extends a built-in by name). Resolution of `extends = "X"` uses the post-collision name map: the `X` that wins under rule 4 is the one used.
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

STEP 1  Fetch each selected issue's body AND milestone via
        `gh issue view <n> --json body,milestone`. Parse `## Blocked By`.
STEP 2  Edge classification. For each dep `d`:
          - if `d` ∈ selected set        → IN-SLICE (real edge in DAG)
          - else fetch `d` via gh issue view → milestone + state. Then:
              - state = CLOSED            → CLOSED-EXTERNAL (drop edge)
              - state = OPEN, milestone == primary_milestone
                                          → SAME-MILESTONE-OPEN-EXTERNAL
              - state = OPEN, milestone != primary_milestone (or no milestone)
                                          → CROSS-MILESTONE-OPEN-EXTERNAL
STEP 3  Cycle check (Tarjan SCC). Any SCC > 1 node → hard error.
STEP 4  OPEN-EXTERNAL handling:
          - SAME-MILESTONE-OPEN-EXTERNAL  → default: auto-expand the selection
            to include `d`. The expansion is shown to the user in the plan
            preview as a diff: "selected 3 issues, run will execute 5 (auto-added
            #X #Y as same-milestone deps)."
          - CROSS-MILESTONE-OPEN-EXTERNAL → cancel-and-ask (no silent expansion
            across milestones).
        Auto-expansion is bounded: if expansion adds > 2x the original selection
        count, the wizard surfaces a confirmation prompt instead of expanding
        silently. Expansion does NOT recurse — auto-added issues are not
        re-fetched for their own deps; user must re-run if they want transitive
        closure. Re-fetching is cheap (one gh call per added issue) but the
        recursion bound prevents balloon scenarios.
STEP 5  Topological levels (Kahn's algorithm).
STEP 6  Concurrency model — simple semaphore (NOT bin-packing). The cap
        `[concurrency.team_max_parallel]` (default 3) is the maximum number of
        in-flight Layer-2 runs at any moment. Within a level, issues launch as
        slots free. The plan preview renders levels purely for dependency
        visualization; actual execution is "as-soon-as-allowed" within the cap.
STEP 7  Render plan in wizard preview. User confirms.
STEP 8  For each level (in order), spawn issues into the global semaphore.
        On Layer-2 result:
          Success → mark issue done; if all peers in level done → unlock next.
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

**Subagent isolation (refined claim):** each subagent session sees only its own system prompt + L2's instructions for that one Task() call. Subagent sessions do NOT see each other's prompts or raw outputs directly. **However, L2 may include a brief structured summary of prior subagent results in the next subagent's instructions** (e.g., "Implementer's diff touched files X, Y, Z; review with focus on Z's regex"). This is necessary to make pipelines work. The summary is bounded by the role's `SubagentResult` contract — typically a `summary: String ≤ 200 tokens` field — and never includes raw issue bodies, diffs, or other subagent prompts. Net effect: subagents are sandboxed but not fully blind; L2 is the only entity with the whole picture, and even it never reads issue bodies directly.

**Provider transparency:** L2 never knows what provider Reviewer ran on; Task() returns are normalized.

### IdeaInbox runtime model (`subagent-idea-triager` interop)

The existing `subagent-idea-triager` lives in `.claude/agents/` — it's a Claude Code subagent definition, not a v0.25.0 `[agents.*]` provider entry. These are two different runtime models:

- v0.25.0 `[agents.<id>]` is a process/HTTP target (`claude` binary, Ollama HTTP, etc.).
- `.claude/agents/<name>.md` is a Claude Code system-prompt agent loaded at runtime by a Claude session.

When `default-triager` runs with `TeamInput::IdeaInbox`, L1 dispatch resolves the bound agent for the triager role (typically `claude`), then **the spawned Claude session is responsible for invoking the `.claude/agents/subagent-idea-triager` definition via Claude Code's existing subagent mechanism**. The team layer does NOT directly load `.claude/agents/` markdown files; that remains Claude Code's domain.

This means: `default-triager` requires `claude` in `min_agents` not just because L2 is Claude (every team requires that), but because the triager subagent itself is Claude-Code-bound. Built-in presets that wrap existing `.claude/agents/` flows must declare this in their description.

## 6. Multi-issue dependency planning

### Source of truth: `## Blocked By`

Per `.claude/CLAUDE.md` §4, every issue MUST carry a `## Blocked By` section. The scheduler reads it directly — no separate "deps" config.

### DAG construction

See L3's STEP 1–6 above. Three notes:

1. **STEP 4's default for same-milestone OPEN-EXTERNAL** is "auto-add to selection." Friendly but can balloon a "I picked 3 issues" launch into a 12-issue run via transitive deps. Conservative alternative: always cancel and ask. Default chosen for ergonomics.
2. **Plan rendering** annotates issues: ✅ closed (informational), 🔒 waiting, ⚠ ignored OPEN-EXTERNAL, ❌ pre-flight fail. ❌ disables the confirm button until resolved.
3. **The scheduler doesn't infer non-`Blocked By` deps** (file overlap, branch conflicts). Worktrees handle *file* isolation at run time — each issue runs in its own worktree, so concurrent edits don't corrupt the working tree. **PR-merge sequencing is a separate concern.** Maestro's existing per-session PR queue is per-session; for team runs, PRs from the same level may conflict at merge time when GitHub auto-merge attempts to land them. v1 punts on this: all team-run PRs land via the existing per-session queue, which serializes them. If two PRs from the same level conflict, the second one fails CI and the user resolves manually. A team-aware merge-train scheduler is explicitly out of v1 scope (see §13).

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

### Cost estimate (in v1, formula locked)

```
estimate_tokens(team, primitive, num_issues) =
    num_issues * (
        L2_system_prompt_tokens                                // ~200, computed at team resolution
      + sum_over_roles(role_system_prompt_tokens)              // from [modes.*]; computed at resolution
      + AVG_ISSUE_CONTEXT_TOKENS_PER_PROVIDER                  // static const, default 800
      + 300 * num_required_roles * RECOVERY_BUDGET             // RECOVERY_BUDGET = 3 (max attempts)
    )

estimate_cost($) = sum_over_providers(
    estimate_tokens_routed_to(provider) * COST_PER_TOKEN[provider]
)
```

`COST_PER_TOKEN[provider]` is a static `HashMap<AgentKind, f64>` in source (one entry per v0.25.0 provider; Ollama is `0.0`; updated only on maestro release). The result is rendered in the plan preview as **"≈ $X.XX (rough estimate, ±50%)"** — explicitly labeled as approximate. No runtime fetching of provider pricing.

Rationale: locking the formula now prevents the implementation from inventing one under time pressure and guarantees the displayed number has a documented derivation. The ±50% label is honest about variance without requiring real telemetry to refine.

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

**Restart reconciliation — pessimistic.** On startup, maestro reads open TeamRuns. Any `InFlight` record is **immediately promoted to `Failed { reason: "process state lost across restart" }`**. We do NOT scan the OS for matching PIDs and we do NOT attempt to resume a session whose in-memory `ManagedSession` is gone.

Rationale: `IssueRunState::InFlight` carries `session_id: Uuid`, not a PID, and `ManagedSession` (the runtime owner of the actual subprocess / HTTP client) is not persisted. Adding a `pid: Option<u32>` field plus a process scan would let us resume in some cases, but the failure modes (PID reused by an unrelated process, session hung in a non-recoverable state, partial worktree state) outweigh the resume benefit. The user gets a clear "the run was interrupted" surface and can hit `[r] retry` on the dashboard.

Idempotency: a TeamRun in any state can be re-opened on the dashboard; verbs are always available. `Failed` is recoverable via `[r] retry`, which clears the state and re-spawns the issue from the queue.

### Pre-flight is sacred

All validation runs before Layer 3 launches the first issue:
- TOML cycle check.
- Every `agent_id` resolves and is healthy. **Health checks are invoked as a library call, not as a `Command::new("maestro").arg("doctor")` subprocess.** Concretely: v0.25.0 #550's doctor module exposes `pub async fn run_health_check(providers: &[AgentProviderId]) -> Vec<AgentHealthCheck>` (the type already exists at `src/agent_provider/types.rs`). Pre-flight calls this directly. Rationale: subprocess-spawning would prevent reuse of already-resolved provider state, double the cold-start cost, and make the pre-flight untestable without process spawning. This adds a small reciprocal dependency on #550 which is acceptable since this work cannot land before v0.25.0 closes anyway.
- Every `mode` resolves.
- L2 provider is available (Claude is healthy; see §3 L2 constraint).
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
| L2 orchestrator | Integration with mock providers | LLM orchestrator runs end-to-end against a fake `Task()` tool returning canned subagent results; assert state-machine transitions. **Mock ownership:** the fake `Task()` lives in `tests/orchestration/mock_task.rs` and intercepts at the Claude provider layer (since `Task()` is Claude Code's tool primitive) — it does NOT replace `AgentProvider` itself; it replaces the L2 session's tool-result channel with canned payloads, so the test doesn't spawn a real Claude binary. |
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

### v0.25.0 (multi-agent)

- **Hard:** #547 (AgentProvider trait + ClaudeProvider) — L1 dispatch consumes this directly.
- **Hard:** #549 (multi-agent config) — every team binding's `agent_id` resolves through `[agents.<id>]` defined here.
- **Hard:** #550 (doctor) — pre-flight calls `run_health_check()` as a library function, not a subprocess. Promoted from soft to hard by review fix C4.
- **Soft:** #551 (TUI per-session agent selector) — the team wizard's "filter to enabled-and-healthy agents" step ideally reuses the dropdown widget #551 ships, but can be re-implemented if #551 slips.
- **Soft:** #552 / #652 (parser adapters) — L1 consumes whichever parser the bound agent uses; if any parser is stubbed at v0.25.0 close, the team that uses it inherits the stub.

### Cargo.toml additions

- `directories` — cross-platform user-config-path resolution (Linux XDG / macOS Application Support / Windows %APPDATA%). New dependency. Pin to a stable major version per Rust deps policy (no wildcards).

### Internal Maestro

- `[modes.*]` — referenced by team role bindings; no schema change.
- `[concurrency]` — extended with optional `team_max_parallel` (default 3); backward-compatible.
- `src/state/types.rs` — extended with `TeamRun` and `IssueRunState`; backward-compatible (new variants).

### NOT depended on

- v0.26.0 (CI quality gates) — independent.
- v0.24.1 (README hero polish) — independent.

## 11. Decisions deferred

The brainstorming session resolved six load-bearing forks. The following remain open and are explicitly deferred to the implementation plan:

- **Naming conventions** for primitive verbs in L2's tool list (`Task` vs `Dispatch` vs `Delegate`). Bikeshed; pick during plan writing.
- **`maestro team manage` UX details** — exact screen layout. Mirrors existing TUI patterns.
- **Migration story** for users who already maintain `[modes.*]` configs and want to "lift" them into team presets. Likely an `assist` slash command in v2.
- **Discord / external notification integration** for TeamRun status — out of v1 scope; reuse existing `notifications` system.
- **Built-in version pinning syntax** (`extends = "default-coder@v1"`) — the upgrade-warning mitigation in §12 risk #4 mentions this, but the TOML syntax for the version suffix is not specified here. Deferred to v2; in v1, all `extends` references are unversioned and built-in changes are documented as breaking in CHANGELOG.

## 12. Risks

1. **L2 cost discipline drifts** — if "minimal-context orchestrator" turns into "actually I read the issue body to make a decision," L2's token use balloons. Mitigation: tool-set restriction at the API level (no Read/Edit/Write/Bash/Grep), max-iteration cap, and a per-TeamRun cost telemetry that flags outliers.
2. **`extends` chain debugging** — when a team behaves unexpectedly, tracing why a binding came out the way it did across three tiers and inheritance can be painful. Mitigation: `maestro team explain <name>` CLI command that prints the resolved table with provenance per field.
3. **Cross-issue dep cascades** — STEP 4's auto-add default can balloon scope unexpectedly. Mitigation: plan preview always shows the final selection count vs the user's original count; visual delta.
4. **Built-in drift on maestro upgrade** — if `default-coder` changes between releases, user presets that `extends = "default-coder"` may break silently. Mitigation in v1: built-in changes that affect downstream `extends` semantics MUST be flagged in CHANGELOG as breaking, and the loader emits a `tracing::warn!` on every load that uses a built-in (so users see "you are inheriting from default-coder" in logs and can audit). Versioned pinning syntax (`default-coder@v1`) is deferred to v2 — see §11.
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
