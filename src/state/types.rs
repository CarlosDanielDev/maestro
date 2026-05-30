use crate::provider::github::types::PendingPr;
use crate::provider::types::Issue;
use crate::session::interaction::InteractionSession;
use crate::session::types::Session;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

pub type IssueNumber = u64;

/// Current on-disk schema version for `MaestroState`. Bumped whenever a
/// breaking format change ships. Files written by older maestro versions
/// have no `version` key and deserialize to `0`; `StateStore::load` calls
/// `migrate()` to bring them up to `CURRENT_STATE_VERSION`.
pub const CURRENT_STATE_VERSION: u32 = 1;

/// Default for the serde `version` field — 0 means "legacy file written
/// before the version stamp was introduced." Migration handles the bump.
pub fn default_state_version() -> u32 {
    0
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
        output: crate::orchestration::types::TeamOutput,
    },
    Failed {
        reason: String,
        attempts: u8,
    },
    Blocked {
        blocking: Vec<IssueNumber>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamRun {
    pub id: Uuid,
    pub team_name: String,
    pub started_at: DateTime<Utc>,
    pub plan: Vec<Vec<IssueNumber>>, // topo-sorted levels
    pub state: HashMap<IssueNumber, IssueRunState>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaestroState {
    /// On-disk schema version. Defaults to `0` for files written before
    /// the version stamp was introduced; `StateStore::load` migrates
    /// such files to `CURRENT_STATE_VERSION`.
    #[serde(default = "default_state_version")]
    pub version: u32,
    pub sessions: Vec<Session>,
    pub total_cost_usd: f64,
    pub file_claims: HashMap<String, uuid::Uuid>,
    pub last_updated: Option<DateTime<Utc>>,
    /// Cached GitHub issue data to avoid repeated API calls.
    #[serde(default)]
    pub issue_cache: HashMap<u64, Issue>,
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
    /// Team orchestrator runs. Defaults to empty for backward compatibility.
    #[serde(default)]
    pub team_runs: Vec<TeamRun>,
    /// Interactive (chat-style) sessions, keyed by issue number (#734).
    /// Defaults to empty for backward compatibility — old state files
    /// without this key load with no interactions. Population lands in #737.
    #[serde(default)]
    pub interactions: Vec<InteractionSession>,
}

impl Default for MaestroState {
    fn default() -> Self {
        Self {
            version: CURRENT_STATE_VERSION,
            sessions: Vec::new(),
            total_cost_usd: 0.0,
            file_claims: HashMap::new(),
            last_updated: None,
            issue_cache: HashMap::new(),
            fork_lineage: HashMap::new(),
            pending_prs: Vec::new(),
            pending_completions: Vec::new(),
            team_runs: Vec::new(),
            interactions: Vec::new(),
        }
    }
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

    /// Trim persisted session history to `cap` most-recent terminal
    /// entries, keeping all non-terminal sessions intact (active /
    /// queued / paused work is never evicted by the history cap).
    /// `cap == 0` clears history entirely and zeros `total_cost_usd`.
    /// Recomputes `total_cost_usd` from the surviving sessions so the
    /// on-disk aggregate stays in sync with the array. Added 2026-05-23.
    pub fn cap_session_history(&mut self, cap: usize) {
        let (terminal, active): (Vec<Session>, Vec<Session>) = self
            .sessions
            .drain(..)
            .partition(|s| s.status.is_terminal());

        let mut terminal = terminal;
        if cap == 0 {
            terminal.clear();
        } else if terminal.len() > cap {
            // Sort newest first by `finished_at` (fall back to
            // `started_at` for sessions that finished without timing
            // — defensive; should not normally happen).
            terminal.sort_by(|a, b| {
                let ka = a.finished_at.or(a.started_at);
                let kb = b.finished_at.or(b.started_at);
                kb.cmp(&ka)
            });
            terminal.truncate(cap);
        }

        self.sessions = active;
        self.sessions.extend(terminal);
        self.update_total_cost();
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

    fn make_terminal_session(cost: f64, finished_offset_secs: i64) -> Session {
        let mut s = Session::new(
            "test".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        s.status = crate::session::types::SessionStatus::Completed;
        s.cost_usd = cost;
        s.started_at = Some(chrono::Utc::now() - chrono::Duration::seconds(60));
        s.finished_at =
            Some(chrono::Utc::now() - chrono::Duration::seconds(60 - finished_offset_secs));
        s
    }

    fn make_active_session(cost: f64) -> Session {
        let mut s = Session::new(
            "active".into(),
            "opus".into(),
            "orchestrator".into(),
            None,
            None,
        );
        s.status = crate::session::types::SessionStatus::Running;
        s.cost_usd = cost;
        s.started_at = Some(chrono::Utc::now());
        s
    }

    #[test]
    fn cap_session_history_zero_clears_all_terminal_sessions() {
        let mut state = MaestroState::default();
        state.sessions.push(make_terminal_session(0.10, 1));
        state.sessions.push(make_terminal_session(0.20, 2));
        state.sessions.push(make_active_session(0.05));

        state.cap_session_history(0);

        assert_eq!(state.sessions.len(), 1, "active session must survive cap=0");
        assert!(matches!(
            state.sessions[0].status,
            crate::session::types::SessionStatus::Running
        ));
        assert!(
            (state.total_cost_usd - 0.05).abs() < f64::EPSILON,
            "total_cost_usd recomputed from surviving sessions only; got {}",
            state.total_cost_usd
        );
    }

    #[test]
    fn cap_session_history_keeps_n_most_recent_terminals() {
        let mut state = MaestroState::default();
        // Three terminal sessions, increasing finished_at.
        state.sessions.push(make_terminal_session(0.10, 10));
        state.sessions.push(make_terminal_session(0.20, 20));
        state.sessions.push(make_terminal_session(0.30, 30));

        state.cap_session_history(2);

        assert_eq!(state.sessions.len(), 2, "cap=2 must keep 2 terminals");
        // Newest two are cost 0.30 + 0.20.
        let total: f64 = state.sessions.iter().map(|s| s.cost_usd).sum();
        assert!(
            (total - 0.50).abs() < f64::EPSILON,
            "newest two by finished_at sum to 0.50; got {}",
            total
        );
        assert!((state.total_cost_usd - 0.50).abs() < f64::EPSILON);
    }

    #[test]
    fn cap_session_history_never_evicts_active_sessions() {
        let mut state = MaestroState::default();
        for i in 0..5 {
            state.sessions.push(make_terminal_session(0.10, i as i64));
        }
        state.sessions.push(make_active_session(0.99));

        state.cap_session_history(2);

        let active_count = state
            .sessions
            .iter()
            .filter(|s| !s.status.is_terminal())
            .count();
        assert_eq!(active_count, 1, "active session must survive any cap");
        // 2 terminals + 1 active = 3 total
        assert_eq!(state.sessions.len(), 3);
    }

    #[test]
    fn cap_session_history_no_op_when_count_under_cap() {
        let mut state = MaestroState::default();
        state.sessions.push(make_terminal_session(0.10, 1));
        state.sessions.push(make_terminal_session(0.20, 2));

        state.cap_session_history(10);

        assert_eq!(state.sessions.len(), 2, "below cap = no truncation");
        assert!((state.total_cost_usd - 0.30).abs() < f64::EPSILON);
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

    #[test]
    fn team_run_serde_round_trip() {
        use crate::orchestration::types::TeamOutput;

        let mut state = HashMap::new();
        state.insert(
            547,
            IssueRunState::Succeeded {
                output: TeamOutput::Pr {
                    number: 714,
                    branch: "feat/547".into(),
                },
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
        let back: TeamRun = serde_json::from_str(&json).unwrap();
        assert_eq!(back.team_name, "default-coder");
        assert_eq!(back.plan.len(), 1);
        assert!(back.state.contains_key(&547));
    }

    #[test]
    fn maestro_state_team_runs_defaults_to_empty_vec() {
        let state = MaestroState::default();
        assert!(state.team_runs.is_empty());
    }

    #[test]
    fn maestro_state_team_runs_deserializes_when_absent() {
        let json = r#"{"sessions":[],"total_cost_usd":0.0,"file_claims":{},"last_updated":null}"#;
        let state: MaestroState = serde_json::from_str(json).unwrap();
        assert!(state.team_runs.is_empty());
    }

    // --- Issue #159: MaestroState::pending_prs persistence ---

    #[test]
    fn maestro_state_pending_prs_defaults_to_empty_vec() {
        let state = MaestroState::default();
        assert!(state.pending_prs.is_empty());
    }

    #[test]
    fn maestro_state_pending_prs_round_trips_via_serde() {
        use crate::provider::github::types::{PendingPrStatus, awaiting_pending_pr};

        let mut state = MaestroState::default();
        let mut p = awaiting_pending_pr(7);
        p.files_touched = vec!["src/lib.rs".into()];
        p.cost_usd = 0.5;
        p.attempt = 0;
        p.status = PendingPrStatus::RetryScheduled;
        state.pending_prs.push(p);

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

    // --- Issue #734: MaestroState::interactions persistence ---

    #[test]
    fn maestro_state_interactions_defaults_to_empty_vec() {
        let state = MaestroState::default();
        assert!(state.interactions.is_empty());
    }

    #[test]
    fn maestro_state_interactions_round_trips_via_serde() {
        use crate::session::interaction::InteractionSession;

        let mut state = MaestroState::default();
        state.interactions.push(InteractionSession::new(
            99,
            std::path::PathBuf::from("/tmp/x"),
            "branch".into(),
            true,
        ));

        let json = serde_json::to_string(&state).unwrap();
        let rt: MaestroState = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.interactions.len(), 1);
        assert_eq!(rt.interactions[0].issue_number, 99);
    }

    #[test]
    fn maestro_state_interactions_deserializes_with_default_when_absent() {
        // Legacy state JSON without the new field should still load.
        let json = r#"{"sessions":[],"total_cost_usd":0.0,"file_claims":{},"last_updated":null}"#;
        let state: MaestroState = serde_json::from_str(json).unwrap();
        assert!(
            state.interactions.is_empty(),
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
