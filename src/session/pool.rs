use std::collections::VecDeque;
use uuid::Uuid;
use tokio::sync::mpsc;

use super::manager::{ManagedSession, SessionEvent};
use super::types::{Session, SessionStatus};
use super::worktree::WorktreeManager;
use crate::state::file_claims::FileClaimManager;

pub struct SessionPool {
    max_concurrent: usize,
    queue: VecDeque<Session>,
    active: Vec<ManagedSession>,
    finished: Vec<ManagedSession>,
    worktree_mgr: Box<dyn WorktreeManager + Send>,
    pub file_claims: FileClaimManager,
    event_tx: mpsc::UnboundedSender<SessionEvent>,
    /// Permission mode passed to Claude CLI sessions.
    permission_mode: String,
    /// Allowed tools whitelist passed to Claude CLI sessions.
    allowed_tools: Vec<String>,
}

impl SessionPool {
    pub fn new(
        max_concurrent: usize,
        worktree_mgr: Box<dyn WorktreeManager + Send>,
        event_tx: mpsc::UnboundedSender<SessionEvent>,
    ) -> Self {
        Self {
            max_concurrent,
            queue: VecDeque::new(),
            active: Vec::new(),
            finished: Vec::new(),
            worktree_mgr,
            file_claims: FileClaimManager::new(),
            event_tx,
            permission_mode: "bypassPermissions".to_string(),
            allowed_tools: Vec::new(),
        }
    }

    /// Set the permission mode for new sessions.
    pub fn set_permission_mode(&mut self, mode: String) {
        self.permission_mode = mode;
    }

    /// Set the allowed tools whitelist for new sessions.
    pub fn set_allowed_tools(&mut self, tools: Vec<String>) {
        self.allowed_tools = tools;
    }

    /// Enqueue a session. It will be promoted when capacity allows.
    pub fn enqueue(&mut self, session: Session) {
        self.queue.push_back(session);
    }

    /// Try to promote queued sessions into active slots.
    /// Creates worktrees and prepares ManagedSessions.
    /// Returns the IDs of sessions that were promoted and need spawning.
    pub fn try_promote(&mut self) -> Vec<Uuid> {
        let mut promoted = Vec::new();

        while self.active.len() < self.max_concurrent {
            let Some(mut session) = self.queue.pop_front() else {
                break;
            };

            let slug = session_slug(&session);

            // Try to create worktree (non-fatal — runs in cwd if it fails)
            let worktree_path = match self.worktree_mgr.create(&slug) {
                Ok(path) => {
                    session.log_activity(format!("Worktree created: {}", path.display()));
                    Some(path)
                }
                Err(e) => {
                    let msg = format!("Worktree skipped (running in cwd): {}", e);
                    tracing::warn!("{}", msg);
                    session.log_activity(msg);
                    None
                }
            };

            // Build system prompt with file claims for other sessions
            let system_prompt = self.file_claims.build_system_prompt(session.id);

            session.status = SessionStatus::Queued; // Still queued until spawned
            let mut managed = ManagedSession::with_worktree(session, worktree_path, system_prompt);
            managed.permission_mode = Some(self.permission_mode.clone());
            managed.allowed_tools = self.allowed_tools.clone();
            let id = managed.session.id;
            self.active.push(managed);
            promoted.push(id);
        }

        promoted
    }

    /// Handle a session reaching terminal state: move to finished, cleanup worktree.
    pub fn on_session_completed(&mut self, session_id: Uuid) {
        if let Some(idx) = self.active.iter().position(|m| m.session.id == session_id) {
            let managed = self.active.remove(idx);
            let slug = session_slug(&managed.session);

            // Release all file claims for this session
            self.file_claims.release_all(session_id);

            // Cleanup worktree
            let _ = self.worktree_mgr.remove(&slug);

            self.finished.push(managed);
        }
    }

    /// Get all sessions for display: active first, then finished, then queued.
    pub fn all_sessions(&self) -> Vec<&Session> {
        let mut out: Vec<&Session> = Vec::new();
        for m in &self.active {
            out.push(&m.session);
        }
        for m in &self.finished {
            out.push(&m.session);
        }
        for s in &self.queue {
            out.push(s);
        }
        out
    }

    /// Mutable access to a managed session by ID (active only).
    pub fn get_active_mut(&mut self, session_id: Uuid) -> Option<&mut ManagedSession> {
        self.active.iter_mut().find(|m| m.session.id == session_id)
    }

    /// Mutable access to a session by ID across all buckets.
    pub fn get_session_mut(&mut self, session_id: Uuid) -> Option<&mut Session> {
        if let Some(m) = self.active.iter_mut().find(|m| m.session.id == session_id) {
            return Some(&mut m.session);
        }
        if let Some(m) = self.finished.iter_mut().find(|m| m.session.id == session_id) {
            return Some(&mut m.session);
        }
        if let Some(s) = self.queue.iter_mut().find(|s| s.id == session_id) {
            return Some(s);
        }
        None
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    pub fn queued_count(&self) -> usize {
        self.queue.len()
    }

    pub fn total_count(&self) -> usize {
        self.active.len() + self.finished.len() + self.queue.len()
    }

    pub fn all_done(&self) -> bool {
        self.total_count() > 0 && self.active.is_empty() && self.queue.is_empty()
    }

    pub async fn kill_all(&mut self) {
        for managed in &mut self.active {
            if !managed.session.status.is_terminal() {
                let _ = managed.kill().await;
            }
        }
        // Move all active to finished
        self.finished.append(&mut self.active);
        // Clear queue
        self.queue.clear();
    }

    #[cfg(unix)]
    pub fn pause_all(&self) {
        for managed in &self.active {
            if managed.session.status == SessionStatus::Running {
                let _ = managed.pause();
            }
        }
    }

    #[cfg(unix)]
    pub fn resume_all(&self) {
        for managed in &self.active {
            if managed.session.status == SessionStatus::Paused {
                let _ = managed.resume();
            }
        }
    }

    /// Get the event sender for spawning sessions.
    pub fn event_tx(&self) -> mpsc::UnboundedSender<SessionEvent> {
        self.event_tx.clone()
    }
}

fn session_slug(session: &Session) -> String {
    match session.issue_number {
        Some(n) => format!("issue-{}", n),
        None => format!("session-{}", &session.id.to_string()[..8]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::worktree::MockWorktreeManager;

    fn make_pool(max: usize) -> SessionPool {
        let (tx, _rx) = mpsc::unbounded_channel();
        SessionPool::new(max, Box::new(MockWorktreeManager::new()), tx)
    }

    fn make_session(prompt: &str) -> Session {
        Session::new(
            prompt.to_string(),
            "opus".to_string(),
            "orchestrator".to_string(),
            None,
        )
    }

    fn make_session_with_issue(prompt: &str, issue: u64) -> Session {
        Session::new(
            prompt.to_string(),
            "opus".to_string(),
            "orchestrator".to_string(),
            Some(issue),
        )
    }

    #[test]
    fn enqueue_adds_to_queue() {
        let mut pool = make_pool(2);
        pool.enqueue(make_session("fix bug"));
        assert_eq!(pool.queued_count(), 1);
        assert_eq!(pool.active_count(), 0);
    }

    #[test]
    fn enqueue_preserves_order() {
        let mut pool = make_pool(2);
        pool.enqueue(make_session("A"));
        pool.enqueue(make_session("B"));
        pool.enqueue(make_session("C"));
        assert_eq!(pool.queued_count(), 3);
        assert_eq!(pool.total_count(), 3);
    }

    #[test]
    fn try_promote_moves_to_active() {
        let mut pool = make_pool(2);
        pool.enqueue(make_session("A"));
        pool.enqueue(make_session("B"));
        let promoted = pool.try_promote();
        assert_eq!(promoted.len(), 2);
        assert_eq!(pool.active_count(), 2);
        assert_eq!(pool.queued_count(), 0);
    }

    #[test]
    fn try_promote_respects_max_concurrent() {
        let mut pool = make_pool(2);
        pool.enqueue(make_session("A"));
        pool.enqueue(make_session("B"));
        pool.enqueue(make_session("C"));
        let promoted = pool.try_promote();
        assert_eq!(promoted.len(), 2);
        assert_eq!(pool.active_count(), 2);
        assert_eq!(pool.queued_count(), 1);
    }

    #[test]
    fn try_promote_returns_empty_when_at_capacity() {
        let mut pool = make_pool(1);
        pool.enqueue(make_session("first"));
        pool.try_promote();
        pool.enqueue(make_session("second"));
        let promoted = pool.try_promote();
        assert_eq!(promoted.len(), 0);
    }

    #[test]
    fn try_promote_returns_empty_when_queue_empty() {
        let mut pool = make_pool(4);
        let promoted = pool.try_promote();
        assert!(promoted.is_empty());
    }

    #[test]
    fn on_session_completed_moves_to_finished() {
        let mut pool = make_pool(2);
        let session = make_session("done");
        let id = session.id;
        pool.enqueue(session);
        pool.try_promote();

        pool.on_session_completed(id);
        assert_eq!(pool.active_count(), 0);
        assert_eq!(pool.total_count(), 1); // in finished
    }

    #[test]
    fn on_session_completed_unknown_id_is_noop() {
        let mut pool = make_pool(2);
        pool.enqueue(make_session("running"));
        pool.try_promote();
        pool.on_session_completed(Uuid::new_v4());
        assert_eq!(pool.active_count(), 1);
    }

    #[test]
    fn on_session_completed_frees_slot_for_promotion() {
        let mut pool = make_pool(1);
        let s1 = make_session("first");
        let id1 = s1.id;
        pool.enqueue(s1);
        pool.enqueue(make_session("second"));
        pool.try_promote(); // promotes first, second stays queued

        pool.on_session_completed(id1);
        let promoted = pool.try_promote();
        assert_eq!(promoted.len(), 1);
        assert_eq!(pool.active_count(), 1);
        assert_eq!(pool.queued_count(), 0);
    }

    #[test]
    fn get_session_mut_finds_active() {
        let mut pool = make_pool(2);
        let session = make_session("find me");
        let id = session.id;
        pool.enqueue(session);
        pool.try_promote();
        assert!(pool.get_session_mut(id).is_some());
    }

    #[test]
    fn get_session_mut_finds_queued() {
        let mut pool = make_pool(0);
        let session = make_session("queued");
        let id = session.id;
        pool.enqueue(session);
        assert!(pool.get_session_mut(id).is_some());
    }

    #[test]
    fn get_session_mut_finds_finished() {
        let mut pool = make_pool(2);
        let session = make_session("finished");
        let id = session.id;
        pool.enqueue(session);
        pool.try_promote();
        pool.on_session_completed(id);
        assert!(pool.get_session_mut(id).is_some());
    }

    #[test]
    fn get_session_mut_returns_none_for_unknown() {
        let mut pool = make_pool(2);
        assert!(pool.get_session_mut(Uuid::new_v4()).is_none());
    }

    #[test]
    fn all_done_false_when_empty() {
        let pool = make_pool(2);
        assert!(!pool.all_done());
    }

    #[test]
    fn all_done_false_when_active() {
        let mut pool = make_pool(2);
        pool.enqueue(make_session("running"));
        pool.try_promote();
        assert!(!pool.all_done());
    }

    #[test]
    fn all_done_false_when_queued() {
        let mut pool = make_pool(0);
        pool.enqueue(make_session("waiting"));
        assert!(!pool.all_done());
    }

    #[test]
    fn all_done_true_when_all_finished() {
        let mut pool = make_pool(2);
        let s1 = make_session("a");
        let s2 = make_session("b");
        let id1 = s1.id;
        let id2 = s2.id;
        pool.enqueue(s1);
        pool.enqueue(s2);
        pool.try_promote();
        pool.on_session_completed(id1);
        pool.on_session_completed(id2);
        assert!(pool.all_done());
    }

    #[tokio::test]
    async fn kill_all_moves_active_to_finished() {
        let mut pool = make_pool(2);
        pool.enqueue(make_session("kill me"));
        pool.try_promote();
        pool.kill_all().await;
        assert_eq!(pool.active_count(), 0);
        assert!(pool.all_done());
    }

    #[test]
    fn file_claims_starts_empty() {
        let pool = make_pool(2);
        assert_eq!(pool.file_claims.total_claims(), 0);
    }

    #[test]
    fn file_claims_accessible() {
        let mut pool = make_pool(2);
        let session = make_session("claimer");
        let id = session.id;
        pool.enqueue(session);
        pool.file_claims.claim("src/target.rs", id);
        assert_eq!(pool.file_claims.total_claims(), 1);
    }

    #[test]
    fn on_session_completed_releases_claims() {
        let mut pool = make_pool(2);
        let session = make_session("claimer");
        let id = session.id;
        pool.enqueue(session);
        pool.try_promote();
        pool.file_claims.claim("src/a.rs", id);
        pool.file_claims.claim("src/b.rs", id);

        pool.on_session_completed(id);
        assert_eq!(pool.file_claims.total_claims(), 0);
    }

    #[test]
    fn worktree_created_on_promote() {
        let mock = MockWorktreeManager::new();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut pool = SessionPool::new(2, Box::new(mock), tx);

        pool.enqueue(make_session_with_issue("work", 42));
        pool.try_promote();

        // Verify worktree_path was set on the active session
        let managed = &pool.active[0];
        assert!(managed.worktree_path.is_some());
        assert!(managed.worktree_path.as_ref().unwrap().to_string_lossy().contains("issue-42"));
    }

    #[test]
    fn all_sessions_returns_all_buckets() {
        let mut pool = make_pool(1);
        let s1 = make_session("active");
        let s2 = make_session("queued");
        let id1 = s1.id;

        pool.enqueue(s1);
        pool.enqueue(s2);
        pool.try_promote(); // promotes s1, s2 stays queued

        assert_eq!(pool.all_sessions().len(), 2);

        // Complete s1 to move it to finished
        pool.on_session_completed(id1);
        pool.try_promote(); // promotes s2

        assert_eq!(pool.all_sessions().len(), 2);
        // 1 active (s2) + 1 finished (s1)
        assert_eq!(pool.active_count(), 1);
    }
}
