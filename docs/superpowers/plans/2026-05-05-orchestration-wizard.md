# Orchestration Wizard Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a coordination layer on top of v0.25.0 multi-agent that lets users compose teams of AI agents (with role→provider routing), save them as portable presets, and launch them on issues or milestone slices with dependency-aware scheduling.

**Architecture:** Three layers. L3 (Rust, in `src/orchestration/`) is the cross-issue scheduler that reads `## Blocked By` from selected issues, builds a topological plan, and dispatches issue-level runs respecting concurrency and dependencies. L2 is a per-issue Claude session with a tightly-scoped system prompt that delegates work via `Task()` tool calls (Claude-only in v1). L1 (in maestro) intercepts each Task call, looks up the team's role→agent binding, and spawns the bound provider session via the v0.25.0 `ManagedSession` pipeline. Teams are TOML presets stored in three tiers (built-in binary-embedded, user `~/.config/maestro/teams/`, project `.maestro/teams/`) with `extends`-based inheritance.

**Tech Stack:** Rust 1.89+, tokio, ratatui, serde + serde_json, toml crate, `directories` crate (new dep — cross-platform user-config-path resolution), `insta` for snapshot tests, `wiremock` for HTTP test doubles. Builds on v0.25.0's `AgentProvider` trait and `[agents.*]` config.

**Spec reference:** `docs/superpowers/specs/2026-05-05-orchestration-wizard-design.md`.

**Hard dependencies:** v0.25.0 #547 + #549 + #550 must close before this plan starts. Soft: #551, #552/#652.

---

## File Structure

### New files

| Path | Chunk | Responsibility |
|------|-------|----------------|
| `src/orchestration/mod.rs` | 1 | Module root, re-exports |
| `src/orchestration/types.rs` | 1 | `Primitive`, `TeamInput`, `TeamOutput`, `Role` enums |
| `src/orchestration/contracts.rs` | 1 | `SubagentResult`, `SubagentError`, `Finding`, `ReviewVerdict`, `NewIssueDraft` |
| `src/orchestration/team.rs` | 1 | `TeamConfig`, `RoleBinding`, `RoleOverride` (TOML schema) |
| `src/orchestration/loader.rs` | 1 | Three-tier loader, `extends` resolution, cycle detection |
| `src/orchestration/validation.rs` | 1 | Load-time validation rules |
| `src/orchestration/builtins/mod.rs` | 1 | Embedded built-in TOML via `include_str!` |
| `src/orchestration/builtins/default-coder.toml` | 1 | Built-in pipeline preset |
| `src/orchestration/builtins/default-researcher.toml` | 1 | Built-in verdict-only preset |
| `src/orchestration/builtins/default-triager.toml` | 1 | Built-in (wraps `subagent-idea-triager`) |
| `src/orchestration/builtins/default-reviewer.toml` | 1 | Built-in single-pass preset |
| `src/orchestration/builtins/default-docs.toml` | 1 | Built-in single-pass preset |
| `src/orchestration/scheduler.rs` | 2 | L3 cross-issue scheduler |
| `src/orchestration/dag.rs` | 2 | DAG construction, edge classification, topo levels, cycle check |
| `src/orchestration/preflight.rs` | 2 | Pre-flight validation pipeline |
| `src/orchestration/run.rs` | 2 | `TeamRun`, `IssueRunState` lifecycle |
| `src/orchestration/orchestrator.rs` | 3 | L2 per-issue orchestrator (Claude session driver) |
| `src/orchestration/primitives/mod.rs` | 3 | Primitive registry |
| `src/orchestration/primitives/pipeline.rs` | 3 | `pipeline` state machine |
| `src/orchestration/primitives/fan_out.rs` | 3 | `fan-out` state machine |
| `src/orchestration/primitives/single_pass.rs` | 3 | `single-pass` state machine |
| `src/orchestration/primitives/verdict_only.rs` | 3 | `verdict-only` state machine |
| `src/orchestration/dispatch.rs` | 4 | L1 subagent dispatch |
| `src/orchestration/cost.rs` | 4 | Cost-estimate formula (locked per spec §7) |
| `src/tui/screens/team_wizard/mod.rs` | 5 | Wizard state machine |
| `src/tui/screens/team_wizard/draw.rs` | 5 | Wizard rendering |
| `src/tui/screens/team_wizard/types.rs` | 5 | Wizard payloads |
| `src/tui/screens/team_wizard/compose.rs` | 5 | Compose-flow steps |
| `src/tui/screens/team_wizard/launch.rs` | 5 | Launch-flow steps |
| `src/tui/screens/team_wizard/manage.rs` | 5 | Manage-flow screen |
| `src/cli/team.rs` | 6 | `maestro team {new,launch,manage,explain}` subcommands |
| `docs/teams/README.md` | 6 | Built-in team reference |
| `docs/teams/default-coder.md` | 6 | Per-built-in doc |
| `docs/teams/default-researcher.md` | 6 | (likewise) |
| `docs/teams/default-triager.md` | 6 | |
| `docs/teams/default-reviewer.md` | 6 | |
| `docs/teams/default-docs.md` | 6 | |
| `tests/fixtures/teams/cycle-a.toml` | 1 | Cycle test fixture |
| `tests/fixtures/teams/cycle-b.toml` | 1 | (cycle partner) |
| `tests/fixtures/teams/missing-mode.toml` | 1 | Validation test fixture |
| `tests/fixtures/blocked_by/none.md` | 2 | "Blocked By: - None" fixture |
| `tests/fixtures/blocked_by/single.md` | 2 | One dep |
| `tests/fixtures/blocked_by/multi.md` | 2 | Multiple deps |
| `tests/fixtures/blocked_by/malformed.md` | 2 | Malformed section |
| `tests/fixtures/blocked_by/missing.md` | 2 | Section absent |
| `tests/fixtures/team_runs/in_flight.json` | 2 | Restart-reconciliation fixture |
| `tests/fixtures/team_runs/all_done.json` | 2 | Completed run fixture |
| `tests/orchestration/mock_task.rs` | 3 | Mock `Task()` test helper |

### Modified files

| Path | Chunk | Change |
|------|-------|--------|
| `Cargo.toml` | 1 | Add `directories = "5"` (or pin current major) dependency |
| `src/lib.rs` | 1 | Add `pub mod orchestration;` and re-exports |
| `src/config/mod.rs` | 1 | Add `[teams.*]` inline table support; `[concurrency.team_max_parallel]` field |
| `src/state/types.rs` | 2 | Add `TeamRun` and `IssueRunState` enums |
| `src/state/store.rs` | 2 | Persist + load `TeamRun` records; restart reconciliation hook |
| `src/agent_provider/types.rs` | 4 | Pub-export `AgentHealthCheck` for orchestration's preflight |
| `src/commands/doctor.rs` | 4 | Expose `run_health_check()` as `pub async fn` library function |
| `src/tui/agent_graph/model.rs` | 5 | Add `NodeId::Team(Uuid)` variant; render team containers |
| `src/tui/agent_graph/render.rs` | 5 | Render team-membership edges |
| `src/tui/screens/issue_browser/mod.rs` | 5 | Add `t` keybinding → launch flow with issue pre-selected |
| `src/tui/screens/milestone/mod.rs` | 5 | Add `t` keybinding → launch flow with milestone pre-selected |
| `src/tui/app/mod.rs` | 5 | Wire team wizard into top-level navigation (`[t] Teams`) |
| `src/main.rs` | 6 | Wire `maestro team` subcommand into clap |
| `src/state/types.rs` | 6 | Bump persisted-state version; migration for pre-team-run state files |

### Chunk boundaries and dependencies

```
Chunk 1: Foundation — types, contracts, tier loader, built-in TOML files
    ↓
Chunk 2: L3 Scheduler — DAG, topo, run lifecycle, state-store integration
    ↓ (chunks 3 and 4 parallelizable)
Chunk 3: L2 Orchestrator + Primitives                      Chunk 4: L1 Dispatch + Cost
                                ↓
Chunk 5: TUI Wizard — compose / launch / manage flows
    ↓
Chunk 6: CLI + Built-in Docs + Smoke Test + State Migration
```

Each chunk → one PR (per user's isolation rule). Chunks 3 and 4 can land in parallel after Chunk 2.

---

## Chunk 1: Foundation

**Goal:** Establish the data model — primitive enum, contract types, TOML schema, three-tier loader with `extends` resolution, cycle detection, validation, and the five binary-embedded built-in presets. After this chunk, `cargo test` should be able to load every built-in and produce a `ResolvedTeam`.

**Files:**
- Create: `src/orchestration/{mod,types,contracts,team,loader,validation}.rs`
- Create: `src/orchestration/builtins/mod.rs` + 5 TOML files
- Modify: `Cargo.toml`, `src/lib.rs`, `src/config/mod.rs`
- Tests: inline `#[cfg(test)]` modules per file + `tests/fixtures/teams/`

### Task 1.1: Add `directories` crate dependency

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Check current `directories` usage**

```bash
grep -n 'directories' Cargo.toml || echo "not present"
```

Expected: `not present` (so this is a new dep).

- [ ] **Step 2: Add to `[dependencies]`**

In `Cargo.toml`, add after the existing `dirs` or `home` entry (alphabetical):

```toml
directories = "5"
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo build 2>&1 | tail -5
```

Expected: success, `directories` resolves to a v5.x version.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore(deps): add directories crate for cross-platform paths"
```

### Task 1.2: Module skeleton + `Primitive` enum

**Files:**
- Create: `src/orchestration/mod.rs`
- Create: `src/orchestration/types.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write failing test for Primitive serde round-trip**

Create `src/orchestration/types.rs`:

```rust
//! Core orchestration types — primitives, inputs, outputs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Primitive {
    Pipeline,
    FanOut,
    SinglePass,
    VerdictOnly,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_serde_kebab_case() {
        let p = Primitive::FanOut;
        let s = serde_json::to_string(&p).unwrap();
        assert_eq!(s, r#""fan-out""#);
        let back: Primitive = serde_json::from_str(&s).unwrap();
        assert_eq!(back, Primitive::FanOut);
    }

    #[test]
    fn primitive_serde_all_variants() {
        for (variant, expected) in [
            (Primitive::Pipeline, "pipeline"),
            (Primitive::FanOut, "fan-out"),
            (Primitive::SinglePass, "single-pass"),
            (Primitive::VerdictOnly, "verdict-only"),
        ] {
            let s = serde_json::to_string(&variant).unwrap();
            assert_eq!(s, format!(r#""{expected}""#));
        }
    }
}
```

Create `src/orchestration/mod.rs`:

```rust
//! Multi-agent team orchestration — see docs/superpowers/specs/2026-05-05-orchestration-wizard-design.md

pub mod types;

pub use types::{Primitive};
```

Modify `src/lib.rs` — add `pub mod orchestration;` near the other `pub mod` declarations.

- [ ] **Step 2: Run test — expect compile success + pass**

```bash
cargo test -p maestro --lib orchestration::types::tests
```

Expected: 2 passed.

- [ ] **Step 3: Add the rest of the type enum surface**

Append to `src/orchestration/types.rs`:

```rust
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TeamInput {
    Issue { number: u64 },
    /// Set of issues, possibly spanning multiple milestones.
    /// `primary_milestone` is the wizard's reference for "same-milestone"
    /// auto-add classification (see scheduler STEP 4).
    IssueSet { primary_milestone: Option<u64>, issues: Vec<u64> },
    IdeaInbox,
    // FileSet variant deferred to v2 per spec §13.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TeamOutput {
    Pr { number: u64, branch: String },
    NewIssues { numbers: Vec<u64> },
    Comment { issue: u64, body: String },
    AdrDraft { path: PathBuf },
    Verdict { json: serde_json::Value },
    Commit { sha: String, branch: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Implementer,
    Reviewer,
    Docs,
    Devops,
    Orchestrator,
    Triager,
    Researcher,
}
```

- [ ] **Step 4: Add a primitive-required-roles table test**

```rust
impl Primitive {
    /// Which `Role`s a given primitive requires bound for a team to be valid.
    pub fn required_roles(self) -> &'static [Role] {
        match self {
            Self::Pipeline => &[Role::Implementer, Role::Reviewer, Role::Docs],
            Self::FanOut => &[Role::Reviewer], // at minimum; team can bind more
            Self::SinglePass => &[], // any single role allowed; validator picks
            Self::VerdictOnly => &[Role::Reviewer],
        }
    }
}

#[cfg(test)]
mod required_roles_tests {
    use super::*;

    #[test]
    fn pipeline_requires_three_roles() {
        let roles = Primitive::Pipeline.required_roles();
        assert_eq!(roles.len(), 3);
        assert!(roles.contains(&Role::Implementer));
        assert!(roles.contains(&Role::Reviewer));
        assert!(roles.contains(&Role::Docs));
    }

    #[test]
    fn single_pass_has_no_required_roles() {
        assert_eq!(Primitive::SinglePass.required_roles(), &[]);
    }
}
```

- [ ] **Step 5: Run all type tests + commit**

```bash
cargo test -p maestro --lib orchestration::types
git add src/lib.rs src/orchestration/
git commit -m "feat(orchestration): add Primitive, TeamInput, TeamOutput, Role enums"
```

### Task 1.3: `SubagentResult` contract enum

**Files:**
- Create: `src/orchestration/contracts.rs`
- Modify: `src/orchestration/mod.rs`

- [ ] **Step 1: Write failing tests for contract types**

Create `src/orchestration/contracts.rs`:

```rust
//! Subagent output contracts. L1 enforces; L2 trusts.
//! See spec §4 "Role output contracts" for derivation.

use crate::orchestration::types::Role;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SubagentResult {
    CodeChange {
        files_touched: Vec<PathBuf>,
        summary: String,
        commit_sha: Option<String>,
    },
    ReviewFindings {
        verdict: ReviewVerdict,
        findings: Vec<Finding>,
    },
    DocsChange {
        files_touched: Vec<PathBuf>,
        summary: String,
    },
    Verdict {
        decision: String,
        rationale: String,
        new_issues: Vec<NewIssueDraft>,
    },
    Generic {
        json: serde_json::Value,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewVerdict {
    Approved,
    RequestChanges,
    Comment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub file: Option<PathBuf>,
    pub line: Option<u32>,
    pub severity: FindingSeverity,
    pub note: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FindingSeverity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewIssueDraft {
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
    pub milestone: Option<u64>,
}

#[derive(Error, Debug, Clone)]
pub enum SubagentError {
    #[error("subagent timed out after {seconds}s")]
    Timeout { seconds: u64 },
    #[error("provider error: {0}")]
    Provider(String),
    #[error("subagent returned a payload that did not match role {role:?}: expected {expected}, got {got}")]
    ResultShapeMismatch {
        role: Role,
        expected: String,
        got: String,
    },
    #[error("malformed parser output: {0}")]
    Malformed(String),
    #[error("subagent reported failure: {0}")]
    SubagentReported(String),
    #[error("other: {0}")]
    Other(String),
}

impl Role {
    /// Which `SubagentResult` variants this role is allowed to produce.
    /// Validator uses this to map L1's parsed output to the right variant.
    pub fn allowed_results(self) -> &'static [&'static str] {
        match self {
            Self::Implementer => &["code-change"],
            Self::Reviewer => &["review-findings", "generic"],
            Self::Docs => &["docs-change"],
            Self::Devops => &["code-change", "generic"],
            Self::Triager | Self::Researcher => &["verdict"],
            Self::Orchestrator => &["generic"], // shouldn't be a subagent itself in v1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subagent_result_round_trips_review_findings() {
        let r = SubagentResult::ReviewFindings {
            verdict: ReviewVerdict::Approved,
            findings: vec![Finding {
                file: Some(PathBuf::from("src/foo.rs")),
                line: Some(42),
                severity: FindingSeverity::Warn,
                note: "watch for off-by-one".into(),
            }],
        };
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains(r#""kind":"review-findings""#));
        assert!(s.contains(r#""verdict":"approved""#));
        let _back: SubagentResult = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn role_allowed_results_implementer() {
        assert_eq!(Role::Implementer.allowed_results(), &["code-change"]);
    }
}
```

- [ ] **Step 2: Add module re-export**

```rust
// src/orchestration/mod.rs
pub mod types;
pub mod contracts;

pub use contracts::{
    Finding, FindingSeverity, NewIssueDraft, ReviewVerdict, SubagentError, SubagentResult,
};
pub use types::{Primitive, Role, TeamInput, TeamOutput};
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p maestro --lib orchestration::contracts
```

Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add src/orchestration/
git commit -m "feat(orchestration): add SubagentResult contract enum"
```

### Task 1.4: TOML schema types — `TeamConfig`, `RoleBinding`, `RoleOverride`

**Files:**
- Create: `src/orchestration/team.rs`
- Modify: `src/orchestration/mod.rs`

- [ ] **Step 1: Write failing test for the minimal-form TOML parse**

Create `src/orchestration/team.rs`:

```rust
//! Team preset TOML schema — see spec §4.

use crate::orchestration::types::{Primitive, Role};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamConfig {
    /// Parent preset name. Empty string means root (built-in).
    #[serde(default)]
    pub extends: String,

    /// Required if no `extends`; otherwise inherited.
    pub primitive: Option<Primitive>,

    /// Required if no `extends`; otherwise inherited.
    #[serde(default)]
    pub min_agents: Option<Vec<String>>,

    /// Minimal-form bindings: top-level keys whose values are agent_id strings.
    /// Captured via #[serde(flatten)] into a HashMap; non-binding fields above
    /// are deserialized first.
    #[serde(flatten)]
    pub bindings: HashMap<String, toml::Value>,

    /// Rich-form bindings: per-role override sub-table.
    #[serde(default)]
    pub role_overrides: HashMap<String, RoleOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RoleOverride {
    pub agent: Option<String>,
    pub mode: Option<String>,
    pub model_override: Option<String>,
    pub prompt_addendum: Option<String>,
    pub fallback_agent: Option<String>,
}

/// Resolved (post-`extends` merge) team — all bindings concrete.
#[derive(Debug, Clone)]
pub struct ResolvedTeam {
    pub name: String,
    pub primitive: Primitive,
    pub min_agents: Vec<String>,
    pub bindings: HashMap<Role, RoleBinding>,
    pub source_tier: SourceTier,
}

#[derive(Debug, Clone)]
pub struct RoleBinding {
    pub agent: String,
    pub mode: Option<String>,
    pub model_override: Option<String>,
    pub prompt_addendum: Option<String>,
    pub fallback_agent: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceTier {
    BuiltIn,
    User,
    Project,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_form() {
        let toml = r#"
extends = "default-coder"
implementer = "ollama"
reviewer = "opencode"
docs = "minimax"
"#;
        let config: TeamConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.extends, "default-coder");
        assert_eq!(config.bindings.get("implementer").unwrap().as_str(), Some("ollama"));
    }

    #[test]
    fn parses_rich_form_with_overrides() {
        let toml = r#"
extends = "cheap-coder"

[role_overrides.reviewer]
agent = "opencode"
mode = "review-strict"
prompt_addendum = "Be terse."
fallback_agent = "claude"
"#;
        let config: TeamConfig = toml::from_str(toml).unwrap();
        let r = config.role_overrides.get("reviewer").unwrap();
        assert_eq!(r.agent, Some("opencode".into()));
        assert_eq!(r.mode, Some("review-strict".into()));
        assert_eq!(r.fallback_agent, Some("claude".into()));
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let toml = r#"
extends = "default-coder"
implementer = "ollama"
unknown_field = "boom"
"#;
        // The flatten captures `unknown_field` as a binding — that's expected
        // behavior for the minimal form. The validator (Task 1.7) will reject
        // bindings that don't map to a known Role at validation time.
        let config: TeamConfig = toml::from_str(toml).unwrap();
        assert!(config.bindings.contains_key("unknown_field"));
    }
}
```

- [ ] **Step 2: Add module export**

```rust
// src/orchestration/mod.rs (add)
pub mod team;
pub use team::{ResolvedTeam, RoleBinding, RoleOverride, SourceTier, TeamConfig};
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p maestro --lib orchestration::team
```

Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add src/orchestration/
git commit -m "feat(orchestration): add TeamConfig, RoleOverride, ResolvedTeam schema"
```

### Task 1.5: Three-tier loader skeleton

**Files:**
- Create: `src/orchestration/loader.rs`
- Modify: `src/orchestration/mod.rs`

- [ ] **Step 1: Write failing test for built-in tier discovery**

Create `src/orchestration/loader.rs`:

```rust
//! Three-tier team preset loader. See spec §4 Tier resolution.

use crate::orchestration::team::{SourceTier, TeamConfig};
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Raw (pre-`extends`-merge) entry as loaded from disk or embedded TOML.
#[derive(Debug, Clone)]
pub struct RawTeam {
    pub name: String,
    pub config: TeamConfig,
    pub source_tier: SourceTier,
    pub source_path: Option<PathBuf>,
}

pub struct Loader {
    builtins: Vec<RawTeam>,
    user_dir: Option<PathBuf>,
    project_dir: Option<PathBuf>,
    project_inline: Vec<RawTeam>,
}

impl Loader {
    /// Build a loader with the given user and project directories.
    /// Pass `None` for the user directory to skip user-tier loading
    /// (used in tests; production loaders use `directories` crate).
    pub fn new(user_dir: Option<PathBuf>, project_dir: Option<PathBuf>) -> Self {
        Self {
            builtins: Self::load_builtins(),
            user_dir,
            project_dir,
            project_inline: Vec::new(),
        }
    }

    fn load_builtins() -> Vec<RawTeam> {
        // Filled in by Task 1.9; placeholder for now.
        Vec::new()
    }

    /// Load all three tiers and apply name-collision (project > user > built-in).
    /// Returns the post-collision raw map; `extends` merge happens separately
    /// in `Loader::resolve`.
    pub fn load_raw(&self) -> Result<HashMap<String, RawTeam>> {
        let mut map = HashMap::new();
        // built-ins first — lowest priority
        for t in &self.builtins {
            map.insert(t.name.clone(), t.clone());
        }
        // user tier
        if let Some(dir) = &self.user_dir {
            for t in load_dir(dir, SourceTier::User)? {
                map.insert(t.name.clone(), t);
            }
        }
        // project tier (highest priority — overwrites)
        if let Some(dir) = &self.project_dir {
            for t in load_dir(dir, SourceTier::Project)? {
                map.insert(t.name.clone(), t);
            }
        }
        for t in &self.project_inline {
            map.insert(t.name.clone(), t.clone());
        }
        Ok(map)
    }
}

fn load_dir(dir: &Path, tier: SourceTier) -> Result<Vec<RawTeam>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("reading team dir {dir:?}"))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("invalid team filename: {path:?}"))?
            .to_string();
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("reading team file {path:?}"))?;
        let config: TeamConfig = toml::from_str(&content)
            .with_context(|| format!("parsing team file {path:?}"))?;
        out.push(RawTeam {
            name,
            config,
            source_tier: tier,
            source_path: Some(path),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn empty_dirs_load_cleanly() {
        let loader = Loader::new(None, None);
        let raw = loader.load_raw().unwrap();
        assert!(raw.is_empty());
    }

    #[test]
    fn project_overwrites_user() {
        let user = tempdir().unwrap();
        let project = tempdir().unwrap();

        fs::write(
            user.path().join("foo.toml"),
            r#"extends = ""
primitive = "single-pass"
implementer = "claude""#,
        )
        .unwrap();
        fs::write(
            project.path().join("foo.toml"),
            r#"extends = ""
primitive = "single-pass"
implementer = "ollama""#,
        )
        .unwrap();

        let loader = Loader::new(
            Some(user.path().to_path_buf()),
            Some(project.path().to_path_buf()),
        );
        let raw = loader.load_raw().unwrap();
        let foo = raw.get("foo").unwrap();
        assert_eq!(foo.source_tier, SourceTier::Project);
        assert_eq!(
            foo.config.bindings.get("implementer").unwrap().as_str(),
            Some("ollama")
        );
    }
}
```

- [ ] **Step 2: Module wiring + test run**

Add to `src/orchestration/mod.rs`:

```rust
pub mod loader;
pub use loader::Loader;
```

Add `tempfile` to `[dev-dependencies]` in `Cargo.toml` if not present:

```bash
grep -A1 'dev-dependencies' Cargo.toml | grep -q tempfile || \
    cargo add --dev tempfile
```

```bash
cargo test -p maestro --lib orchestration::loader
```

Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add src/orchestration/ Cargo.toml Cargo.lock
git commit -m "feat(orchestration): add three-tier team loader skeleton"
```

### Task 1.6: `extends` resolution + cycle detection

**Files:**
- Modify: `src/orchestration/loader.rs`

- [ ] **Step 1: Write failing tests for the resolver**

Append to the `tests` module in `src/orchestration/loader.rs`:

```rust
#[test]
fn extends_chain_merges_role_overrides() {
    let user = tempdir().unwrap();
    fs::write(
        user.path().join("base.toml"),
        r#"extends = ""
primitive = "pipeline"
implementer = "claude"
reviewer = "claude"
docs = "claude""#,
    )
    .unwrap();
    fs::write(
        user.path().join("child.toml"),
        r#"extends = "base"
reviewer = "opencode""#,
    )
    .unwrap();

    let loader = Loader::new(Some(user.path().to_path_buf()), None);
    let resolved = loader.resolve().unwrap();
    let child = resolved.get("child").unwrap();
    assert_eq!(child.bindings.get(&Role::Reviewer).unwrap().agent, "opencode");
    assert_eq!(child.bindings.get(&Role::Implementer).unwrap().agent, "claude");
}

#[test]
fn cycle_detected() {
    let user = tempdir().unwrap();
    fs::write(
        user.path().join("a.toml"),
        r#"extends = "b"
implementer = "claude""#,
    )
    .unwrap();
    fs::write(
        user.path().join("b.toml"),
        r#"extends = "a"
implementer = "claude""#,
    )
    .unwrap();

    let loader = Loader::new(Some(user.path().to_path_buf()), None);
    let err = loader.resolve().unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("cycle"), "expected cycle error, got: {msg}");
}
```

(Add `use crate::orchestration::types::Role;` to the test module imports if needed.)

- [ ] **Step 2: Implement `resolve()`**

Add to `Loader`:

```rust
impl Loader {
    pub fn resolve(&self) -> Result<HashMap<String, ResolvedTeam>> {
        use crate::orchestration::team::{ResolvedTeam, RoleBinding};
        use crate::orchestration::types::Role;

        let raw = self.load_raw()?;
        let mut resolved: HashMap<String, ResolvedTeam> = HashMap::new();

        for name in raw.keys() {
            // Walk the extends chain, detecting cycles via visited set.
            let mut visited: Vec<String> = vec![name.clone()];
            let mut chain: Vec<&RawTeam> = Vec::new();
            let mut cur_name = name.clone();
            loop {
                let cur = raw.get(&cur_name)
                    .ok_or_else(|| anyhow!("team {name:?} extends missing parent {cur_name:?}"))?;
                chain.push(cur);
                if cur.config.extends.is_empty() {
                    break;
                }
                if visited.contains(&cur.config.extends) {
                    return Err(anyhow!(
                        "extends cycle detected: {} → {}",
                        visited.join(" → "),
                        cur.config.extends
                    ));
                }
                visited.push(cur.config.extends.clone());
                cur_name = cur.config.extends.clone();
            }

            // Merge from root toward leaf.
            let mut primitive: Option<Primitive> = None;
            let mut min_agents: Vec<String> = Vec::new();
            let mut bindings_str: HashMap<String, String> = HashMap::new();
            let mut overrides: HashMap<String, RoleOverride> = HashMap::new();
            for cur in chain.iter().rev() {
                if cur.config.primitive.is_some() {
                    primitive = cur.config.primitive;
                }
                if let Some(m) = &cur.config.min_agents {
                    min_agents = m.clone();
                }
                for (k, v) in &cur.config.bindings {
                    if let Some(s) = v.as_str() {
                        bindings_str.insert(k.clone(), s.to_string());
                    }
                }
                for (k, v) in &cur.config.role_overrides {
                    overrides.insert(k.clone(), v.clone());
                }
            }

            let primitive = primitive.ok_or_else(|| {
                anyhow!("team {name:?}: primitive not set anywhere in extends chain")
            })?;

            let mut bindings: HashMap<Role, RoleBinding> = HashMap::new();
            for (k, agent) in &bindings_str {
                let role = match k.as_str() {
                    "implementer" => Role::Implementer,
                    "reviewer" => Role::Reviewer,
                    "docs" => Role::Docs,
                    "devops" => Role::Devops,
                    "orchestrator" => Role::Orchestrator,
                    "triager" => Role::Triager,
                    "researcher" => Role::Researcher,
                    other => return Err(anyhow!("team {name:?}: unknown role binding {other:?}")),
                };
                let ovr = overrides.get(k);
                bindings.insert(role, RoleBinding {
                    agent: ovr.and_then(|o| o.agent.clone()).unwrap_or_else(|| agent.clone()),
                    mode: ovr.and_then(|o| o.mode.clone()),
                    model_override: ovr.and_then(|o| o.model_override.clone()),
                    prompt_addendum: ovr.and_then(|o| o.prompt_addendum.clone()),
                    fallback_agent: ovr.and_then(|o| o.fallback_agent.clone()),
                });
            }

            resolved.insert(name.clone(), ResolvedTeam {
                name: name.clone(),
                primitive,
                min_agents,
                bindings,
                source_tier: raw[name].source_tier,
            });
        }

        Ok(resolved)
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p maestro --lib orchestration::loader
```

Expected: 4 passed.

- [ ] **Step 4: Commit**

```bash
git add src/orchestration/
git commit -m "feat(orchestration): add extends-chain resolver with cycle detection"
```

### Task 1.7: Validation rules

**Files:**
- Create: `src/orchestration/validation.rs`
- Modify: `src/orchestration/mod.rs`

- [ ] **Step 1: Write failing tests**

Create `src/orchestration/validation.rs`:

```rust
//! Load-time validation per spec §4 "Validation rules".

use crate::orchestration::team::ResolvedTeam;
use crate::orchestration::types::Role;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum ValidationError {
    #[error("team {team:?}: missing required role {role:?} for primitive {primitive:?}")]
    MissingRequiredRole {
        team: String,
        role: Role,
        primitive: String,
    },
    #[error("team {team:?}: agent {agent:?} (referenced by role {role:?}) is not configured in [agents.*]")]
    AgentNotConfigured {
        team: String,
        agent: String,
        role: Role,
    },
    #[error("team {team:?}: mode {mode:?} (role {role:?}) is not configured in [modes.*]")]
    ModeNotConfigured {
        team: String,
        mode: String,
        role: Role,
    },
    #[error("team {team:?}: claude must be in min_agents (L2 provider constraint, see spec §3)")]
    ClaudeNotInMinAgents { team: String },
}

/// Validate a resolved team against the live config (known agents, modes).
pub fn validate(
    team: &ResolvedTeam,
    known_agents: &[String],
    known_modes: &[String],
) -> Result<(), Vec<ValidationError>> {
    let mut errs = Vec::new();

    // Required-roles check.
    for role in team.primitive.required_roles() {
        if !team.bindings.contains_key(role) {
            errs.push(ValidationError::MissingRequiredRole {
                team: team.name.clone(),
                role: *role,
                primitive: format!("{:?}", team.primitive).to_lowercase(),
            });
        }
    }

    // Agent existence.
    for (role, binding) in &team.bindings {
        if !known_agents.iter().any(|a| a == &binding.agent) {
            errs.push(ValidationError::AgentNotConfigured {
                team: team.name.clone(),
                agent: binding.agent.clone(),
                role: *role,
            });
        }
        if let Some(mode) = &binding.mode {
            if !known_modes.iter().any(|m| m == mode) {
                errs.push(ValidationError::ModeNotConfigured {
                    team: team.name.clone(),
                    mode: mode.clone(),
                    role: *role,
                });
            }
        }
    }

    // L2 Claude constraint (spec §3).
    if !team.min_agents.iter().any(|a| a == "claude") {
        errs.push(ValidationError::ClaudeNotInMinAgents {
            team: team.name.clone(),
        });
    }

    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::team::{RoleBinding, SourceTier};
    use crate::orchestration::types::Primitive;
    use std::collections::HashMap;

    fn binding(agent: &str) -> RoleBinding {
        RoleBinding {
            agent: agent.into(),
            mode: None,
            model_override: None,
            prompt_addendum: None,
            fallback_agent: None,
        }
    }

    #[test]
    fn pipeline_missing_reviewer_fails() {
        let mut bindings = HashMap::new();
        bindings.insert(Role::Implementer, binding("claude"));
        bindings.insert(Role::Docs, binding("claude"));
        let team = ResolvedTeam {
            name: "broken".into(),
            primitive: Primitive::Pipeline,
            min_agents: vec!["claude".into()],
            bindings,
            source_tier: SourceTier::User,
        };
        let errs = validate(&team, &["claude".into()], &[]).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::MissingRequiredRole { role: Role::Reviewer, .. })));
    }

    #[test]
    fn unknown_agent_fails() {
        let mut bindings = HashMap::new();
        bindings.insert(Role::Reviewer, binding("ghost"));
        let team = ResolvedTeam {
            name: "ghost-team".into(),
            primitive: Primitive::SinglePass,
            min_agents: vec!["claude".into()],
            bindings,
            source_tier: SourceTier::User,
        };
        let errs = validate(&team, &["claude".into()], &[]).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::AgentNotConfigured { .. })));
    }

    #[test]
    fn missing_claude_in_min_agents_fails() {
        let mut bindings = HashMap::new();
        bindings.insert(Role::Reviewer, binding("ollama"));
        let team = ResolvedTeam {
            name: "no-claude".into(),
            primitive: Primitive::SinglePass,
            min_agents: vec!["ollama".into()],
            bindings,
            source_tier: SourceTier::User,
        };
        let errs = validate(&team, &["ollama".into()], &[]).unwrap_err();
        assert!(errs.iter().any(|e| matches!(e, ValidationError::ClaudeNotInMinAgents { .. })));
    }
}
```

- [ ] **Step 2: Module export + tests**

Add to `src/orchestration/mod.rs`:

```rust
pub mod validation;
pub use validation::{validate, ValidationError};
```

```bash
cargo test -p maestro --lib orchestration::validation
```

Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add src/orchestration/
git commit -m "feat(orchestration): add team validation rules"
```

### Task 1.8: Cross-platform user-config path resolution

**Files:**
- Modify: `src/orchestration/loader.rs`

- [ ] **Step 1: Write test for path resolution**

Add to `src/orchestration/loader.rs`:

```rust
impl Loader {
    /// Resolve the user-tier teams directory using the `directories` crate.
    /// Returns `None` if the platform's config dir cannot be determined.
    pub fn user_tier_default() -> Option<PathBuf> {
        directories::ProjectDirs::from("io", "maestro", "maestro")
            .map(|p| p.config_dir().join("teams"))
    }

    /// Resolve the project-tier teams directory: `<repo_root>/.maestro/teams`.
    pub fn project_tier_default(repo_root: &Path) -> PathBuf {
        repo_root.join(".maestro/teams")
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn user_tier_default_returns_some_on_linux_macos_windows() {
        let dir = Loader::user_tier_default();
        assert!(dir.is_some(), "directories::ProjectDirs returned None");
        let dir = dir.unwrap();
        assert!(dir.ends_with("teams"));
    }

    #[test]
    fn project_tier_default_uses_dot_maestro() {
        let p = Loader::project_tier_default(Path::new("/tmp/repo"));
        assert_eq!(p, PathBuf::from("/tmp/repo/.maestro/teams"));
    }
}
```

- [ ] **Step 2: Run + commit**

```bash
cargo test -p maestro --lib orchestration::loader::path_tests
git add src/orchestration/loader.rs
git commit -m "feat(orchestration): add cross-platform user-tier path resolver"
```

### Task 1.9: Embed five built-in TOML files

**Files:**
- Create: `src/orchestration/builtins/mod.rs`
- Create: `src/orchestration/builtins/{default-coder,default-researcher,default-triager,default-reviewer,default-docs}.toml`

- [ ] **Step 1: Write failing test for built-ins loading**

Add a test (e.g., `src/orchestration/builtins/mod.rs`):

```rust
//! Binary-embedded built-in team presets.
//! See spec §4 "Built-in seed list (v1, ship 5)".

use crate::orchestration::loader::RawTeam;
use crate::orchestration::team::{SourceTier, TeamConfig};

const DEFAULT_CODER: &str = include_str!("default-coder.toml");
const DEFAULT_RESEARCHER: &str = include_str!("default-researcher.toml");
const DEFAULT_TRIAGER: &str = include_str!("default-triager.toml");
const DEFAULT_REVIEWER: &str = include_str!("default-reviewer.toml");
const DEFAULT_DOCS: &str = include_str!("default-docs.toml");

const ALL: &[(&str, &str)] = &[
    ("default-coder", DEFAULT_CODER),
    ("default-researcher", DEFAULT_RESEARCHER),
    ("default-triager", DEFAULT_TRIAGER),
    ("default-reviewer", DEFAULT_REVIEWER),
    ("default-docs", DEFAULT_DOCS),
];

pub(crate) fn load_all() -> Vec<RawTeam> {
    ALL.iter()
        .map(|(name, src)| {
            let config: TeamConfig = toml::from_str(src)
                .unwrap_or_else(|e| panic!("built-in {name} fails to parse: {e}"));
            RawTeam {
                name: name.to_string(),
                config,
                source_tier: SourceTier::BuiltIn,
                source_path: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builtins_parse() {
        let raws = load_all();
        assert_eq!(raws.len(), 5);
        for r in &raws {
            assert_eq!(r.source_tier, SourceTier::BuiltIn);
            assert!(r.config.min_agents.as_ref().unwrap().contains(&"claude".to_string()));
        }
    }
}
```

- [ ] **Step 2: Write the five TOML files**

`src/orchestration/builtins/default-coder.toml`:

```toml
extends = ""
primitive = "pipeline"
min_agents = ["claude"]
implementer = "claude"
reviewer = "claude"
docs = "claude"
```

`src/orchestration/builtins/default-researcher.toml`:

```toml
extends = ""
primitive = "verdict-only"
min_agents = ["claude"]
implementer = "claude"
reviewer = "claude"
```

`src/orchestration/builtins/default-triager.toml`:

```toml
extends = ""
primitive = "verdict-only"
min_agents = ["claude"]
triager = "claude"
```

`src/orchestration/builtins/default-reviewer.toml`:

```toml
extends = ""
primitive = "single-pass"
min_agents = ["claude"]
reviewer = "claude"
```

`src/orchestration/builtins/default-docs.toml`:

```toml
extends = ""
primitive = "single-pass"
min_agents = ["claude"]
docs = "claude"
```

- [ ] **Step 3: Wire built-ins into loader**

Modify `Loader::load_builtins()` in `src/orchestration/loader.rs`:

```rust
fn load_builtins() -> Vec<RawTeam> {
    crate::orchestration::builtins::load_all()
}
```

Add `pub mod builtins;` to `src/orchestration/mod.rs` (private inside the crate; `pub(crate)` is the right visibility — but `pub mod` keeps the test access simple). Actually, since `load_all` is `pub(crate)`, `mod builtins;` is sufficient.

- [ ] **Step 4: Run tests**

```bash
cargo test -p maestro --lib orchestration::builtins
cargo test -p maestro --lib orchestration::loader
```

Expected: all green.

- [ ] **Step 5: Smoke test — the loader resolves all five built-ins**

Add to `src/orchestration/loader.rs` tests:

```rust
#[test]
fn builtins_resolve_clean() {
    let loader = Loader::new(None, None);
    let resolved = loader.resolve().unwrap();
    assert_eq!(resolved.len(), 5);
    for name in ["default-coder", "default-researcher", "default-triager", "default-reviewer", "default-docs"] {
        assert!(resolved.contains_key(name), "missing {name}");
    }
}
```

```bash
cargo test -p maestro --lib orchestration::loader::tests::builtins_resolve_clean
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add src/orchestration/
git commit -m "feat(orchestration): embed five built-in team presets"
```

### Task 1.10: Add `[concurrency.team_max_parallel]` and `[teams.*]` to config schema

**Files:**
- Modify: `src/config/mod.rs`

- [ ] **Step 1: Write failing test**

Add to the existing config tests (find the appropriate test file under `src/config/`):

```rust
#[test]
fn parses_team_max_parallel() {
    let toml = r#"
[concurrency]
team_max_parallel = 5
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.concurrency.team_max_parallel, Some(5));
}

#[test]
fn parses_inline_teams_section() {
    let toml = r#"
[teams.cheap]
extends = "default-coder"
implementer = "ollama"
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert!(cfg.teams.contains_key("cheap"));
}
```

- [ ] **Step 2: Add fields to `Config`**

In `src/config/mod.rs`'s `Concurrency` struct:

```rust
#[serde(default)]
pub team_max_parallel: Option<u32>,
```

Add to `Config`:

```rust
#[serde(default)]
pub teams: std::collections::HashMap<String, crate::orchestration::team::TeamConfig>,
```

- [ ] **Step 3: Test + commit**

```bash
cargo test -p maestro --lib config
git add src/config/mod.rs
git commit -m "feat(config): add [concurrency.team_max_parallel] and inline [teams.*]"
```

---

## Chunk 2: L3 Cross-Issue Scheduler

**Goal:** Implement the cross-issue scheduler — DAG construction from `## Blocked By`, edge classification, topo levels, the auto-add expansion, and the `TeamRun` lifecycle in the state store. After this chunk, given a `TeamInput::IssueSet` and a `ResolvedTeam`, the scheduler produces a launch plan and persists a `TeamRun` (without yet executing it).

### Task 2.1: `TeamRun` and `IssueRunState` in state store

**Files:**
- Modify: `src/state/types.rs`

- [ ] **Step 1: Write failing test for serde round-trip**

In a new test in `src/state/types.rs`:

```rust
#[cfg(test)]
#[test]
fn team_run_serde_round_trip() {
    use chrono::Utc;
    use std::collections::HashMap;
    use uuid::Uuid;
    use crate::orchestration::types::TeamOutput;

    let mut state = HashMap::new();
    state.insert(
        547,
        IssueRunState::Succeeded {
            output: TeamOutput::Pr { number: 714, branch: "feat/547".into() },
        },
    );

    let run = TeamRun {
        id: Uuid::new_v4(),
        team_name: "default-coder".into(),
        started_at: Utc::now(),
        plan: vec![vec![547]],
        state,
    };
    let json = serde_json::to_string(&run).unwrap();
    let _back: TeamRun = serde_json::from_str(&json).unwrap();
}
```

- [ ] **Step 2: Add types**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use crate::orchestration::types::TeamOutput;

pub type IssueNumber = u64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamRun {
    pub id: Uuid,
    pub team_name: String,
    pub started_at: DateTime<Utc>,
    pub plan: Vec<Vec<IssueNumber>>,
    pub state: HashMap<IssueNumber, IssueRunState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum IssueRunState {
    Queued,
    InFlight {
        session_id: Uuid,
        started_at: DateTime<Utc>,
    },
    Succeeded {
        output: TeamOutput,
    },
    Failed {
        reason: String,
        attempts: u8,
    },
    Blocked {
        blocking: Vec<IssueNumber>,
    },
}
```

- [ ] **Step 3: Test + commit**

```bash
cargo test -p maestro --lib state::types::team_run_serde_round_trip
git add src/state/types.rs
git commit -m "feat(state): add TeamRun and IssueRunState records"
```

### Task 2.2: DAG construction from `Blocked By`

**Files:**
- Create: `src/orchestration/dag.rs`

- [ ] **Step 1: Write failing test for `## Blocked By` parser**

Create `src/orchestration/dag.rs`:

```rust
//! DAG construction, edge classification, topo sort. Spec §5.

use crate::state::types::IssueNumber;
use anyhow::Result;
use regex::Regex;
use std::collections::{HashMap, HashSet};

/// Parse the `## Blocked By` section out of an issue body.
/// Returns the list of dep issue numbers, or empty if "None".
pub fn parse_blocked_by(body: &str) -> Vec<IssueNumber> {
    // Find the `## Blocked By` heading and collect lines until the next heading.
    let header_re = Regex::new(r"(?m)^##\s+Blocked\s+By\s*$").unwrap();
    let next_header_re = Regex::new(r"(?m)^##\s+").unwrap();
    let issue_re = Regex::new(r"#(\d+)").unwrap();

    let Some(start) = header_re.find(body) else {
        return Vec::new();
    };
    let after = &body[start.end()..];
    let section = match next_header_re.find(after) {
        Some(m) => &after[..m.start()],
        None => after,
    };

    if section.to_lowercase().contains("none") {
        return Vec::new();
    }
    issue_re
        .captures_iter(section)
        .filter_map(|c| c.get(1)?.as_str().parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_none_as_empty() {
        let body = "## Blocked By\n\n- None\n\n## Other\n";
        assert!(parse_blocked_by(body).is_empty());
    }

    #[test]
    fn parses_single_dep() {
        let body = "## Blocked By\n\n- #547 trait extraction\n";
        assert_eq!(parse_blocked_by(body), vec![547]);
    }

    #[test]
    fn parses_multi_dep() {
        let body = "## Blocked By\n\n- #547\n- #549\n- #650\n";
        assert_eq!(parse_blocked_by(body), vec![547, 549, 650]);
    }

    #[test]
    fn missing_section_returns_empty() {
        assert!(parse_blocked_by("## Overview\n").is_empty());
    }

    #[test]
    fn malformed_section_recovers() {
        let body = "## Blocked By\n\nIDK refs #99 and stuff #100\n";
        assert_eq!(parse_blocked_by(body), vec![99, 100]);
    }
}
```

- [ ] **Step 2: Add `regex` to `Cargo.toml` if missing**

```bash
grep -q '^regex' Cargo.toml || cargo add regex
```

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p maestro --lib orchestration::dag::tests
git add src/orchestration/ Cargo.toml Cargo.lock
git commit -m "feat(orchestration): add Blocked By parser"
```

### Task 2.3: Edge classification

**Files:**
- Modify: `src/orchestration/dag.rs`

- [ ] **Step 1: Define types and write failing test**

Append to `src/orchestration/dag.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Edge {
    InSlice(IssueNumber),
    ClosedExternal(IssueNumber),
    SameMilestoneOpenExternal(IssueNumber),
    CrossMilestoneOpenExternal(IssueNumber),
}

#[derive(Debug, Clone)]
pub struct IssueMeta {
    pub number: IssueNumber,
    pub state: IssueState,
    pub milestone: Option<u64>,
    pub blocked_by: Vec<IssueNumber>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueState {
    Open,
    Closed,
}

pub fn classify_edges(
    selected: &HashSet<IssueNumber>,
    primary_milestone: Option<u64>,
    metas: &HashMap<IssueNumber, IssueMeta>,
) -> HashMap<IssueNumber, Vec<Edge>> {
    let mut out = HashMap::new();
    for &n in selected {
        let meta = match metas.get(&n) {
            Some(m) => m,
            None => continue,
        };
        let mut edges = Vec::new();
        for &dep in &meta.blocked_by {
            if selected.contains(&dep) {
                edges.push(Edge::InSlice(dep));
                continue;
            }
            let dep_meta = metas.get(&dep);
            match dep_meta.map(|m| m.state) {
                Some(IssueState::Closed) => edges.push(Edge::ClosedExternal(dep)),
                Some(IssueState::Open) => {
                    let same_ms = match (primary_milestone, dep_meta.unwrap().milestone) {
                        (Some(a), Some(b)) => a == b,
                        _ => false,
                    };
                    edges.push(if same_ms {
                        Edge::SameMilestoneOpenExternal(dep)
                    } else {
                        Edge::CrossMilestoneOpenExternal(dep)
                    });
                }
                None => edges.push(Edge::CrossMilestoneOpenExternal(dep)), // unknown = treat as cross
            }
        }
        out.insert(n, edges);
    }
    out
}

#[cfg(test)]
mod classify_tests {
    use super::*;

    fn meta(n: IssueNumber, state: IssueState, milestone: Option<u64>, blocked_by: Vec<IssueNumber>) -> IssueMeta {
        IssueMeta { number: n, state, milestone, blocked_by }
    }

    #[test]
    fn in_slice_dep_is_in_slice() {
        let mut metas = HashMap::new();
        metas.insert(549, meta(549, IssueState::Open, Some(49), vec![547]));
        metas.insert(547, meta(547, IssueState::Open, Some(49), vec![]));
        let selected: HashSet<_> = [547, 549].iter().copied().collect();
        let edges = classify_edges(&selected, Some(49), &metas);
        assert_eq!(edges[&549], vec![Edge::InSlice(547)]);
    }

    #[test]
    fn closed_external_dep_is_dropped_class() {
        let mut metas = HashMap::new();
        metas.insert(549, meta(549, IssueState::Open, Some(49), vec![900]));
        metas.insert(900, meta(900, IssueState::Closed, Some(49), vec![]));
        let selected: HashSet<_> = [549].iter().copied().collect();
        let edges = classify_edges(&selected, Some(49), &metas);
        assert_eq!(edges[&549], vec![Edge::ClosedExternal(900)]);
    }

    #[test]
    fn same_milestone_open_external() {
        let mut metas = HashMap::new();
        metas.insert(549, meta(549, IssueState::Open, Some(49), vec![650]));
        metas.insert(650, meta(650, IssueState::Open, Some(49), vec![]));
        let selected: HashSet<_> = [549].iter().copied().collect();
        let edges = classify_edges(&selected, Some(49), &metas);
        assert_eq!(edges[&549], vec![Edge::SameMilestoneOpenExternal(650)]);
    }

    #[test]
    fn cross_milestone_open_external() {
        let mut metas = HashMap::new();
        metas.insert(549, meta(549, IssueState::Open, Some(49), vec![999]));
        metas.insert(999, meta(999, IssueState::Open, Some(50), vec![]));
        let selected: HashSet<_> = [549].iter().copied().collect();
        let edges = classify_edges(&selected, Some(49), &metas);
        assert_eq!(edges[&549], vec![Edge::CrossMilestoneOpenExternal(999)]);
    }
}
```

- [ ] **Step 2: Run + commit**

```bash
cargo test -p maestro --lib orchestration::dag::classify_tests
git add src/orchestration/dag.rs
git commit -m "feat(orchestration): add edge classification"
```

### Task 2.4: Topological levels (Kahn's) + cycle detection

**Files:**
- Modify: `src/orchestration/dag.rs`

- [ ] **Step 1: Write failing tests**

Append to `src/orchestration/dag.rs`:

```rust
/// Build levels via Kahn's algorithm. Each level = set of issues with all
/// in-slice deps satisfied by prior levels.
pub fn topo_levels(
    selected: &HashSet<IssueNumber>,
    edges: &HashMap<IssueNumber, Vec<Edge>>,
) -> Result<Vec<Vec<IssueNumber>>> {
    use anyhow::anyhow;

    // Build in-degree counting only InSlice edges.
    let mut in_degree: HashMap<IssueNumber, u32> = selected.iter().map(|n| (*n, 0)).collect();
    let mut adj: HashMap<IssueNumber, Vec<IssueNumber>> = HashMap::new();
    for (&from, es) in edges {
        for e in es {
            if let Edge::InSlice(dep) = e {
                *in_degree.entry(from).or_insert(0) += 1;
                adj.entry(*dep).or_default().push(from);
            }
        }
    }

    let mut levels: Vec<Vec<IssueNumber>> = Vec::new();
    let mut remaining: HashSet<_> = selected.iter().copied().collect();
    while !remaining.is_empty() {
        let level: Vec<_> = remaining.iter()
            .copied()
            .filter(|n| in_degree.get(n).copied().unwrap_or(0) == 0)
            .collect();
        if level.is_empty() {
            return Err(anyhow!(
                "cycle in dependency graph: {:?}",
                remaining.iter().copied().collect::<Vec<_>>()
            ));
        }
        for &n in &level {
            remaining.remove(&n);
            if let Some(children) = adj.get(&n) {
                for c in children {
                    if let Some(d) = in_degree.get_mut(c) {
                        *d = d.saturating_sub(1);
                    }
                }
            }
        }
        let mut sorted = level;
        sorted.sort();
        levels.push(sorted);
    }
    Ok(levels)
}

#[cfg(test)]
mod topo_tests {
    use super::*;

    #[test]
    fn linear_chain_produces_three_levels() {
        // 547 → 549 → 552
        let selected: HashSet<_> = [547, 549, 552].iter().copied().collect();
        let mut edges = HashMap::new();
        edges.insert(549, vec![Edge::InSlice(547)]);
        edges.insert(552, vec![Edge::InSlice(549)]);
        let levels = topo_levels(&selected, &edges).unwrap();
        assert_eq!(levels, vec![vec![547], vec![549], vec![552]]);
    }

    #[test]
    fn parallel_leaves_share_level_zero() {
        let selected: HashSet<_> = [547, 549, 650].iter().copied().collect();
        let levels = topo_levels(&selected, &HashMap::new()).unwrap();
        assert_eq!(levels.len(), 1);
        let mut l0 = levels[0].clone();
        l0.sort();
        assert_eq!(l0, vec![547, 549, 650]);
    }

    #[test]
    fn cycle_returns_error() {
        let selected: HashSet<_> = [1, 2].iter().copied().collect();
        let mut edges = HashMap::new();
        edges.insert(1, vec![Edge::InSlice(2)]);
        edges.insert(2, vec![Edge::InSlice(1)]);
        assert!(topo_levels(&selected, &edges).is_err());
    }
}
```

- [ ] **Step 2: Run + commit**

```bash
cargo test -p maestro --lib orchestration::dag::topo_tests
git add src/orchestration/dag.rs
git commit -m "feat(orchestration): add topological levels with cycle detection"
```

### Task 2.5: Auto-add expansion with bound

**Files:**
- Modify: `src/orchestration/dag.rs`

- [ ] **Step 1: Write failing test**

```rust
/// Auto-add same-milestone open-external deps to the selection.
/// Bound: refuses to expand if expansion would more than double the original count.
/// Single-pass (does NOT recurse).
pub fn auto_expand(
    selected: HashSet<IssueNumber>,
    edges: &HashMap<IssueNumber, Vec<Edge>>,
) -> ExpandResult {
    let original_count = selected.len();
    let mut expanded = selected.clone();
    let mut added: Vec<IssueNumber> = Vec::new();
    for es in edges.values() {
        for e in es {
            if let Edge::SameMilestoneOpenExternal(d) = e {
                if !expanded.contains(d) {
                    expanded.insert(*d);
                    added.push(*d);
                }
            }
        }
    }
    if expanded.len() > original_count.saturating_mul(2) {
        return ExpandResult::TooLarge {
            original: original_count,
            would_be: expanded.len(),
            added,
        };
    }
    if added.is_empty() {
        return ExpandResult::NoChange { selected };
    }
    added.sort();
    ExpandResult::Expanded { selected: expanded, added }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExpandResult {
    NoChange { selected: HashSet<IssueNumber> },
    Expanded { selected: HashSet<IssueNumber>, added: Vec<IssueNumber> },
    TooLarge { original: usize, would_be: usize, added: Vec<IssueNumber> },
}

#[cfg(test)]
mod expand_tests {
    use super::*;

    #[test]
    fn expands_two_into_three_when_one_dep() {
        let selected: HashSet<_> = [549, 552].iter().copied().collect();
        let mut edges = HashMap::new();
        edges.insert(552, vec![Edge::SameMilestoneOpenExternal(547)]);
        let r = auto_expand(selected, &edges);
        match r {
            ExpandResult::Expanded { added, .. } => assert_eq!(added, vec![547]),
            _ => panic!("expected Expanded"),
        }
    }

    #[test]
    fn refuses_when_over_2x_bound() {
        let selected: HashSet<_> = [1].iter().copied().collect();
        let mut edges = HashMap::new();
        edges.insert(1, vec![
            Edge::SameMilestoneOpenExternal(2),
            Edge::SameMilestoneOpenExternal(3),
            Edge::SameMilestoneOpenExternal(4),
        ]);
        let r = auto_expand(selected, &edges);
        assert!(matches!(r, ExpandResult::TooLarge { .. }));
    }
}
```

- [ ] **Step 2: Run + commit**

```bash
cargo test -p maestro --lib orchestration::dag::expand_tests
git add src/orchestration/dag.rs
git commit -m "feat(orchestration): add bounded auto-expansion of same-milestone deps"
```

### Task 2.6: Pre-flight pipeline assembly

**Files:**
- Create: `src/orchestration/preflight.rs`

- [ ] **Step 1: Write failing test (skeleton — full integration with doctor lives in Chunk 4)**

Create `src/orchestration/preflight.rs`:

```rust
//! Pre-flight validation pipeline. Spec §8 "Pre-flight is sacred".

use crate::orchestration::team::ResolvedTeam;
use crate::orchestration::validation::{validate, ValidationError};

#[derive(Debug, Clone)]
pub enum PreflightFailure {
    Validation(Vec<ValidationError>),
    L2ProviderUnavailable,
    AgentUnhealthy { id: String, reason: String },
    DagCycle(String),
    MalformedBlockedBy { issue: u64, snippet: String },
}

/// Sync portion of pre-flight. The async health-check portion is added in
/// Chunk 4 once #550 doctor is wired up.
pub fn preflight_sync(
    team: &ResolvedTeam,
    known_agents: &[String],
    known_modes: &[String],
) -> Result<(), PreflightFailure> {
    if let Err(errs) = validate(team, known_agents, known_modes) {
        return Err(PreflightFailure::Validation(errs));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::team::{RoleBinding, SourceTier};
    use crate::orchestration::types::{Primitive, Role};
    use std::collections::HashMap;

    #[test]
    fn passes_for_valid_team() {
        let mut bindings = HashMap::new();
        bindings.insert(Role::Reviewer, RoleBinding {
            agent: "claude".into(),
            mode: None, model_override: None, prompt_addendum: None, fallback_agent: None,
        });
        let team = ResolvedTeam {
            name: "default-reviewer".into(),
            primitive: Primitive::SinglePass,
            min_agents: vec!["claude".into()],
            bindings,
            source_tier: SourceTier::BuiltIn,
        };
        assert!(preflight_sync(&team, &["claude".into()], &[]).is_ok());
    }
}
```

- [ ] **Step 2: Run + commit**

```bash
cargo test -p maestro --lib orchestration::preflight
git add src/orchestration/
git commit -m "feat(orchestration): add preflight pipeline skeleton"
```

### Task 2.7: Scheduler skeleton — `Scheduler::new`, `plan()`, `next_ready()`

**Files:**
- Create: `src/orchestration/scheduler.rs`

- [ ] **Step 1: Write failing tests**

Create `src/orchestration/scheduler.rs`:

```rust
//! L3 cross-issue scheduler. Spec §5.

use crate::orchestration::dag::{IssueMeta, classify_edges, topo_levels};
use crate::orchestration::team::ResolvedTeam;
use crate::orchestration::types::TeamInput;
use crate::state::types::{IssueNumber, IssueRunState, TeamRun};
use anyhow::Result;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub struct Scheduler {
    pub team: ResolvedTeam,
    pub run: TeamRun,
    pub max_parallel: usize,
}

impl Scheduler {
    pub fn from_input(
        team: ResolvedTeam,
        input: TeamInput,
        metas: HashMap<IssueNumber, IssueMeta>,
        max_parallel: usize,
    ) -> Result<Self> {
        let (selected, primary_ms) = match input {
            TeamInput::Issue { number } => (HashSet::from([number]), None),
            TeamInput::IssueSet { primary_milestone, issues } => {
                (issues.into_iter().collect(), primary_milestone)
            }
            TeamInput::IdeaInbox => (HashSet::new(), None),
        };

        let edges = classify_edges(&selected, primary_ms, &metas);
        let levels = topo_levels(&selected, &edges)?;

        let mut state = HashMap::new();
        for &n in &selected {
            state.insert(n, IssueRunState::Queued);
        }
        let run = TeamRun {
            id: Uuid::new_v4(),
            team_name: team.name.clone(),
            started_at: Utc::now(),
            plan: levels,
            state,
        };
        Ok(Self { team, run, max_parallel })
    }

    /// Returns issues that are eligible to spawn now: queued, no in-slice deps
    /// outstanding, room in the semaphore.
    pub fn next_ready(&self) -> Vec<IssueNumber> {
        let in_flight = self
            .run
            .state
            .values()
            .filter(|s| matches!(s, IssueRunState::InFlight { .. }))
            .count();
        let slots = self.max_parallel.saturating_sub(in_flight);
        if slots == 0 {
            return Vec::new();
        }

        let mut ready: Vec<IssueNumber> = Vec::new();
        for level in &self.run.plan {
            let any_unfinished_in_level = level.iter().any(|n| {
                matches!(self.run.state.get(n), Some(IssueRunState::Queued | IssueRunState::InFlight { .. }))
            });
            for &n in level {
                if let Some(IssueRunState::Queued) = self.run.state.get(&n) {
                    ready.push(n);
                }
            }
            if any_unfinished_in_level {
                break; // do not look at next level until this one closes
            }
        }
        ready.truncate(slots);
        ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::team::{RoleBinding, SourceTier};
    use crate::orchestration::types::{Primitive, Role};

    fn solo_team() -> ResolvedTeam {
        let mut bindings = HashMap::new();
        bindings.insert(Role::Reviewer, RoleBinding {
            agent: "claude".into(),
            mode: None, model_override: None, prompt_addendum: None, fallback_agent: None,
        });
        ResolvedTeam {
            name: "default-reviewer".into(),
            primitive: Primitive::SinglePass,
            min_agents: vec!["claude".into()],
            bindings,
            source_tier: SourceTier::BuiltIn,
        }
    }

    #[test]
    fn single_issue_is_ready_immediately() {
        let mut metas = HashMap::new();
        metas.insert(547, IssueMeta {
            number: 547, state: crate::orchestration::dag::IssueState::Open,
            milestone: Some(49), blocked_by: vec![],
        });
        let s = Scheduler::from_input(
            solo_team(),
            TeamInput::Issue { number: 547 },
            metas,
            3,
        ).unwrap();
        assert_eq!(s.next_ready(), vec![547]);
    }

    #[test]
    fn second_level_blocked_until_first_done() {
        let mut metas = HashMap::new();
        metas.insert(547, IssueMeta { number: 547, state: crate::orchestration::dag::IssueState::Open, milestone: Some(49), blocked_by: vec![] });
        metas.insert(549, IssueMeta { number: 549, state: crate::orchestration::dag::IssueState::Open, milestone: Some(49), blocked_by: vec![547] });
        let mut s = Scheduler::from_input(
            solo_team(),
            TeamInput::IssueSet { primary_milestone: Some(49), issues: vec![547, 549] },
            metas,
            3,
        ).unwrap();
        assert_eq!(s.next_ready(), vec![547]);

        // Mark 547 in-flight then succeeded.
        s.run.state.insert(547, IssueRunState::Succeeded {
            output: crate::orchestration::types::TeamOutput::Pr { number: 1, branch: "x".into() }
        });
        assert_eq!(s.next_ready(), vec![549]);
    }
}
```

- [ ] **Step 2: Run + commit**

```bash
cargo test -p maestro --lib orchestration::scheduler
git add src/orchestration/
git commit -m "feat(orchestration): add scheduler with semaphore-based next_ready"
```

### Task 2.8: Restart reconciliation

**Files:**
- Modify: `src/state/store.rs`

- [ ] **Step 1: Write failing test**

Add to the existing `src/state/store.rs` tests (or create a new test file under `src/state/`):

```rust
#[cfg(test)]
#[test]
fn reconcile_pessimistic_marks_in_flight_as_failed() {
    use crate::state::types::{IssueRunState, TeamRun};
    use chrono::Utc;
    use std::collections::HashMap;
    use uuid::Uuid;

    let mut state = HashMap::new();
    state.insert(547, IssueRunState::InFlight {
        session_id: Uuid::new_v4(),
        started_at: Utc::now(),
    });
    state.insert(549, IssueRunState::Succeeded {
        output: crate::orchestration::types::TeamOutput::Pr { number: 1, branch: "x".into() }
    });

    let mut run = TeamRun {
        id: Uuid::new_v4(),
        team_name: "x".into(),
        started_at: Utc::now(),
        plan: vec![vec![547, 549]],
        state,
    };

    crate::state::store::reconcile_team_run(&mut run);

    match run.state.get(&547).unwrap() {
        IssueRunState::Failed { reason, .. } => {
            assert!(reason.contains("process state lost across restart"));
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    // 549 untouched
    assert!(matches!(run.state.get(&549).unwrap(), IssueRunState::Succeeded { .. }));
}
```

- [ ] **Step 2: Add the function**

```rust
// src/state/store.rs
use crate::state::types::{IssueRunState, TeamRun};

pub fn reconcile_team_run(run: &mut TeamRun) {
    for state in run.state.values_mut() {
        if let IssueRunState::InFlight { .. } = state {
            *state = IssueRunState::Failed {
                reason: "process state lost across restart".into(),
                attempts: 0,
            };
        }
    }
}
```

- [ ] **Step 3: Wire into the existing store-load path**

Find where the state store loads on startup (likely `Store::load` or similar in `src/state/store.rs`). After deserialization, iterate over loaded `TeamRun`s and call `reconcile_team_run` on each.

- [ ] **Step 4: Test + commit**

```bash
cargo test -p maestro --lib state::store
git add src/state/
git commit -m "feat(state): pessimistic restart reconciliation for TeamRun InFlight"
```

---

## Chunk 3: L2 Per-Issue Orchestrator + Primitives

**Goal:** Implement the per-issue Claude orchestrator session and the four primitive state machines. After this chunk, given a queued issue and a `ResolvedTeam`, the L2 driver can emit the L2 system prompt, expose the constrained tool set, and route `Task()` calls to a stub L1.

### Task 3.1: Primitive state-machine trait

**Files:**
- Create: `src/orchestration/primitives/mod.rs`

- [ ] **Step 1: Write failing test for primitive registry**

```rust
//! Primitive state machines. One file per primitive.

use crate::orchestration::contracts::SubagentResult;
use crate::orchestration::types::{Primitive, Role, TeamOutput};
use crate::state::types::IssueNumber;

pub mod pipeline;
pub mod fan_out;
pub mod single_pass;
pub mod verdict_only;

/// One step in a primitive's execution. The L2 orchestrator dispatches each
/// step as a Task() call and feeds the result back via `advance`.
#[derive(Debug, Clone)]
pub enum NextStep {
    Dispatch { role: Role, instructions: String },
    Done { output: TeamOutput },
    Fail { reason: String },
}

pub trait PrimitiveMachine: Send {
    fn next(&mut self) -> NextStep;
    fn advance(&mut self, role: Role, result: Result<SubagentResult, crate::orchestration::contracts::SubagentError>);
}

pub fn make_machine(
    primitive: Primitive,
    issue: IssueNumber,
) -> Box<dyn PrimitiveMachine> {
    match primitive {
        Primitive::Pipeline => Box::new(pipeline::PipelineMachine::new(issue)),
        Primitive::FanOut => Box::new(fan_out::FanOutMachine::new(issue)),
        Primitive::SinglePass => Box::new(single_pass::SinglePassMachine::new(issue)),
        Primitive::VerdictOnly => Box::new(verdict_only::VerdictOnlyMachine::new(issue)),
    }
}
```

- [ ] **Step 2: Stub each primitive file**

Create each of `pipeline.rs`, `fan_out.rs`, `single_pass.rs`, `verdict_only.rs` under `src/orchestration/primitives/`. Each starts as a stub that compiles.

Example `pipeline.rs`:

```rust
use super::{NextStep, PrimitiveMachine};
use crate::orchestration::contracts::{SubagentError, SubagentResult};
use crate::orchestration::types::Role;
use crate::state::types::IssueNumber;

pub struct PipelineMachine {
    issue: IssueNumber,
    step: PipelineStep,
    last_summary: String,
}

#[derive(Debug)]
enum PipelineStep {
    Implementer,
    Reviewer { implementer_summary: String },
    Docs { reviewer_verdict: String },
    Done { branch: String, pr_number: u64 },
    Failed(String),
}

impl PipelineMachine {
    pub fn new(issue: IssueNumber) -> Self {
        Self { issue, step: PipelineStep::Implementer, last_summary: String::new() }
    }
}

impl PrimitiveMachine for PipelineMachine {
    fn next(&mut self) -> NextStep {
        match &self.step {
            PipelineStep::Implementer => NextStep::Dispatch {
                role: Role::Implementer,
                instructions: format!("Implement issue #{}.", self.issue),
            },
            PipelineStep::Reviewer { implementer_summary } => NextStep::Dispatch {
                role: Role::Reviewer,
                instructions: format!(
                    "Review the implementer's diff for issue #{}. Implementer summary: {}",
                    self.issue, implementer_summary
                ),
            },
            PipelineStep::Docs { reviewer_verdict } => NextStep::Dispatch {
                role: Role::Docs,
                instructions: format!(
                    "Update docs for issue #{}. Review verdict was: {}",
                    self.issue, reviewer_verdict
                ),
            },
            PipelineStep::Done { branch, pr_number } => NextStep::Done {
                output: crate::orchestration::types::TeamOutput::Pr {
                    number: *pr_number,
                    branch: branch.clone(),
                },
            },
            PipelineStep::Failed(reason) => NextStep::Fail { reason: reason.clone() },
        }
    }

    fn advance(&mut self, role: Role, result: Result<SubagentResult, SubagentError>) {
        match (role, result, &self.step) {
            (Role::Implementer, Ok(SubagentResult::CodeChange { summary, commit_sha, .. }), _) => {
                self.last_summary = summary.clone();
                self.step = PipelineStep::Reviewer { implementer_summary: summary };
                let _ = commit_sha; // use later for PR creation
            }
            (Role::Reviewer, Ok(SubagentResult::ReviewFindings { verdict, .. }), _) => {
                self.step = PipelineStep::Docs {
                    reviewer_verdict: format!("{verdict:?}"),
                };
            }
            (Role::Docs, Ok(SubagentResult::DocsChange { .. }), _) => {
                // PR creation happens in L2 driver; here we just signal Done.
                self.step = PipelineStep::Done {
                    branch: format!("feat/{}", self.issue),
                    pr_number: 0, // filled by L2
                };
            }
            (_, Err(e), _) => {
                self.step = PipelineStep::Failed(format!("{e}"));
            }
            _ => {
                self.step = PipelineStep::Failed(format!(
                    "unexpected role/result combination at step {:?}",
                    self.step
                ));
            }
        }
    }
}
```

Stub the other three machines similarly (each is shorter than `PipelineMachine`).

- [ ] **Step 3: Add tests for each machine**

Each `*_tests` module verifies: success path, failure path, malformed-result path. Example for `single_pass.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::contracts::{ReviewVerdict, SubagentResult};
    use crate::orchestration::primitives::NextStep;

    #[test]
    fn dispatches_then_completes() {
        let mut m = SinglePassMachine::new(123);
        // First call: dispatch.
        match m.next() {
            NextStep::Dispatch { role, .. } => assert_eq!(role, Role::Reviewer),
            _ => panic!(),
        }
        // Advance with success.
        m.advance(Role::Reviewer, Ok(SubagentResult::ReviewFindings {
            verdict: ReviewVerdict::Approved,
            findings: vec![],
        }));
        // Second call: done.
        assert!(matches!(m.next(), NextStep::Done { .. }));
    }
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p maestro --lib orchestration::primitives
git add src/orchestration/
git commit -m "feat(orchestration): add four primitive state machines"
```

### Task 3.2: L2 system-prompt template + tool surface

**Files:**
- Create: `src/orchestration/orchestrator.rs`

- [ ] **Step 1: Write failing test for prompt template**

```rust
//! L2 per-issue orchestrator. Drives a Claude session with restricted tools.
//! Spec §3 "L2 — Per-issue LLM orchestrator".

use crate::orchestration::team::ResolvedTeam;
use crate::orchestration::types::Primitive;
use crate::state::types::IssueNumber;

pub fn build_system_prompt(team: &ResolvedTeam, issue: IssueNumber) -> String {
    let primitive_name = match team.primitive {
        Primitive::Pipeline => "pipeline",
        Primitive::FanOut => "fan-out",
        Primitive::SinglePass => "single-pass",
        Primitive::VerdictOnly => "verdict-only",
    };
    let pr_create_clause = if matches!(team.primitive, Primitive::Pipeline) {
        "GhPrCreate (only at the terminal step), "
    } else { "" };
    format!(
        r#"You orchestrate a `{primitive_name}` team for issue #{issue}. You do NOT read the issue body.
Your only verbs are: Task(role=...), {pr_create_clause}ReportFailure(reason).
After each Task returns, decide: continue / retry-different-provider / fail.
Output a single typed result when the primitive's state machine reaches the terminal state.

Forbidden: Read, Edit, Write, Bash, Grep — all delegated to subagents."#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::team::{RoleBinding, SourceTier};
    use crate::orchestration::types::Role;
    use std::collections::HashMap;

    fn pipeline_team() -> ResolvedTeam {
        let mut bindings = HashMap::new();
        for r in [Role::Implementer, Role::Reviewer, Role::Docs] {
            bindings.insert(r, RoleBinding {
                agent: "claude".into(),
                mode: None, model_override: None, prompt_addendum: None, fallback_agent: None,
            });
        }
        ResolvedTeam {
            name: "default-coder".into(),
            primitive: Primitive::Pipeline,
            min_agents: vec!["claude".into()],
            bindings,
            source_tier: SourceTier::BuiltIn,
        }
    }

    #[test]
    fn prompt_includes_primitive_and_issue_number() {
        let p = build_system_prompt(&pipeline_team(), 547);
        assert!(p.contains("pipeline"));
        assert!(p.contains("#547"));
        assert!(p.contains("GhPrCreate"));
        assert!(p.contains("Forbidden:"));
    }

    #[test]
    fn non_pipeline_omits_pr_create() {
        let mut t = pipeline_team();
        t.primitive = Primitive::SinglePass;
        let p = build_system_prompt(&t, 1);
        assert!(!p.contains("GhPrCreate"));
    }
}
```

- [ ] **Step 2: Run + commit**

```bash
cargo test -p maestro --lib orchestration::orchestrator
git add src/orchestration/
git commit -m "feat(orchestration): add L2 system-prompt template"
```

### Task 3.3: Mock `Task()` test infrastructure

**Files:**
- Create: `tests/orchestration/mock_task.rs`
- Create: `tests/orchestration/mod.rs`

- [ ] **Step 1: Write the mock module**

```rust
//! Mock Task() for integration tests. See spec §9 mock-ownership note.
//!
//! Replaces the L2 session's tool-result channel with canned payloads,
//! so tests don't spawn a real Claude binary.

use maestro::orchestration::contracts::{SubagentError, SubagentResult};
use maestro::orchestration::types::Role;
use std::collections::VecDeque;
use std::sync::Mutex;

pub struct MockTaskQueue {
    inner: Mutex<VecDeque<MockResponse>>,
}

#[derive(Debug, Clone)]
pub struct MockResponse {
    pub role: Role,
    pub result: Result<SubagentResult, SubagentError>,
}

impl MockTaskQueue {
    pub fn new(queue: Vec<MockResponse>) -> Self {
        Self { inner: Mutex::new(queue.into()) }
    }

    pub fn next(&self, expected_role: Role) -> Option<Result<SubagentResult, SubagentError>> {
        let mut q = self.inner.lock().unwrap();
        let front = q.pop_front()?;
        assert_eq!(front.role, expected_role,
            "mock got Task(role={:?}), but next queued was for role {:?}",
            expected_role, front.role);
        Some(front.result)
    }
}
```

- [ ] **Step 2: Add `tests/orchestration/mod.rs`**

```rust
pub mod mock_task;
```

- [ ] **Step 3: Add an integration test that exercises the pipeline machine end-to-end with the mock**

```rust
#[test]
fn pipeline_drives_to_done_with_mock() {
    use maestro::orchestration::primitives::{make_machine, NextStep};
    use maestro::orchestration::types::Primitive;
    use maestro::orchestration::contracts::{SubagentResult, ReviewVerdict};

    let queue = MockTaskQueue::new(vec![
        MockResponse {
            role: Role::Implementer,
            result: Ok(SubagentResult::CodeChange {
                files_touched: vec!["src/foo.rs".into()],
                summary: "added foo".into(),
                commit_sha: Some("abc123".into()),
            }),
        },
        MockResponse {
            role: Role::Reviewer,
            result: Ok(SubagentResult::ReviewFindings {
                verdict: ReviewVerdict::Approved,
                findings: vec![],
            }),
        },
        MockResponse {
            role: Role::Docs,
            result: Ok(SubagentResult::DocsChange {
                files_touched: vec!["docs/foo.md".into()],
                summary: "doc updated".into(),
            }),
        },
    ]);

    let mut m = make_machine(Primitive::Pipeline, 547);
    loop {
        match m.next() {
            NextStep::Dispatch { role, .. } => {
                let r = queue.next(role).expect("queue empty");
                m.advance(role, r);
            }
            NextStep::Done { .. } => return, // success
            NextStep::Fail { reason } => panic!("unexpected fail: {reason}"),
        }
    }
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test --test orchestration
git add tests/
git commit -m "test(orchestration): add MockTaskQueue and pipeline integration test"
```

---

## Chunk 4: L1 Subagent Dispatch + Cost Estimate

**Goal:** Implement the L1 layer that translates a `Task(role)` call into a real provider session via v0.25.0's `ManagedSession`, plus the locked cost-estimate formula.

### Task 4.1: Make doctor health check a library function

**Files:**
- Modify: `src/commands/doctor.rs`
- Modify: `src/agent_provider/types.rs` (verify `AgentHealthCheck` is `pub`)

- [ ] **Step 1: Inspect current shape**

```bash
grep -n 'health_check\|AgentHealthCheck' src/commands/doctor.rs src/agent_provider/types.rs | head -20
```

- [ ] **Step 2: Extract the per-provider health-check loop into a library function**

Add to `src/commands/doctor.rs`:

```rust
/// Library-callable health check for a list of agents. Pre-flight (orchestration)
/// uses this directly instead of spawning `maestro doctor` as a subprocess.
pub async fn run_health_check(agent_ids: &[String]) -> Vec<AgentHealthCheck> {
    let mut out = Vec::new();
    for id in agent_ids {
        let result = check_one_agent(id).await;
        out.push(result);
    }
    out
}
```

Refactor the existing `doctor` command to call `run_health_check` internally.

- [ ] **Step 3: Test the library function**

Add a small integration test that calls `run_health_check(&["claude".into()])` and asserts the return shape (without asserting healthy/unhealthy — that depends on the test environment).

- [ ] **Step 4: Commit**

```bash
git add src/commands/doctor.rs src/agent_provider/types.rs
git commit -m "feat(doctor): expose run_health_check as library function"
```

### Task 4.2: L1 dispatch — `dispatch_subagent`

**Files:**
- Create: `src/orchestration/dispatch.rs`

- [ ] **Step 1: Write failing test using a stub `AgentProvider`**

```rust
//! L1 subagent dispatch. Spec §3 "L1 — Subagent dispatch".

use crate::agent_provider::types::{AgentProvider, AgentRequest, StreamEvent};
use crate::config::Config;
use crate::orchestration::contracts::{SubagentError, SubagentResult};
use crate::orchestration::team::{ResolvedTeam, RoleBinding};
use crate::orchestration::types::Role;
use anyhow::{Context, Result};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct DispatchContext<'a> {
    pub config: &'a Config,
    pub team: &'a ResolvedTeam,
    pub worktree: std::path::PathBuf,
    pub cancel: CancellationToken,
}

pub async fn dispatch_subagent(
    ctx: &DispatchContext<'_>,
    role: Role,
    instructions: String,
) -> Result<SubagentResult, SubagentError> {
    let binding = ctx.team.bindings.get(&role)
        .ok_or_else(|| SubagentError::Other(format!("role {role:?} not bound in team {}", ctx.team.name)))?;
    let provider = build_provider(ctx.config, binding)
        .map_err(|e| SubagentError::Other(format!("provider build: {e:#}")))?;

    let prompt = compose_prompt(ctx, binding, &instructions);
    let request = AgentRequest {
        prompt,
        worktree: ctx.worktree.clone(),
        // … other fields per AgentProvider trait
    };

    let (tx, mut rx) = mpsc::unbounded_channel::<StreamEvent>();
    let cancel = ctx.cancel.clone();
    let provider_handle = tokio::spawn(async move {
        provider.run(request, tx, cancel).await
    });

    // Aggregate stream into a SubagentResult per role's expected shape.
    let mut buffer = String::new();
    while let Some(ev) = rx.recv().await {
        if let StreamEvent::AssistantText(s) = ev {
            buffer.push_str(&s);
        }
    }
    let _ = provider_handle.await;

    parse_result(role, &buffer)
}

fn build_provider(
    config: &Config,
    binding: &RoleBinding,
) -> Result<Box<dyn AgentProvider>> {
    let agent_cfg = config.agents.get(&binding.agent)
        .with_context(|| format!("agent {} not in config", binding.agent))?;
    crate::agent_provider::factory::build(agent_cfg.clone())
}

fn compose_prompt(ctx: &DispatchContext, binding: &RoleBinding, instructions: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(mode_id) = &binding.mode {
        if let Some(mode) = ctx.config.modes.get(mode_id) {
            parts.push(mode.system_prompt.clone());
        }
    }
    if let Some(addendum) = &binding.prompt_addendum {
        parts.push(addendum.clone());
    }
    parts.push(instructions.to_string());
    parts.join("\n\n")
}

fn parse_result(role: Role, raw: &str) -> Result<SubagentResult, SubagentError> {
    // Try JSON parse first (subagents may output structured JSON in the role's
    // declared shape). On failure, wrap in `Generic` if role allows it; else
    // return ResultShapeMismatch.
    match serde_json::from_str::<SubagentResult>(raw) {
        Ok(r) => Ok(r),
        Err(_) => {
            if role.allowed_results().contains(&"generic") {
                Ok(SubagentResult::Generic { json: serde_json::Value::String(raw.to_string()) })
            } else {
                Err(SubagentError::ResultShapeMismatch {
                    role,
                    expected: role.allowed_results().join("|"),
                    got: format!("plain text ({} bytes)", raw.len()),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::contracts::{ReviewVerdict, SubagentResult};

    #[test]
    fn parse_review_findings_from_json() {
        let raw = r#"{"kind":"review-findings","verdict":"approved","findings":[]}"#;
        let r = parse_result(Role::Reviewer, raw).unwrap();
        match r {
            SubagentResult::ReviewFindings { verdict, .. } => assert_eq!(verdict, ReviewVerdict::Approved),
            _ => panic!(),
        }
    }

    #[test]
    fn implementer_plain_text_returns_shape_mismatch() {
        let err = parse_result(Role::Implementer, "just some text").unwrap_err();
        assert!(matches!(err, SubagentError::ResultShapeMismatch { .. }));
    }
}
```

- [ ] **Step 2: Run + commit**

```bash
cargo test -p maestro --lib orchestration::dispatch
git add src/orchestration/
git commit -m "feat(orchestration): add L1 subagent dispatch"
```

### Task 4.3: Cost estimate

**Files:**
- Create: `src/orchestration/cost.rs`

- [ ] **Step 1: Write failing test (verify formula constants are derivable)**

```rust
//! Cost-estimate formula per spec §7.

use crate::orchestration::team::ResolvedTeam;
use crate::orchestration::types::Primitive;
use std::collections::HashMap;

const L2_SYSTEM_PROMPT_TOKENS: u32 = 200;
const AVG_ISSUE_CONTEXT_TOKENS: u32 = 800;
const RECOVERY_BUDGET: u32 = 3;
const TOKENS_PER_DISPATCH: u32 = 300;

/// Per-provider cost in USD per 1k tokens. Static. Updated only on release.
fn cost_per_1k_tokens() -> HashMap<&'static str, f64> {
    HashMap::from([
        ("claude", 0.015),     // Opus rate, conservative
        ("codex", 0.020),      // GPT-5 rate
        ("opencode", 0.010),   // mid-range pass-through; varies per backend
        ("ollama", 0.0),       // local
        ("minimax", 0.005),    // cloud cheap
        ("qwen", 0.015),       // similar to Claude
    ])
}

pub fn estimate_tokens(team: &ResolvedTeam, num_issues: u32, avg_role_prompt_tokens: u32) -> u32 {
    let num_roles = team.bindings.len() as u32;
    let per_issue = L2_SYSTEM_PROMPT_TOKENS
        + (avg_role_prompt_tokens.saturating_mul(num_roles))
        + AVG_ISSUE_CONTEXT_TOKENS
        + (TOKENS_PER_DISPATCH * num_roles * RECOVERY_BUDGET);
    per_issue.saturating_mul(num_issues)
}

pub fn estimate_cost_usd(team: &ResolvedTeam, num_issues: u32, avg_role_prompt_tokens: u32) -> f64 {
    let prices = cost_per_1k_tokens();
    let mut total = 0.0;
    let tokens_per_issue = estimate_tokens(team, num_issues, avg_role_prompt_tokens) / num_issues.max(1);
    for (_role, binding) in &team.bindings {
        let rate = prices.get(binding.agent.as_str()).copied().unwrap_or(0.015);
        total += (tokens_per_issue as f64 / 1000.0) * rate;
    }
    total * num_issues as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::team::{RoleBinding, SourceTier};
    use crate::orchestration::types::Role;
    use std::collections::HashMap;

    #[test]
    fn estimate_grows_with_issue_count() {
        let mut bindings = HashMap::new();
        bindings.insert(Role::Reviewer, RoleBinding {
            agent: "claude".into(),
            mode: None, model_override: None, prompt_addendum: None, fallback_agent: None,
        });
        let team = ResolvedTeam {
            name: "x".into(),
            primitive: Primitive::SinglePass,
            min_agents: vec!["claude".into()],
            bindings,
            source_tier: SourceTier::BuiltIn,
        };
        let one = estimate_tokens(&team, 1, 100);
        let three = estimate_tokens(&team, 3, 100);
        assert_eq!(three, 3 * one);
    }

    #[test]
    fn ollama_costs_zero() {
        let mut bindings = HashMap::new();
        bindings.insert(Role::Reviewer, RoleBinding {
            agent: "ollama".into(),
            mode: None, model_override: None, prompt_addendum: None, fallback_agent: None,
        });
        let team = ResolvedTeam {
            name: "x".into(),
            primitive: Primitive::SinglePass,
            min_agents: vec!["claude".into(), "ollama".into()],
            bindings,
            source_tier: SourceTier::BuiltIn,
        };
        assert_eq!(estimate_cost_usd(&team, 1, 100), 0.0);
    }
}
```

- [ ] **Step 2: Run + commit**

```bash
cargo test -p maestro --lib orchestration::cost
git add src/orchestration/
git commit -m "feat(orchestration): lock cost-estimate formula"
```

---

## Chunk 5: TUI Wizard

**Goal:** Implement the team_wizard TUI screen mirroring the existing `issue_wizard` / `milestone_wizard` pattern. Compose flow, launch flow, manage flow, plan-preview rendering, and entry-point keybindings on issue browser + milestone screen.

### Task 5.1: Wizard module skeleton

**Files:**
- Create: `src/tui/screens/team_wizard/{mod,draw,types}.rs`

- [ ] **Step 1: Mirror the existing wizard pattern**

Copy the structural pattern from `src/tui/screens/milestone_wizard/`. Each step is a variant on a `TeamWizardStep` enum; `mod.rs` holds the state machine; `draw.rs` renders.

```rust
// src/tui/screens/team_wizard/types.rs
use crate::orchestration::types::{Primitive, TeamInput};

#[derive(Debug, Clone)]
pub struct ComposePayload {
    pub source: SourceChoice,
    pub primitive: Option<Primitive>,
    pub bindings: Vec<(String, String)>, // (role_name, agent_id)
    // … overrides etc.
}

#[derive(Debug, Clone)]
pub enum SourceChoice {
    Blank,
    Extends(String),
}

#[derive(Debug, Clone)]
pub struct LaunchPayload {
    pub team_name: String,
    pub input: TeamInput,
}
```

- [ ] **Step 2: Step machine in `mod.rs`**

```rust
//! Team wizard screen. Mirrors milestone_wizard pattern.

mod draw;
mod types;
mod compose;
mod launch;
mod manage;

pub use types::{ComposePayload, LaunchPayload, SourceChoice};

pub enum TeamWizardStep {
    Home,
    Compose(compose::ComposeStep),
    Launch(launch::LaunchStep),
    Manage(manage::ManageStep),
    Done,
}

pub struct TeamWizardScreen {
    pub step: TeamWizardStep,
    pub compose: ComposePayload,
    pub launch: LaunchPayload,
}
```

- [ ] **Step 3: Compile-only test + commit**

```bash
cargo build -p maestro
git add src/tui/screens/team_wizard/
git commit -m "feat(tui): scaffold team wizard skeleton"
```

### Task 5.2: Compose flow steps 1–5

(Detailed mirror of milestone_wizard's step 1–5; each step renders a panel and accepts keyboard navigation. Snapshot tests at 80×24 / 60×20 / 120×40 per the existing snapshot policy. Insta acceptance via `cargo insta accept`.)

- [ ] **Step 1: Step 1 — Source picker.** Snapshot test.
- [ ] **Step 2: Step 2 — Primitive picker.** Snapshot test.
- [ ] **Step 3: Step 3 — Roles picker.** Filter dropdown to enabled-and-healthy agents (call `doctor::run_health_check` once at wizard entry; cache for the session). Snapshot test.
- [ ] **Step 4: Step 4 — Optional overrides.** Skip-able. Snapshot test.
- [ ] **Step 5: Step 5 — Save (name + tier).** Calls `Loader` validation; surface errors red. Snapshot test for each error case.
- [ ] **Step 6: Commit.**

### Task 5.3: Launch flow steps 1–4

- [ ] **Step 1: Step 1 — Team picker.** Default to most recent.
- [ ] **Step 2: Step 2 — Input picker.** Primitive determines picker shape.
- [ ] **Step 3: Step 3 — Plan preview.** Render levels via the scheduler from Chunk 2; display cost via Chunk 4. Snapshot test for the example from the design doc.
- [ ] **Step 4: Step 4 — Confirm + execute.** Hands off to scheduler; switches view to dashboard.
- [ ] **Step 5: Commit.**

### Task 5.4: Manage flow

- [ ] **Step 1: List user-tier presets** — table view.
- [ ] **Step 2: Edit (jumps into Compose flow with Source = Extends(self) pre-filled).**
- [ ] **Step 3: Delete (with confirmation).**
- [ ] **Step 4: Commit.**

### Task 5.5: Entry-point keybindings

**Files:**
- Modify: `src/tui/screens/issue_browser/mod.rs`
- Modify: `src/tui/screens/milestone/mod.rs` (or whatever the milestone screen's owning file is)
- Modify: `src/tui/app/mod.rs`

- [ ] **Step 1: Add `t` handler in issue_browser** — opens TeamWizardScreen with launch flow + issue pre-selected.
- [ ] **Step 2: Add `t` handler in milestone screen** — opens with launch flow + milestone slice pre-selected.
- [ ] **Step 3: Add `[t] Teams` to top-level menu** — wire into `src/tui/app/mod.rs` navigation.
- [ ] **Step 4: Snapshot test the help-bar entries.**
- [ ] **Step 5: Commit.**

### Task 5.6: Extend agent_graph for team membership

**Files:**
- Modify: `src/tui/agent_graph/model.rs`
- Modify: `src/tui/agent_graph/render.rs`

- [ ] **Step 1: Add `NodeId::Team(Uuid)` variant** — mirrors `NodeId::Agent` and `NodeId::File`.
- [ ] **Step 2: Add `GraphNode::Team` rendering** — a container box showing the team name; member agent nodes are nested visually.
- [ ] **Step 3: Snapshot test the agent graph with one team active.**
- [ ] **Step 4: Commit.**

---

## Chunk 6: CLI + Built-in Docs + State Migration + Smoke Test

**Goal:** Wire up `maestro team` CLI subcommands, write per-built-in docs, add a state-store migration for existing maestro state files, and add an end-to-end smoke test.

### Task 6.1: `maestro team` CLI surface

**Files:**
- Create: `src/cli/team.rs`
- Modify: `src/main.rs` (clap wiring)

- [ ] **Step 1: Write failing test for `maestro team list` output**

`maestro team list` should print all resolved teams with their tier and primitive.

- [ ] **Step 2: Implement subcommands**

```rust
// src/cli/team.rs
use clap::Subcommand;

#[derive(Subcommand)]
pub enum TeamCmd {
    List,
    New { name: String, #[arg(long)] extends: Option<String> },
    Launch { preset: String, #[arg(long)] issue: Option<u64>, #[arg(long)] milestone: Option<u64> },
    Manage,
    Explain { name: String },
}
```

- [ ] **Step 3: Wire each subcommand**:
  - `list` — `Loader::resolve()` → tabular output
  - `new` — opens TUI compose flow (or interactive prompts in headless mode)
  - `launch` — opens TUI launch flow at confirm step (or fully headless via `--yes` flag)
  - `manage` — opens TUI manage flow
  - `explain <name>` — prints the resolved bindings with provenance per field (mitigation for §12 risk #2)
- [ ] **Step 4: Test each subcommand** with a tempdir for project / user tiers.
- [ ] **Step 5: Commit.**

### Task 6.2: Per-built-in docs

**Files:**
- Create: `docs/teams/README.md` — index, with the comparison table from spec §4
- Create: `docs/teams/{default-coder,default-researcher,default-triager,default-reviewer,default-docs}.md`

- [ ] **Step 1: Write each doc**: setup, what it produces, example usage, customization tips. Each doc references the canonical TOML by `include_str!`-style code block.
- [ ] **Step 2: Verify all internal links resolve** — run a markdown link check.
- [ ] **Step 3: Commit.**

### Task 6.3: State-store version bump + migration

**Files:**
- Modify: `src/state/types.rs` (bump `STATE_VERSION` constant if one exists; otherwise add it)
- Modify: `src/state/store.rs` (migration on load)

- [ ] **Step 1: Find current version mechanism** — `grep -rn 'STATE_VERSION\|version.*=.*[0-9]' src/state/`
- [ ] **Step 2: Bump version + add migration shim**

If pre-team-run state files lack the `team_runs` field, the migration adds an empty `team_runs: vec![]` and rewrites on next save.

- [ ] **Step 3: Test loading a pre-team-run state file** (use a fixture in `tests/fixtures/state_pre_team_run.json`).
- [ ] **Step 4: Commit.**

### Task 6.4: End-to-end smoke test

**Files:**
- Create: `tests/orchestration/smoke.rs`

- [ ] **Step 1: Write the smoke test**

```rust
//! End-to-end: load default-coder, build a 2-issue plan from a stub gh
//! response, run with mocked Task() outputs, assert TeamRun reaches
//! all-Succeeded.

#[tokio::test]
async fn smoke_pipeline_two_issues() {
    // [Stub gh, build scheduler from default-coder + 2 issues with edges,
    // drive each via MockTaskQueue, assert run.state == all Succeeded.]
}
```

- [ ] **Step 2: Run + commit**

```bash
cargo test --test orchestration smoke
git add tests/orchestration/smoke.rs
git commit -m "test(orchestration): add end-to-end pipeline smoke test"
```

### Task 6.5: CHANGELOG + README

**Files:**
- Modify: `CHANGELOG.md`
- Modify: `README.md` (add a "Team orchestration (v0.27.0+)" section pointing to `docs/teams/`)

- [ ] **Step 1: Write CHANGELOG entry**: feature description, breaking changes (state-store version bump), new dependency (`directories` crate).
- [ ] **Step 2: Add README section** — short with link.
- [ ] **Step 3: Commit.**

### Task 6.6: Final verification

- [ ] **Step 1: `cargo fmt --check`**
- [ ] **Step 2: `cargo clippy -- -D warnings`**
- [ ] **Step 3: `cargo test`** — all chunks combined
- [ ] **Step 4: `cargo build --release`**
- [ ] **Step 5: Manual smoke** — `cargo run -- team list` should print 5 built-ins on a fresh user
- [ ] **Step 6: Commit + open PR**

---

## Decisions captured (from brainstorming + spec review)

- Manage mode included in v1 (open question §14 confirmed "keep").
- Cost estimate included in v1 with locked formula (§7); ±50% labeled.
- STEP 4 same-milestone OPEN-EXTERNAL auto-add stays as default (with 2x bound + visual diff).
- L2 is Claude-only in v1; future `[orchestrator]` config key is v2.
- `OpenAiCompatibleSseParser` consumed from v0.25.0 #652, not duplicated here.
- `model_override` validation against agent's known model list: TBD by plan author at implementation time per round-2 review note.

## What the implementer can ignore for v1

- Real-time team coordination (subagents talking).
- Cross-machine team execution.
- Team marketplaces / preset registry.
- Auto-tuning bindings.
- Team-aware merge train.
- `FileSet` TeamInput variant.
- Versioned `extends` syntax (`default-coder@v1`).

## Execution path

This plan should be executed via `superpowers:subagent-driven-development` (Claude Code has subagents). Fresh subagent per task, two-stage review per the skill's flow.
