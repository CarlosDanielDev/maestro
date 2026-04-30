use crate::provider::github::types::{GhIssue, PendingPr};
use crate::session::types::Session;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// A session-end event awaiting auto-PR processing. Persisted in
/// `MaestroState::pending_completions` so a maestro shutdown between
/// session-completion and the next `check_completions` tick does not
/// orphan the worktree (#514).
///
/// Lives in the state layer (not the TUI layer) so the architecture
/// rule that forbids `state -> tui` imports is respected. The TUI layer
/// re-exports this type via `crate::tui::app::types::PendingIssueCompletion`
/// for backward compatibility with existing call sites.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingIssueCompletion {
    pub issue_number: u64,
    /// Additional issue numbers for unified PR sessions.
    pub issue_numbers: Vec<u64>,
    pub success: bool,
    pub cost_usd: f64,
    pub files_touched: Vec<String>,
    pub worktree_branch: Option<String>,
    pub worktree_path: Option<PathBuf>,
    pub is_ci_fix: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MaestroState {
    pub sessions: Vec<Session>,
    pub total_cost_usd: f64,
    pub file_claims: HashMap<String, uuid::Uuid>,
    pub last_updated: Option<DateTime<Utc>>,
    /// Cached GitHub issue data to avoid repeated API calls.
    #[serde(default)]
    pub issue_cache: HashMap<u64, GhIssue>,
    /// Fork lineage: maps child session ID to parent session ID.
    #[serde(default)]
    pub fork_lineage: HashMap<uuid::Uuid, uuid::Uuid>,
    /// PRs that failed creation and are queued for retry or manual action.
    #[serde(default)]
    pub pending_prs: Vec<PendingPr>,
    /// Session-end events awaiting auto-PR processing. Persisted across
    /// restarts so a maestro shutdown between session-completion and the
    /// next `check_completions` tick does not orphan the worktree (#514).
    /// Backward compatible — older state files default to an empty vec.
    #[serde(default)]
    pub pending_completions: Vec<PendingIssueCompletion>,
}

impl MaestroState {
    pub fn active_sessions(&self) -> Vec<&Session> {
        self.sessions
            .iter()
            .filter(|s| !s.status.is_terminal())
            .collect()
    }

    pub fn update_total_cost(&mut self) {
        self.total_cost_usd = self.sessions.iter().map(|s| s.cost_usd).sum();
    }

    /// Record a fork relationship.
    pub fn record_fork(&mut self, parent_id: uuid::Uuid, child_id: uuid::Uuid) {
        self.fork_lineage.insert(child_id, parent_id);
    }

    /// Get the fork chain for a session (from root to leaf).
    #[allow(dead_code)] // Reason: fork chain traversal — to be used in session view
    pub fn fork_chain(&self, session_id: uuid::Uuid) -> Vec<uuid::Uuid> {
        let mut chain = vec![session_id];
        let mut current = session_id;
        let mut visited = std::collections::HashSet::new();
        visited.insert(current);
        while let Some(&parent) = self.fork_lineage.get(&current) {
            if !visited.insert(parent) {
                break; // cycle guard
            }
            chain.push(parent);
            current = parent;
        }
        chain.reverse();
        chain
    }

    /// Get the fork depth for a session.
    #[allow(dead_code)] // Reason: fork depth for session view display
    pub fn fork_depth(&self, session_id: uuid::Uuid) -> usize {
        self.fork_chain(session_id).len() - 1
    }

    /// Apply TurboQuant-driven compaction to every session's activity log.
    /// When `adapter` is None or disabled, returns an empty report list and
    /// does not mutate sessions.
    pub fn compact(
        &mut self,
        adapter: Option<&crate::turboquant::adapter::TurboQuantAdapter>,
    ) -> Vec<crate::turboquant::adapter::StateCompactionReport> {
        let Some(tq) = adapter else {
            return Vec::new();
        };
        self.sessions
            .iter_mut()
            .map(|s| tq.compact_session_history(s))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn record_fork_inserts_lineage_entry() {
        let mut state = MaestroState::default();
        let parent_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();
        state.record_fork(parent_id, child_id);
        assert_eq!(state.fork_lineage.get(&child_id), Some(&parent_id));
    }

    #[test]
    fn fork_chain_returns_full_ancestry() {
        let mut state = MaestroState::default();
        let root_id = Uuid::new_v4();
        let mid_id = Uuid::new_v4();
        let leaf_id = Uuid::new_v4();
        state.record_fork(root_id, mid_id);
        state.record_fork(mid_id, leaf_id);
        let chain = state.fork_chain(leaf_id);
        assert_eq!(chain, vec![root_id, mid_id, leaf_id]);
    }

    #[test]
    fn fork_chain_single_session_returns_just_itself() {
        let state = MaestroState::default();
        let id = Uuid::new_v4();
        assert_eq!(state.fork_chain(id), vec![id]);
    }

    #[test]
    fn fork_depth_returns_zero_for_root() {
        let state = MaestroState::default();
        assert_eq!(state.fork_depth(Uuid::new_v4()), 0);
    }

    #[test]
    fn fork_depth_returns_correct_depth_for_leaf() {
        let mut state = MaestroState::default();
        let root_id = Uuid::new_v4();
        let mid_id = Uuid::new_v4();
        let leaf_id = Uuid::new_v4();
        state.record_fork(root_id, mid_id);
        state.record_fork(mid_id, leaf_id);
        assert_eq!(state.fork_depth(leaf_id), 2);
    }

    #[test]
    fn fork_lineage_serializes_and_deserializes() {
        let mut state = MaestroState::default();
        let parent_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();
        state.record_fork(parent_id, child_id);
        let json = serde_json::to_string(&state).unwrap();
        let restored: MaestroState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.fork_lineage.get(&child_id), Some(&parent_id));
    }

    #[test]
    fn update_total_cost_unaffected_by_fork_lineage() {
        let mut state = MaestroState::default();
        let mut s1 = crate::session::types::Session::new(
            "a".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        s1.cost_usd = 1.0;
        let mut s2 = crate::session::types::Session::new(
            "b".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        s2.cost_usd = 1.0;
        state.sessions.push(s1);
        state.sessions.push(s2);
        state.record_fork(Uuid::new_v4(), Uuid::new_v4());
        state.update_total_cost();
        assert!((state.total_cost_usd - 2.0).abs() < f64::EPSILON);
    }

    // --- Issue #159: MaestroState::pending_prs persistence ---

    #[test]
    fn maestro_state_pending_prs_defaults_to_empty_vec() {
        let state = MaestroState::default();
        assert!(state.pending_prs.is_empty());
    }

    #[test]
    fn maestro_state_pending_prs_round_trips_via_serde() {
        use crate::provider::github::types::{PendingPr, PendingPrStatus};

        let mut state = MaestroState::default();
        state.pending_prs.push(PendingPr {
            issue_number: 7,
            issue_numbers: vec![],
            branch: "maestro/issue-7".into(),
            base_branch: "main".into(),
            files_touched: vec!["src/lib.rs".into()],
            cost_usd: 0.5,
            attempt: 0,
            max_attempts: 3,
            last_error: String::new(),
            last_attempt_at: chrono::Utc::now(),
            next_retry_at: None,
            status: PendingPrStatus::RetryScheduled,
            last_errors: std::collections::VecDeque::new(),
            manual_retry_count: 0,
        });

        let json = serde_json::to_string(&state).unwrap();
        let rt: MaestroState = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.pending_prs.len(), 1);
        assert_eq!(rt.pending_prs[0].issue_number, 7);
        assert_eq!(rt.pending_prs[0].branch, "maestro/issue-7");
    }

    #[test]
    fn maestro_state_pending_prs_deserializes_with_default_when_absent() {
        let state = MaestroState::default();
        let json = serde_json::to_string(&state).unwrap();
        let stripped = json.replace(r#","pending_prs":[]"#, "");
        let rt: MaestroState = serde_json::from_str(&stripped).unwrap();
        assert!(
            rt.pending_prs.is_empty(),
            "must default to empty vec for backward compatibility"
        );
    }

    // --- Issue #514: MaestroState::pending_completions persistence ---
    // pending_issue_completions are in-memory in App today; if maestro
    // shuts down between session-end and the next check_completions tick,
    // the auto-PR work is lost (orphan worktree, no PR). Persisting them
    // closes that gap.

    #[test]
    fn maestro_state_pending_completions_defaults_to_empty_vec() {
        let state = MaestroState::default();
        assert!(state.pending_completions.is_empty());
    }

    #[test]
    fn maestro_state_pending_completions_round_trips_via_serde() {
        let mut state = MaestroState::default();
        state.pending_completions.push(PendingIssueCompletion {
            issue_number: 42,
            issue_numbers: vec![42, 99],
            success: true,
            cost_usd: 1.5,
            files_touched: vec!["src/foo.rs".into()],
            worktree_branch: Some("maestro/unified-42-99".into()),
            worktree_path: Some(std::path::PathBuf::from("/tmp/wt")),
            is_ci_fix: false,
        });

        let json = serde_json::to_string(&state).unwrap();
        let rt: MaestroState = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.pending_completions.len(), 1);
        assert_eq!(rt.pending_completions[0].issue_number, 42);
        assert_eq!(rt.pending_completions[0].issue_numbers, vec![42, 99]);
        assert_eq!(
            rt.pending_completions[0].worktree_branch.as_deref(),
            Some("maestro/unified-42-99")
        );
    }

    #[test]
    fn maestro_state_pending_completions_deserializes_with_default_when_absent() {
        // Legacy state JSON without the new field should still load.
        let json = r#"{"sessions":[],"total_cost_usd":0.0,"file_claims":{},"last_updated":null}"#;
        let state: MaestroState = serde_json::from_str(json).unwrap();
        assert!(
            state.pending_completions.is_empty(),
            "must default to empty vec for backward compatibility"
        );
    }

    #[test]
    fn fork_chain_terminates_on_cycle() {
        let mut state = MaestroState::default();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        // Create a cycle: a -> b -> a
        state.fork_lineage.insert(b, a);
        state.fork_lineage.insert(a, b);
        let chain = state.fork_chain(a);
        // Should not infinite loop — chain should be finite
        assert!(chain.len() <= 3);
    }

    // --- Issue #345: compact() on MaestroState ---

    use crate::session::types::{ActivityEntry, SessionStatus};
    use crate::turboquant::adapter::TurboQuantAdapter;
    use chrono::Utc;

    fn adapter() -> TurboQuantAdapter {
        TurboQuantAdapter::new(4)
    }

    #[test]
    fn compact_returns_empty_when_adapter_is_none() {
        let mut state = MaestroState::default();
        let mut s = crate::session::types::Session::new(
            "p".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        for _ in 0..5 {
            s.activity_log.push(ActivityEntry {
                timestamp: Utc::now(),
                message: "Tool: Bash".into(),
            });
        }
        state.sessions.push(s);
        let reports = state.compact(None);
        assert!(reports.is_empty());
        assert_eq!(state.sessions[0].activity_log.len(), 5);
    }

    #[test]
    fn compact_runs_per_session_when_adapter_enabled() {
        let mut state = MaestroState::default();
        let mut s1 = crate::session::types::Session::new(
            "a".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        s1.status = SessionStatus::Running;
        for _ in 0..8 {
            s1.activity_log.push(ActivityEntry {
                timestamp: Utc::now(),
                message: "Tool: Bash".into(),
            });
        }
        let mut s2 = crate::session::types::Session::new(
            "b".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        s2.status = SessionStatus::Running;
        state.sessions.push(s1);
        state.sessions.push(s2);

        let a = adapter();
        let reports = state.compact(Some(&a));
        assert_eq!(reports.len(), 2);
        assert_eq!(state.sessions[0].activity_log.len(), 1);
        assert_eq!(state.sessions[1].activity_log.len(), 0);
    }

    #[test]
    fn compact_then_serde_round_trip_preserves_compacted_log() {
        let mut state = MaestroState::default();
        let mut s = crate::session::types::Session::new(
            "a".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        s.status = SessionStatus::Running;
        for _ in 0..10 {
            s.activity_log.push(ActivityEntry {
                timestamp: Utc::now(),
                message: "Tool: Bash".into(),
            });
        }
        state.sessions.push(s);

        let a = adapter();
        state.compact(Some(&a));
        let json = serde_json::to_string(&state).unwrap();
        let rt: MaestroState = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.sessions[0].activity_log.len(), 1);
        assert!(rt.sessions[0].activity_log[0].message.contains("x10"));
    }
}
