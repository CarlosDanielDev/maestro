//! Role taxonomy for agent personalities — spike prototype.
//!
//! See `docs/adr/002-agent-personalities.md` § Role Taxonomy. The canonical
//! list is five: `Implementer` (default), `Orchestrator`, `Reviewer`, `Docs`,
//! `DevOps`. The `derive_role` classifier mirrors `crate::session::intent`'s
//! keyword-matching idiom so the follow-up can lift it to `src/session/role.rs`
//! unchanged.

use serde::{Deserialize, Serialize};

/// Five-role taxonomy for the agent-personalities spike.
///
/// `Implementer` is the default because it is the largest category in maestro's
/// session log; "unknown prompt → Implementer" is the safest miscategorization
/// (most sessions do work, so misfires are visually invisible).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Writes code: edits files, adds features, runs `cargo test`.
    #[default]
    Implementer,
    /// Coordinates other sessions; spawns work; merges PRs.
    Orchestrator,
    /// Reads code, runs gates, posts PR reviews.
    Reviewer,
    /// Writes `.md` files, updates ADRs, regenerates `directory-tree.md`.
    Docs,
    /// CI fixes, conflict resolution, dependency bumps, infrastructure.
    DevOps,
}

/// All keyword lists are matched as case-insensitive substrings against the
/// full prompt; precedence is enforced in `derive_role`'s control flow rather
/// than in any per-list ordering.
const ORCHESTRATOR_KEYWORDS: &[&str] = &[
    "coordinate",
    "orchestrate",
    "merge ",
    "merge pr",
    "spawn ",
    "dispatch ",
    "delegate ",
    "milestone",
    "queue",
];

const REVIEWER_KEYWORDS: &[&str] = &[
    "review ",
    "audit ",
    "inspect",
    "code review",
    "security review",
    "pr review",
    "post review",
    "approve ",
    "request changes",
];

const DOCS_KEYWORDS: &[&str] = &[
    "doc ",
    "docs ",
    "documentation",
    "readme",
    "adr ",
    "directory-tree",
    "changelog",
    "rustdoc",
    ".md",
    "guide",
    "tutorial",
];

const DEVOPS_KEYWORDS: &[&str] = &[
    "ci ",
    "ci/",
    "github actions",
    "workflow",
    "actionlint",
    "shellcheck",
    "conflict",
    "rebase",
    "dependabot",
    "bump ",
    "release ",
    "deploy",
    "infra",
];

/// Classify a prompt into a `Role` using case-insensitive substring matching.
///
/// Resolution order: Orchestrator > Reviewer > DevOps > Docs > Implementer (default).
/// The order is chosen so that explicit coordination verbs ("coordinate", "merge")
/// win over ambient nouns ("docs"), and infrastructure keywords ("ci ", "workflow")
/// win over the documentation noun "readme" when both appear.
pub fn derive_role(prompt: &str) -> Role {
    let normalized = prompt.to_ascii_lowercase();
    if ORCHESTRATOR_KEYWORDS.iter().any(|k| normalized.contains(k)) {
        return Role::Orchestrator;
    }
    if REVIEWER_KEYWORDS.iter().any(|k| normalized.contains(k)) {
        return Role::Reviewer;
    }
    if DEVOPS_KEYWORDS.iter().any(|k| normalized.contains(k)) {
        return Role::DevOps;
    }
    if DOCS_KEYWORDS.iter().any(|k| normalized.contains(k)) {
        return Role::Docs;
    }
    Role::Implementer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_implementer() {
        assert_eq!(Role::default(), Role::Implementer);
    }

    #[test]
    fn empty_prompt_returns_implementer() {
        assert_eq!(derive_role(""), Role::Implementer);
    }

    #[test]
    fn coordinate_prompt_is_orchestrator() {
        assert_eq!(
            derive_role("coordinate the merge of #527 and #528"),
            Role::Orchestrator
        );
    }

    #[test]
    fn implement_prompt_is_implementer() {
        assert_eq!(
            derive_role("implement #529 — loading animations"),
            Role::Implementer
        );
    }

    #[test]
    fn review_prompt_is_reviewer() {
        assert_eq!(
            derive_role("review the diff on PR #530 and post review"),
            Role::Reviewer
        );
    }

    #[test]
    fn adr_prompt_is_docs() {
        assert_eq!(derive_role("update the README and add a guide"), Role::Docs);
    }

    #[test]
    fn ci_prompt_is_devops() {
        assert_eq!(
            derive_role("fix the CI workflow that broke after the rebase"),
            Role::DevOps
        );
    }

    #[test]
    fn case_insensitive_classification() {
        assert_eq!(
            derive_role("COORDINATE the merge"),
            Role::Orchestrator,
            "case should not affect classification"
        );
    }

    /// Sanity gate for the spike's Go signal #5: the seed corpus produces ≥3
    /// distinct `Role` values. The follow-up's full property test ratchets this
    /// to ≥80% accuracy on a 25-prompt corpus.
    #[test]
    fn seed_corpus_produces_at_least_three_distinct_roles() {
        let corpus = [
            "coordinate the merge of #527 and #528", // Orchestrator
            "implement #529 — loading animations",   // Implementer
            "review the diff on PR #530",            // Reviewer
            "update the README and add a guide",     // Docs
            "fix the CI workflow",                   // DevOps
        ];
        let mut seen = std::collections::HashSet::new();
        for prompt in corpus {
            seen.insert(derive_role(prompt));
        }
        assert!(
            seen.len() >= 3,
            "expected ≥3 distinct roles from seed corpus, got {}: {:?}",
            seen.len(),
            seen
        );
    }

    #[test]
    fn round_trips_via_serde_json() {
        let original = Role::Reviewer;
        let json = serde_json::to_string(&original).expect("serialize");
        assert_eq!(json, "\"reviewer\"", "must serialize as snake_case");
        let back: Role = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, original);
    }
}
