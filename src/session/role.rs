//! Role taxonomy for sessions.
//!
//! Lifted from `src/tui/agent_personalities/role.rs` (the ADR-002 spike).
//! See `docs/adr/002-agent-personalities.md` § Data Model for the verdict
//! ("stored field with derived fallback") and § Role Taxonomy for the keyword
//! corpus rationale.
//!
//! The canonical list is five: `Implementer` (default), `Orchestrator`,
//! `Reviewer`, `Docs`, `DevOps`. The `derive_role` classifier mirrors
//! `crate::session::intent`'s keyword-matching idiom (case-insensitive
//! substring scan).

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Five-role taxonomy for agent sessions.
///
/// `Implementer` is the default because it is the largest category in maestro's
/// session log; "unknown prompt → Implementer" is the safest miscategorization
/// (most sessions do work, so misfires are visually invisible).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, ValueEnum)]
#[serde(rename_all = "snake_case")]
#[clap(rename_all = "snake_case")]
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
    let role = if ORCHESTRATOR_KEYWORDS.iter().any(|k| normalized.contains(k)) {
        Role::Orchestrator
    } else if REVIEWER_KEYWORDS.iter().any(|k| normalized.contains(k)) {
        Role::Reviewer
    } else if DEVOPS_KEYWORDS.iter().any(|k| normalized.contains(k)) {
        Role::DevOps
    } else if DOCS_KEYWORDS.iter().any(|k| normalized.contains(k)) {
        Role::Docs
    } else {
        Role::Implementer
    };
    tracing::debug!(prompt = %prompt, role = ?role, "derived role");
    role
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Role enum & serde ---

    #[test]
    fn default_is_implementer() {
        assert_eq!(Role::default(), Role::Implementer);
    }

    #[test]
    fn role_serializes_as_snake_case_implementer() {
        let json = serde_json::to_string(&Role::Implementer).unwrap();
        assert_eq!(json, r#""implementer""#);
    }

    #[test]
    fn role_serializes_as_snake_case_orchestrator() {
        let json = serde_json::to_string(&Role::Orchestrator).unwrap();
        assert_eq!(json, r#""orchestrator""#);
    }

    #[test]
    fn role_serializes_as_snake_case_reviewer() {
        let json = serde_json::to_string(&Role::Reviewer).unwrap();
        assert_eq!(json, r#""reviewer""#);
    }

    #[test]
    fn role_serializes_as_snake_case_docs() {
        let json = serde_json::to_string(&Role::Docs).unwrap();
        assert_eq!(json, r#""docs""#);
    }

    #[test]
    fn role_serializes_as_snake_case_dev_ops() {
        let json = serde_json::to_string(&Role::DevOps).unwrap();
        assert_eq!(
            json, r#""dev_ops""#,
            "DevOps must serialize as 'dev_ops' (snake_case), not 'devops'"
        );
    }

    #[test]
    fn role_deserializes_from_implementer() {
        let v: Role = serde_json::from_str(r#""implementer""#).unwrap();
        assert_eq!(v, Role::Implementer);
    }

    #[test]
    fn role_deserializes_from_orchestrator() {
        let v: Role = serde_json::from_str(r#""orchestrator""#).unwrap();
        assert_eq!(v, Role::Orchestrator);
    }

    #[test]
    fn role_deserializes_from_reviewer() {
        let v: Role = serde_json::from_str(r#""reviewer""#).unwrap();
        assert_eq!(v, Role::Reviewer);
    }

    #[test]
    fn role_deserializes_from_docs() {
        let v: Role = serde_json::from_str(r#""docs""#).unwrap();
        assert_eq!(v, Role::Docs);
    }

    #[test]
    fn role_deserializes_from_dev_ops() {
        let v: Role = serde_json::from_str(r#""dev_ops""#).unwrap();
        assert_eq!(v, Role::DevOps);
    }

    // --- derive_role basic ---

    #[test]
    fn derive_role_empty_prompt_is_implementer() {
        assert_eq!(derive_role(""), Role::Implementer);
    }

    // --- Resolution-order tests (highest-value) ---
    //
    // These validate the priority spec: Orchestrator > Reviewer > DevOps > Docs > Implementer.

    #[test]
    fn derive_role_orchestrator_beats_reviewer() {
        // "coordinate" (Orchestrator) + "review " (Reviewer) → Orchestrator wins.
        assert_eq!(
            derive_role("coordinate the review of PR #530 before merging"),
            Role::Orchestrator
        );
    }

    #[test]
    fn derive_role_orchestrator_beats_devops() {
        // "orchestrate" (Orchestrator) + "deploy" (DevOps) → Orchestrator wins.
        assert_eq!(
            derive_role("orchestrate the CI workflow fix and deploy to staging"),
            Role::Orchestrator
        );
    }

    #[test]
    fn derive_role_orchestrator_beats_docs() {
        // "coordinate" (Orchestrator) + "readme" (Docs) → Orchestrator wins.
        assert_eq!(
            derive_role("coordinate the documentation sprint and update the readme"),
            Role::Orchestrator
        );
    }

    #[test]
    fn derive_role_reviewer_beats_devops() {
        // "audit " (Reviewer) + "workflow" (DevOps) → Reviewer wins.
        assert_eq!(
            derive_role("audit the CI workflow configuration for security issues"),
            Role::Reviewer
        );
    }

    #[test]
    fn derive_role_reviewer_beats_docs() {
        // "inspect" (Reviewer) + "guide" (Docs) → Reviewer wins.
        assert_eq!(
            derive_role("inspect the documentation guide for accuracy"),
            Role::Reviewer
        );
    }

    #[test]
    fn derive_role_devops_beats_docs() {
        // "bump " (DevOps) + "changelog" (Docs) → DevOps wins.
        assert_eq!(
            derive_role("bump the dependabot deps and update the changelog"),
            Role::DevOps
        );
    }

    // --- Case insensitivity ---

    #[test]
    fn derive_role_case_insensitive() {
        assert_eq!(derive_role("COORDINATE the sessions"), Role::Orchestrator);
        assert_eq!(derive_role("AUDIT the codebase"), Role::Reviewer);
        assert_eq!(derive_role("FIX THE CI WORKFLOW"), Role::DevOps);
        assert_eq!(derive_role("UPDATE THE README"), Role::Docs);
    }

    // --- Accuracy property test (≥80% on a 25-prompt corpus, 5 per role) ---

    #[test]
    fn derive_role_accuracy_on_25_prompt_corpus_is_above_80_percent() {
        let orchestrator: &[(&str, Role)] = &[
            // "coordinate" + "dispatch " keywords
            (
                "coordinate the sprint and dispatch tasks to available sessions",
                Role::Orchestrator,
            ),
            // "orchestrate" + "merge " keywords
            (
                "orchestrate the release by merging open PRs in milestone order",
                Role::Orchestrator,
            ),
            // "milestone" + "queue" keywords
            (
                "advance the milestone by queuing all unblocked issues",
                Role::Orchestrator,
            ),
            // "delegate " keyword
            (
                "delegate issue #538 to the implementer session pool",
                Role::Orchestrator,
            ),
            // "spawn " keyword
            (
                "spawn a reviewer session for PR #540 and wait for approval",
                Role::Orchestrator,
            ),
        ];
        let reviewer: &[(&str, Role)] = &[
            // "code review" keyword
            (
                "perform a code review on the new session/role module before merge",
                Role::Reviewer,
            ),
            // "pr review" keyword
            (
                "run a pr review for #538 and post review comments on the diff",
                Role::Reviewer,
            ),
            // "security review" + "audit " keywords
            (
                "security review the auth flow in manager.rs and audit the token handling",
                Role::Reviewer,
            ),
            // "inspect" + "request changes" keywords
            (
                "inspect the serde round-trip tests and request changes if any are missing",
                Role::Reviewer,
            ),
            // "approve " keyword
            (
                "approve the PR once all CI checks pass and the review is signed off",
                Role::Reviewer,
            ),
        ];
        let devops: &[(&str, Role)] = &[
            // "github actions" + "workflow" + "rebase" keywords
            (
                "fix the github actions workflow that broke on the rebase of main",
                Role::DevOps,
            ),
            // "ci " + "rebase" keywords
            (
                "resolve ci failures after rebasing feat/role onto main",
                Role::DevOps,
            ),
            // "bump " + "deploy" keywords
            (
                "bump the tokio version in Cargo.toml and deploy to staging",
                Role::DevOps,
            ),
            // "dependabot" keyword (avoid "review" which would beat DevOps)
            (
                "triage the dependabot update for serde 1.0.197 and merge if CI passes",
                Role::DevOps,
            ),
            // "conflict" + "release " keywords
            (
                "fix the merge conflict in src/session/types.rs blocking the release",
                Role::DevOps,
            ),
        ];
        let docs: &[(&str, Role)] = &[
            // "readme" keyword
            (
                "update the README with the new --role flag documentation",
                Role::Docs,
            ),
            // "adr " keyword
            (
                "write an adr for the role taxonomy decision in issue #538",
                Role::Docs,
            ),
            // "changelog" keyword
            (
                "add the role classifier feature to the changelog under Unreleased",
                Role::Docs,
            ),
            // "rustdoc" + "documentation" keywords
            (
                "regenerate the rustdoc documentation for the session module",
                Role::Docs,
            ),
            // ".md" keyword
            (
                "create CONTRIBUTING.md explaining the TDD workflow for contributors",
                Role::Docs,
            ),
        ];
        let implementer: &[(&str, Role)] = &[
            (
                "implement issue #538: add Role enum and derive_role classifier",
                Role::Implementer,
            ),
            (
                "fix the serde deserialization bug in session/types.rs line 282",
                Role::Implementer,
            ),
            (
                "add unit tests for the fork depth limit in session/fork.rs",
                Role::Implementer,
            ),
            (
                "refactor the SessionStatus transition matrix to remove dead states",
                Role::Implementer,
            ),
            (
                "wire up the new flag in src/main.rs and pass it to Session constructors",
                Role::Implementer,
            ),
        ];

        let mut correct = 0usize;
        let mut total = 0usize;
        for (prompt, expected) in orchestrator
            .iter()
            .chain(reviewer)
            .chain(devops)
            .chain(docs)
            .chain(implementer)
        {
            total += 1;
            if derive_role(prompt) == *expected {
                correct += 1;
            }
        }
        let accuracy = correct as f64 / total as f64;
        assert!(
            accuracy > 0.80,
            "accuracy {:.1}% on {} prompts (correct: {})",
            accuracy * 100.0,
            total,
            correct
        );
    }
}
