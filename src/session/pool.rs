use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::image::copy_images_to_worktree;
use super::manager::{ManagedSession, SessionEvent};
use super::types::{Session, SessionOrigin, SessionStatus};
use super::worktree::WorktreeManager;
use crate::agent_provider::{AgentProvider, ClaudeProvider};
use crate::state::file_claims::FileClaimManager;
use crate::templates::RenderedTemplateStore;
use crate::turboquant::adapter::TurboQuantAdapter;

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
    /// Agent provider used for newly promoted sessions.
    provider: Arc<dyn AgentProvider>,
    /// Agent providers keyed by configured `[agents.*]` id.
    agent_providers: HashMap<String, Arc<dyn AgentProvider>>,
    /// Guardrail prompt appended to every session's system prompt.
    guardrail_prompt: Option<String>,
    /// TurboQuant adapter used to compact the system-prompt appendix.
    turboquant_adapter: Option<Arc<TurboQuantAdapter>>,
    /// Token budget for system-prompt compaction.
    system_prompt_budget: usize,
    /// Cached knowledge-base appendix loaded once at configure time.
    knowledge_appendix: Option<String>,
    /// Lookup for rendered HTTP-provider templates (issue #707). `None`
    /// disables injection — used in unit tests that don't care about
    /// templates and as a no-op fallback when XDG cache resolution fails.
    rendered_template_store: Option<Arc<dyn RenderedTemplateStore>>,
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
            provider: Arc::new(ClaudeProvider::default()),
            agent_providers: HashMap::new(),
            guardrail_prompt: None,
            turboquant_adapter: None,
            system_prompt_budget: 0,
            knowledge_appendix: None,
            rendered_template_store: None,
        }
    }

    /// Install a rendered-template lookup so HTTP-generic provider sessions
    /// can receive the cached canonical command body at promotion time.
    /// See issue #707.
    #[allow(dead_code)] // Reason: wired by setup_app_from_config once active_command is plumbed
    pub fn set_rendered_template_store(&mut self, store: Arc<dyn RenderedTemplateStore>) {
        self.rendered_template_store = Some(store);
    }

    /// Inject a shared TurboQuant adapter for system-prompt compaction.
    /// Token budget of 0 disables truncation (dedup still runs).
    pub fn set_turboquant_adapter(&mut self, adapter: Arc<TurboQuantAdapter>, budget: usize) {
        self.turboquant_adapter = Some(adapter);
        self.system_prompt_budget = budget;
    }

    /// Cache the knowledge-base appendix (loaded once from `.maestro/knowledge.md`)
    /// so promotions don't hit disk per session.
    pub fn set_knowledge_appendix(&mut self, appendix: Option<String>) {
        self.knowledge_appendix = appendix;
    }

    /// Get the max concurrent session limit.
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }

    /// Set the permission mode for new sessions.
    pub fn set_permission_mode(&mut self, mode: String) {
        self.permission_mode = mode;
    }

    /// Set the guardrail prompt appended to every session's system prompt.
    pub fn set_guardrail_prompt(&mut self, prompt: String) {
        self.guardrail_prompt = Some(prompt);
    }

    /// Assemble the system-prompt appendix for an interaction launch (#946):
    /// mode prompt + guardrail + knowledge, joined with "\n\n". This is the
    /// subset of `try_promote`'s appendix reachable before a live `Session`
    /// exists — file-claims and rendered HTTP templates are keyed on a
    /// promoted session and are intentionally excluded here. Returns `None`
    /// when every component is empty.
    pub(crate) fn interaction_appendix(
        &self,
        mode_config: Option<&crate::session::types::SessionModeConfig>,
    ) -> Option<String> {
        let mut components: Vec<String> = Vec::new();
        if let Some(mode) = mode_config
            && !mode.system_prompt.trim().is_empty()
        {
            components.push(mode.system_prompt.clone());
        }
        if let Some(ref guardrail) = self.guardrail_prompt {
            components.push(guardrail.clone());
        }
        if let Some(ref knowledge) = self.knowledge_appendix {
            components.push(knowledge.clone());
        }
        if components.is_empty() {
            None
        } else {
            Some(components.join("\n\n"))
        }
    }

    /// Set the allowed tools whitelist for new sessions.
    pub fn set_allowed_tools(&mut self, tools: Vec<String>) {
        self.allowed_tools = tools;
    }

    /// Set the agent provider for newly promoted sessions.
    pub fn set_provider(&mut self, provider: Arc<dyn AgentProvider>) {
        self.provider = provider;
    }

    /// Set provider registry for per-session agent selection.
    pub fn set_agent_providers(&mut self, providers: HashMap<String, Arc<dyn AgentProvider>>) {
        self.agent_providers = providers;
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
            let branch_name = format!("maestro/{}", slug);
            let (worktree_path, branch) = match self.worktree_mgr.create(&slug) {
                Ok(path) => {
                    session.log_activity(format!("Worktree created: {}", path.display()));
                    (Some(path), Some(branch_name))
                }
                Err(e) => {
                    let msg = format!("Worktree skipped (running in cwd): {}", e);
                    tracing::warn!("{}", msg);
                    session.log_activity(msg);
                    (None, None)
                }
            };

            // Copy images to worktree if available
            if let Some(ref wt_path) = worktree_path
                && !session.image_paths.is_empty()
            {
                match copy_images_to_worktree(&session.image_paths, wt_path) {
                    Ok(relative_paths) => {
                        session.log_activity(format!(
                            "Copied {} image(s) to worktree",
                            session.image_paths.len()
                        ));
                        session.image_paths = relative_paths;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to copy images to worktree: {}", e);
                        session.log_activity(format!("Image copy failed (non-fatal): {}", e));
                    }
                }
            }

            // Build system prompt appendix from file claims + guardrails + knowledge base.
            let mode_config = session.mode_config.clone();
            let mut components: Vec<String> = Vec::new();
            if let Some(ref mode) = mode_config
                && !mode.system_prompt.trim().is_empty()
            {
                components.push(mode.system_prompt.clone());
            }
            if let Some(fc) = self.file_claims.build_system_prompt(session.id) {
                components.push(fc);
            }
            if let Some(ref guardrail) = self.guardrail_prompt {
                components.push(guardrail.clone());
            }
            if let Some(ref knowledge) = self.knowledge_appendix {
                components.push(knowledge.clone());
            }

            // Resolve the provider once, honoring `session.agent_id` overrides.
            // Reused below for rendered-template lookup (issue #707) and for
            // setting `managed.provider`.
            let provider = Arc::clone(self.resolve_provider(&session));

            // HTTP-provider rendered-template injection (issue #707).
            // Inserted before the TurboQuant branch below so oversized
            // templates still get compacted.
            if let Some(rendered) = self.rendered_template_for_session(&session, &provider) {
                let command = session.active_command.as_deref().unwrap_or("?").to_string();
                tracing::info!(
                    session_id = %session.id,
                    provider = %provider.id(),
                    command = %command,
                    bytes = rendered.len(),
                    "injecting rendered HTTP-provider template into system_prompt_appendix"
                );
                components.push(rendered);
            }

            let system_prompt = if components.is_empty() {
                None
            } else if let Some(ref tq) = self.turboquant_adapter
                && tq.is_active()
            {
                let refs: Vec<&str> = components.iter().map(|s| s.as_str()).collect();
                Some(tq.compact_system_prompt(&refs, self.system_prompt_budget))
            } else {
                Some(components.join("\n\n"))
            };

            // Session remains Queued until ManagedSession::spawn() transitions it
            let mut managed =
                ManagedSession::with_worktree(session, worktree_path, branch, system_prompt);
            managed.set_provider(Arc::clone(&provider));
            managed.permission_mode = mode_config
                .as_ref()
                .and_then(|mode| mode.permission_mode.clone())
                .or_else(|| Some(self.permission_mode.clone()));
            managed.allowed_tools = mode_config
                .map(|mode| mode.allowed_tools)
                .unwrap_or_else(|| self.allowed_tools.clone());
            let id = managed.session.id;
            self.active.push(managed);
            promoted.push(id);
        }

        promoted
    }

    /// Resolve the provider this session will run against, honoring any
    /// `session.agent_id` override against the registered provider map.
    fn resolve_provider(&self, session: &Session) -> &Arc<dyn AgentProvider> {
        session
            .agent_id
            .as_ref()
            .and_then(|id| self.agent_providers.get(id))
            .unwrap_or(&self.provider)
    }

    /// Resolve the rendered HTTP-provider template body for this session,
    /// applying every gate in order. Returns `None` (no injection) when
    /// origin is not `DirectUser`, when no `active_command` is set, when
    /// the resolved provider is not HTTP-generic, when the store is absent,
    /// or on cache miss. Caller supplies the already-resolved provider to
    /// avoid re-walking the agent map. See issue #707.
    fn rendered_template_for_session(
        &self,
        session: &Session,
        provider: &Arc<dyn AgentProvider>,
    ) -> Option<String> {
        if session.origin != SessionOrigin::DirectUser {
            return None;
        }
        let command = session.active_command.as_deref()?;
        if provider.template_rules().target_dir().is_some() {
            return None;
        }
        let store = self.rendered_template_store.as_ref()?;
        store.lookup(provider.id(), command)
    }

    /// Move a terminal session from `active` to `finished`, deciding whether
    /// to tear down the worktree based on status: `FailedGates` retains the
    /// worktree (uncommitted model edits live there); every other terminal
    /// status tears it down. Returns `true` when a session was finalized.
    pub fn finalize(&mut self, session_id: Uuid) -> bool {
        let Some(idx) = self.active.iter().position(|m| m.session.id == session_id) else {
            return false;
        };
        // A session finalized while still `Retrying` has had its retry spawned
        // as a separate session. Advance it to a terminal status, or it lingers
        // in `finished` as a non-terminal "RETRYING" row that renders like an
        // active session and that the active-only kill path can't remove.
        // `Killed` is the only terminal status reachable from `Retrying`.
        if self.active[idx].session.status == SessionStatus::Retrying {
            let _ = self.active[idx].session.transition_to(
                SessionStatus::Killed,
                crate::session::transition::TransitionReason::RetryTriggered,
            );
        }
        let retain = self.active[idx].session.status == SessionStatus::FailedGates;
        self.finalize_at(idx, retain);
        true
    }

    /// Move a terminal session to `finished` and tear down its worktree.
    /// Test-only affordance: production code routes finalization through
    /// [`Self::finalize`], which dispatches to teardown vs. retain based
    /// on the session's terminal status. Tests use the explicit form to
    /// assert the correct branch fired.
    #[cfg(test)]
    pub fn finalize_and_teardown(&mut self, session_id: Uuid) {
        if let Some(idx) = self.active.iter().position(|m| m.session.id == session_id) {
            self.finalize_at(idx, false);
        }
    }

    /// Move a terminal session to `finished` without removing its
    /// worktree. Test-only counterpart to `finalize_and_teardown` — see
    /// note above. Production code reaches this branch via
    /// [`Self::finalize`] when the session ended in `FailedGates`.
    #[cfg(test)]
    pub fn finalize_retain_worktree(&mut self, session_id: Uuid) {
        if let Some(idx) = self.active.iter().position(|m| m.session.id == session_id) {
            self.finalize_at(idx, true);
        }
    }

    fn finalize_at(&mut self, idx: usize, retain_worktree: bool) {
        let managed = self.active.remove(idx);
        let session_id = managed.session.id;
        self.file_claims.release_all(session_id);
        if !retain_worktree {
            let slug = session_slug(&managed.session);
            let _ = self.worktree_mgr.remove(&slug);
        }
        self.finished.push(managed);
    }

    /// Whether a worktree exists for the given slug. Test-only
    /// accessor that delegates to the underlying `WorktreeManager` so
    /// integration tests can assert teardown vs. retain without
    /// exposing the manager itself. Production code never inspects
    /// worktree existence at this layer.
    #[cfg(test)]
    pub fn worktree_exists(&self, slug: &str) -> bool {
        self.worktree_mgr.exists(slug)
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

    /// Iterate over all session statuses without allocating a Vec.
    pub fn all_statuses(&self) -> impl Iterator<Item = &SessionStatus> {
        self.active
            .iter()
            .map(|m| &m.session.status)
            .chain(self.finished.iter().map(|m| &m.session.status))
            .chain(self.queue.iter().map(|s| &s.status))
    }

    /// Get session UUID at a given display index (from all_sessions ordering).
    pub fn session_id_at_index(&self, index: usize) -> Option<Uuid> {
        self.session_at_index(index).map(|s| s.id)
    }

    /// Borrow the session at a given display index without allocating —
    /// mirrors `all_statuses` for the on-render hot path. Active → finished
    /// → queue ordering matches `all_sessions`.
    pub fn session_at_index(&self, index: usize) -> Option<&Session> {
        self.active
            .iter()
            .map(|m| &m.session)
            .chain(self.finished.iter().map(|m| &m.session))
            .chain(self.queue.iter())
            .nth(index)
    }

    /// Find a session by UUID from any bucket.
    pub fn get_session(&self, session_id: Uuid) -> Option<&Session> {
        self.active
            .iter()
            .find(|m| m.session.id == session_id)
            .map(|m| &m.session)
            .or_else(|| {
                self.finished
                    .iter()
                    .find(|m| m.session.id == session_id)
                    .map(|m| &m.session)
            })
            .or_else(|| self.queue.iter().find(|s| s.id == session_id))
    }

    /// Mutable access to a managed session by ID (active only).
    pub fn get_active_mut(&mut self, session_id: Uuid) -> Option<&mut ManagedSession> {
        self.active.iter_mut().find(|m| m.session.id == session_id)
    }

    /// Mutable access to a managed session by issue number (active or finished).
    /// Skips `SessionMode::Interactive` sessions (#947) — the one-shot
    /// completion machinery (pr_retry, completion_pipeline, auto_pr) must
    /// never grab a kept-alive chat session.
    pub fn find_by_issue_mut(&mut self, issue_number: u64) -> Option<&mut ManagedSession> {
        let is_match = |m: &&mut ManagedSession| {
            m.session.issue_number == Some(issue_number)
                && m.session.session_mode != crate::session::types::SessionMode::Interactive
        };
        if let Some(m) = self.active.iter_mut().find(is_match) {
            return Some(m);
        }
        self.finished.iter_mut().find(is_match)
    }

    /// Live (non-terminal) interactive-mode session for an issue (#948).
    /// Replaces `find_active_interaction_by_issue` — "active" now means
    /// the unified `Session` has not reached a terminal status.
    pub fn interactive_managed(&self, issue_number: u64) -> Option<&ManagedSession> {
        self.active.iter().find(|m| {
            m.session.session_mode == crate::session::types::SessionMode::Interactive
                && m.session.issue_number == Some(issue_number)
                && !m.session.status.is_terminal()
        })
    }

    /// Mutable twin of [`Self::interactive_managed`] (#739/#948). Used by
    /// the `/pushup` marker consumer to call `signal_terminator` and by
    /// the turn dispatch to mutate `turns`/`turn_state`.
    pub fn interactive_managed_mut(&mut self, issue_number: u64) -> Option<&mut ManagedSession> {
        self.active.iter_mut().find(|m| {
            m.session.session_mode == crate::session::types::SessionMode::Interactive
                && m.session.issue_number == Some(issue_number)
                && !m.session.status.is_terminal()
        })
    }

    /// Create + register the unified interactive session for `issue_number`
    /// (#947/#948): a real `SessionMode::Interactive` [`Session`] over a
    /// fresh worktree (non-fatal — falls back to cwd), driven by the pool's
    /// default provider (whose parked PTY children survive across turns,
    /// #751). Idempotent while a live interactive session exists for the
    /// issue. The prompt is seeded at first-turn dispatch (deferred issue
    /// fetch, #953).
    ///
    /// Deliberate deviations from `try_promote`:
    /// - bypasses `max_concurrent` — a user-driven chat must not queue
    ///   behind batch sessions;
    /// - no `system_prompt_appendix` — the appendix is embedded in the
    ///   first-turn prompt by `build_interaction_launch_prompt` (#946).
    pub fn create_interaction_session(
        &mut self,
        issue_number: u64,
        produce_pr: bool,
        model: String,
        mode: String,
        agent_id: Option<String>,
    ) -> Uuid {
        if let Some(existing) = self.interactive_pipeline_session_id(issue_number) {
            return existing;
        }
        let slug = format!("issue-{issue_number}");
        let (worktree_path, branch) = match self.worktree_mgr.create(&slug) {
            Ok(path) => (path, format!("maestro/{slug}")),
            Err(e) => {
                tracing::warn!("interaction worktree skipped (running in cwd): {e}");
                (PathBuf::from("."), format!("maestro/{slug}"))
            }
        };
        let mut session = Session::new(String::new(), model, mode, Some(issue_number), None);
        session.session_mode = crate::session::types::SessionMode::Interactive;
        session.produce_pr = produce_pr;
        // #929: carry the selected agent id so turns route to that provider
        // (qwen/minimax/HTTP-generic), not the Claude default. The first turn
        // spawns this ManagedSession directly, bypassing `try_promote`'s
        // `resolve_provider`, so the provider is resolved here too.
        session.agent_id = agent_id;
        let provider = Arc::clone(self.resolve_provider(&session));
        let mut managed =
            ManagedSession::with_worktree(session, Some(worktree_path), Some(branch), None);
        managed.set_provider(provider);
        managed.permission_mode = Some(self.permission_mode.clone());
        managed.allowed_tools = self.allowed_tools.clone();
        let id = managed.session.id;
        self.active.push(managed);
        id
    }

    /// Id of the live Interactive-mode pipeline session for an issue
    /// (#947), if one exists. Terminal (quit/PR-terminated) sessions are
    /// excluded so re-entry after a close starts fresh.
    pub fn interactive_pipeline_session_id(&self, issue_number: u64) -> Option<Uuid> {
        self.interactive_managed(issue_number).map(|m| m.session.id)
    }

    /// Number of registered interactive sessions, any state (#738 re-entry
    /// assertions).
    #[cfg(test)]
    pub fn interaction_count(&self) -> usize {
        self.active
            .iter()
            .chain(self.finished.iter())
            .filter(|m| m.session.session_mode == crate::session::types::SessionMode::Interactive)
            .count()
    }

    /// Append a turn to the live interactive session for `issue_number`.
    /// Test seam for re-entry assertions (#738).
    #[cfg(test)]
    pub fn test_push_interaction_turn(
        &mut self,
        issue_number: u64,
        turn: super::interaction::TurnRecord,
    ) {
        if let Some(m) = self.interactive_managed_mut(issue_number) {
            m.session.turns.push(turn);
        }
    }

    /// Mutable access to a session by ID across all buckets.
    #[allow(dead_code)] // Reason: session mutation by ID — to be used in orchestration
    pub fn get_session_mut(&mut self, session_id: Uuid) -> Option<&mut Session> {
        if let Some(m) = self.active.iter_mut().find(|m| m.session.id == session_id) {
            return Some(&mut m.session);
        }
        if let Some(m) = self
            .finished
            .iter_mut()
            .find(|m| m.session.id == session_id)
        {
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

    #[allow(dead_code)] // Reason: queue size for TUI display
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

    /// Remove a finished session from the pool entirely.
    /// Returns true if the session was found and removed.
    #[allow(dead_code)]
    pub fn dismiss_session(&mut self, session_id: Uuid) -> bool {
        if let Some(idx) = self
            .finished
            .iter()
            .position(|m| m.session.id == session_id)
        {
            self.finished.remove(idx);
            true
        } else {
            false
        }
    }

    /// Remove all finished sessions that are in a terminal state.
    /// Returns the number of sessions dismissed.
    pub fn dismiss_all_completed(&mut self) -> usize {
        let before = self.finished.len();
        self.finished.retain(|m| !m.session.status.is_terminal());
        before - self.finished.len()
    }

    /// Kill a single active session by ID.
    pub async fn kill_session(&mut self, session_id: Uuid) -> anyhow::Result<bool> {
        if let Some(managed) = self.active.iter_mut().find(|m| m.session.id == session_id) {
            managed.kill().await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get the event sender for spawning sessions.
    pub fn event_tx(&self) -> mpsc::UnboundedSender<SessionEvent> {
        self.event_tx.clone()
    }

    /// Decrement transition flash counters for all sessions (called once per render tick).
    pub fn tick_flash_counters(&mut self) {
        for managed in self.active.iter_mut().chain(self.finished.iter_mut()) {
            if managed.session.transition_flash_remaining > 0 {
                managed.session.transition_flash_remaining -= 1;
            }
        }
    }
}

fn session_slug(session: &Session) -> String {
    if session.issue_numbers.len() >= 2 {
        // Reuse unified_branch_name which returns "maestro/unified-N-M";
        // strip the "maestro/" prefix since the caller adds it.
        let full = crate::provider::github::pr::unified_branch_name(&session.issue_numbers);
        return full.strip_prefix("maestro/").unwrap_or(&full).to_string();
    }
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
            None,
        )
    }

    fn make_session_with_issue(prompt: &str, issue: u64) -> Session {
        Session::new(
            prompt.to_string(),
            "opus".to_string(),
            "orchestrator".to_string(),
            Some(issue),
            None,
        )
    }

    fn must_get_active_mut(pool: &mut SessionPool, id: Uuid) -> &mut ManagedSession {
        match pool.get_active_mut(id) {
            Some(managed) => managed,
            None => panic!("expected active session {id}"),
        }
    }

    fn must_get_session(pool: &SessionPool, id: Uuid) -> &Session {
        match pool.get_session(id) {
            Some(session) => session,
            None => panic!("expected session {id} in pool"),
        }
    }

    fn must_get_appendix(managed: &ManagedSession) -> &str {
        match managed.system_prompt_appendix.as_deref() {
            Some(appendix) => appendix,
            None => panic!("expected system prompt appendix"),
        }
    }

    // --- Issues #947/#948: unified interaction pipeline session ---

    fn create_unified(pool: &mut SessionPool, issue: u64, produce_pr: bool) -> Uuid {
        pool.create_interaction_session(
            issue,
            produce_pr,
            "opus".to_string(),
            "orchestrator".to_string(),
            None,
        )
    }

    #[test]
    fn create_interaction_session_registers_interactive_managed_session() {
        let mut pool = make_pool(2);
        let id = create_unified(&mut pool, 947, true);

        let managed = must_get_active_mut(&mut pool, id);
        assert_eq!(
            managed.session.session_mode,
            crate::session::types::SessionMode::Interactive
        );
        assert_eq!(managed.session.issue_number, Some(947));
        assert!(managed.session.produce_pr);
        assert_eq!(managed.session.status, SessionStatus::Queued);
        // Appendix is embedded in the first-turn prompt by
        // build_interaction_launch_prompt (#946) — never set on the request.
        assert!(managed.system_prompt_appendix.is_none());
        assert_eq!(pool.interaction_count(), 1);
    }

    #[test]
    fn create_interaction_session_is_idempotent_while_live() {
        let mut pool = make_pool(2);
        let first = create_unified(&mut pool, 947, false);
        let second = create_unified(&mut pool, 947, false);
        assert_eq!(first, second);
        assert_eq!(pool.active_count(), 1);
    }

    #[test]
    fn create_interaction_session_after_termination_starts_fresh() {
        use crate::session::transition::TransitionReason;
        let mut pool = make_pool(2);
        let first = create_unified(&mut pool, 947, false);
        must_get_active_mut(&mut pool, first)
            .session
            .transition_to(SessionStatus::Killed, TransitionReason::UserKill)
            .unwrap();

        let second = create_unified(&mut pool, 947, false);
        assert_ne!(first, second, "a quit interaction must not be reopened");
    }

    #[test]
    fn create_interaction_session_bypasses_capacity_cap() {
        // A user-driven chat must not queue behind batch sessions.
        let mut pool = make_pool(1);
        pool.enqueue(make_session("batch"));
        pool.try_promote();
        assert_eq!(pool.active_count(), 1);

        create_unified(&mut pool, 947, false);
        assert_eq!(pool.active_count(), 2);
    }

    #[test]
    fn create_interaction_session_routes_to_selected_agent_provider() {
        // #929: an interaction launched while a non-Claude agent is selected
        // must run its turns against that provider, not the Claude default.
        let mut pool = make_pool(1);
        let mut providers: std::collections::HashMap<String, Arc<dyn AgentProvider>> =
            std::collections::HashMap::new();
        providers.insert("qwen".to_string(), Arc::new(FakeHttpProvider));
        pool.set_agent_providers(providers);

        let id = pool.create_interaction_session(
            7,
            false,
            "qwen-2.5".to_string(),
            "orchestrator".to_string(),
            Some("qwen".to_string()),
        );

        let managed = pool.get_active_mut(id).expect("session is active");
        assert_eq!(
            managed.session.agent_id.as_deref(),
            Some("qwen"),
            "selected agent id must ride on the session"
        );
        assert_eq!(
            managed.provider_id(),
            "qwen",
            "turns must route to the selected provider, not the Claude default"
        );
    }

    #[test]
    fn create_interaction_session_falls_back_to_default_provider_when_agent_unknown() {
        // An unset/unknown agent id keeps the Claude default — no panic.
        let mut pool = make_pool(1);
        let id = pool.create_interaction_session(
            7,
            false,
            "opus".to_string(),
            "orchestrator".to_string(),
            None,
        );
        let managed = pool.get_active_mut(id).expect("session is active");
        assert_eq!(managed.provider_id(), "claude");
    }

    #[test]
    fn find_by_issue_mut_skips_interactive_sessions() {
        // One-shot completion machinery (pr_retry, completion_pipeline,
        // auto_pr) must never grab a kept-alive chat session.
        let mut pool = make_pool(2);
        create_unified(&mut pool, 947, false);
        assert!(pool.find_by_issue_mut(947).is_none());
    }

    #[test]
    fn interactive_managed_returns_none_when_empty() {
        let pool = make_pool(2);
        assert!(pool.interactive_managed(1).is_none());
    }

    #[test]
    fn interactive_managed_returns_none_for_other_issue() {
        let mut pool = make_pool(2);
        create_unified(&mut pool, 42, false);
        assert!(pool.interactive_managed(99).is_none());
    }

    #[test]
    fn interactive_managed_finds_live_session() {
        let mut pool = make_pool(2);
        create_unified(&mut pool, 42, false);
        let found = pool.interactive_managed(42);
        assert!(found.is_some());
        assert_eq!(found.unwrap().session.issue_number, Some(42));
    }

    #[test]
    fn interactive_managed_skips_terminated_session() {
        use crate::session::transition::TransitionReason;
        let mut pool = make_pool(2);
        let id = create_unified(&mut pool, 42, false);
        must_get_active_mut(&mut pool, id)
            .session
            .transition_to(SessionStatus::Killed, TransitionReason::UserKill)
            .unwrap();
        assert!(pool.interactive_managed(42).is_none());
    }

    #[test]
    fn interactive_managed_skips_one_shot_session_on_same_issue() {
        let mut pool = make_pool(2);
        pool.enqueue(make_session_with_issue("one shot", 42));
        pool.try_promote();
        assert!(pool.interactive_managed(42).is_none());
    }

    #[test]
    fn test_push_interaction_turn_lands_on_session_turns() {
        let mut pool = make_pool(2);
        let id = create_unified(&mut pool, 42, false);
        pool.test_push_interaction_turn(
            42,
            crate::session::interaction::TurnRecord {
                role: crate::session::interaction::TurnRole::User,
                content: "hi".into(),
                started_at: chrono::Utc::now(),
                finished_at: Some(chrono::Utc::now()),
            },
        );
        assert_eq!(must_get_active_mut(&mut pool, id).session.turns.len(), 1);
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
    fn try_promote_applies_mode_config_to_managed_session() {
        let mut pool = make_pool(1);
        pool.set_permission_mode("bypassPermissions".to_string());
        pool.set_allowed_tools(vec!["Read".to_string(), "Write".to_string()]);
        let session =
            make_session("A").with_mode_config(Some(crate::session::types::SessionModeConfig {
                system_prompt: "Vibe mode prompt".to_string(),
                allowed_tools: vec!["Read".to_string()],
                permission_mode: Some("plan".to_string()),
            }));
        pool.enqueue(session);

        let promoted = pool.try_promote();
        assert_eq!(promoted.len(), 1);
        let managed = must_get_active_mut(&mut pool, promoted[0]);

        assert_eq!(managed.permission_mode.as_deref(), Some("plan"));
        assert_eq!(managed.allowed_tools, vec!["Read".to_string()]);
        assert_eq!(must_get_appendix(managed), "Vibe mode prompt");
    }

    #[test]
    fn try_promote_uses_default_spawn_settings_without_mode_config() {
        let mut pool = make_pool(1);
        pool.set_permission_mode("bypassPermissions".to_string());
        pool.set_allowed_tools(vec!["Read".to_string(), "Write".to_string()]);
        pool.enqueue(make_session("A"));

        let promoted = pool.try_promote();
        let managed = must_get_active_mut(&mut pool, promoted[0]);

        assert_eq!(
            managed.permission_mode.as_deref(),
            Some("bypassPermissions")
        );
        assert_eq!(
            managed.allowed_tools,
            vec!["Read".to_string(), "Write".to_string()]
        );
        assert!(managed.system_prompt_appendix.is_none());
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
    fn finalize_and_teardown_moves_to_finished() {
        let mut pool = make_pool(2);
        let session = make_session("done");
        let id = session.id;
        pool.enqueue(session);
        pool.try_promote();

        pool.finalize_and_teardown(id);
        assert_eq!(pool.active_count(), 0);
        assert_eq!(pool.total_count(), 1); // in finished
    }

    #[test]
    fn finalize_and_teardown_unknown_id_is_noop() {
        let mut pool = make_pool(2);
        pool.enqueue(make_session("running"));
        pool.try_promote();
        pool.finalize_and_teardown(Uuid::new_v4());
        assert_eq!(pool.active_count(), 1);
    }

    #[test]
    fn finalize_advances_retrying_session_to_terminal_status() {
        // A session whose retry has been spawned as a separate session is
        // finalized while still `Retrying`. It must land in `finished` with a
        // terminal status, not linger as a non-terminal "RETRYING" zombie row
        // that the active-only kill path cannot remove.
        use crate::session::transition::TransitionReason;
        let mut pool = make_pool(2);
        let s = make_session("retry-me");
        let id = s.id;
        pool.enqueue(s);
        pool.try_promote();
        {
            let m = must_get_active_mut(&mut pool, id);
            for (st, reason) in [
                (SessionStatus::Spawning, TransitionReason::Spawned),
                (SessionStatus::Running, TransitionReason::Promoted),
                (SessionStatus::Stalled, TransitionReason::HealthStall),
                (SessionStatus::Retrying, TransitionReason::RetryTriggered),
            ] {
                let _ = m.session.transition_to(st, reason);
            }
            assert_eq!(m.session.status, SessionStatus::Retrying);
        }

        assert!(pool.finalize(id));
        let finished = pool
            .all_sessions()
            .into_iter()
            .find(|s| s.id == id)
            .expect("finalized session must remain in the pool");
        assert!(
            finished.status.is_terminal(),
            "a finalized Retrying session must end terminal, got {:?}",
            finished.status
        );
    }

    #[test]
    fn finalize_and_teardown_frees_slot_for_promotion() {
        let mut pool = make_pool(1);
        let s1 = make_session("first");
        let id1 = s1.id;
        pool.enqueue(s1);
        pool.enqueue(make_session("second"));
        pool.try_promote(); // promotes first, second stays queued

        pool.finalize_and_teardown(id1);
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
        pool.finalize_and_teardown(id);
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
        pool.finalize_and_teardown(id1);
        pool.finalize_and_teardown(id2);
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
    fn finalize_and_teardown_releases_claims() {
        let mut pool = make_pool(2);
        let session = make_session("claimer");
        let id = session.id;
        pool.enqueue(session);
        pool.try_promote();
        pool.file_claims.claim("src/a.rs", id);
        pool.file_claims.claim("src/b.rs", id);

        pool.finalize_and_teardown(id);
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
        let path = match managed.worktree_path.as_ref() {
            Some(path) => path,
            None => panic!("expected worktree path for issue session"),
        };
        assert!(path.to_string_lossy().contains("issue-42"));
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
        pool.finalize_and_teardown(id1);
        pool.try_promote(); // promotes s2

        assert_eq!(pool.all_sessions().len(), 2);
        // 1 active (s2) + 1 finished (s1)
        assert_eq!(pool.active_count(), 1);
    }

    // --- Issue #203: dismiss/kill tests ---

    #[test]
    fn dismiss_session_removes_from_finished() {
        let mut pool = make_pool(1);
        let s = make_session("A");
        let id = s.id;
        pool.enqueue(s);
        pool.try_promote();
        pool.finalize_and_teardown(id);
        assert_eq!(pool.total_count(), 1); // 1 finished

        assert!(pool.dismiss_session(id));
        assert_eq!(pool.total_count(), 0);
    }

    #[test]
    fn dismiss_session_unknown_id_returns_false() {
        let mut pool = make_pool(1);
        assert!(!pool.dismiss_session(Uuid::new_v4()));
    }

    #[test]
    fn dismiss_all_completed_clears_terminal_sessions() {
        use crate::session::transition::TransitionReason;

        let mut pool = make_pool(2);
        let s1 = make_session("A");
        let s2 = make_session("B");
        let id1 = s1.id;
        let id2 = s2.id;
        pool.enqueue(s1);
        pool.enqueue(s2);
        pool.try_promote();

        // Transition through valid state machine: Queued → Spawning → Running → Completed
        for id in [id1, id2] {
            if let Some(m) = pool.get_active_mut(id) {
                let _ = m
                    .session
                    .transition_to(SessionStatus::Spawning, TransitionReason::Spawned);
                let _ = m
                    .session
                    .transition_to(SessionStatus::Running, TransitionReason::Promoted);
                let _ = m
                    .session
                    .transition_to(SessionStatus::Completed, TransitionReason::StreamCompleted);
            }
            pool.finalize_and_teardown(id);
        }
        assert_eq!(pool.total_count(), 2);

        let dismissed = pool.dismiss_all_completed();
        assert_eq!(dismissed, 2);
        assert_eq!(pool.total_count(), 0);
    }

    // --- Issue #202: tick_flash_counters ---

    #[test]
    fn tick_flash_counters_decrements_nonzero() {
        let mut pool = make_pool(2);
        let session = make_session("flash");
        let id = session.id;
        pool.enqueue(session);
        pool.try_promote();
        must_get_active_mut(&mut pool, id)
            .session
            .transition_flash_remaining = 3;
        pool.tick_flash_counters();
        assert_eq!(must_get_session(&pool, id).transition_flash_remaining, 2);
    }

    #[test]
    fn tick_flash_counters_does_not_go_below_zero() {
        let mut pool = make_pool(2);
        let session = make_session("zero");
        let id = session.id;
        pool.enqueue(session);
        pool.try_promote();
        assert_eq!(must_get_session(&pool, id).transition_flash_remaining, 0);
        pool.tick_flash_counters();
        assert_eq!(must_get_session(&pool, id).transition_flash_remaining, 0);
    }

    #[test]
    fn tick_flash_counters_decrements_all_sessions() {
        let mut pool = make_pool(3);
        let s1 = make_session("A");
        let s2 = make_session("B");
        let id1 = s1.id;
        let id2 = s2.id;
        pool.enqueue(s1);
        pool.enqueue(s2);
        pool.try_promote();
        must_get_active_mut(&mut pool, id1)
            .session
            .transition_flash_remaining = 4;
        must_get_active_mut(&mut pool, id2)
            .session
            .transition_flash_remaining = 2;
        pool.tick_flash_counters();
        assert_eq!(must_get_session(&pool, id1).transition_flash_remaining, 3);
        assert_eq!(must_get_session(&pool, id2).transition_flash_remaining, 1);
    }

    // --- Issue #344: TurboQuant system-prompt compaction integration ---

    #[test]
    fn pool_promote_without_adapter_joins_components_plainly() {
        let mut pool = make_pool(1);
        pool.set_guardrail_prompt("GUARDRAIL: safety rules".into());
        pool.enqueue(make_session("do work"));
        pool.try_promote();
        let managed = &pool.active[0];
        let appendix = managed
            .system_prompt_appendix
            .as_ref()
            .expect("appendix should be set when guardrail configured");
        assert!(appendix.contains("GUARDRAIL: safety rules"));
    }

    #[test]
    fn pool_promote_with_adapter_compacts_appendix() {
        use crate::turboquant::adapter::TurboQuantAdapter;

        let mut pool = make_pool(1);
        pool.set_guardrail_prompt(
            "GUARDRAIL: never modify auth. GUARDRAIL: never modify auth.".into(),
        );
        let adapter = Arc::new(TurboQuantAdapter::new(4));
        pool.set_turboquant_adapter(adapter, 1024);
        pool.enqueue(make_session("work"));
        pool.try_promote();
        let managed = &pool.active[0];
        let appendix = must_get_appendix(managed);
        assert!(appendix.contains("GUARDRAIL"));
    }

    #[test]
    fn pool_promote_with_disabled_adapter_falls_back_to_join() {
        use crate::turboquant::adapter::TurboQuantAdapter;

        let mut pool = make_pool(1);
        pool.set_guardrail_prompt("GUARDRAIL: X".into());
        let mut a = TurboQuantAdapter::new(4);
        a.set_enabled(false);
        pool.set_turboquant_adapter(Arc::new(a), 1024);
        pool.enqueue(make_session("work"));
        pool.try_promote();
        let managed = &pool.active[0];
        let appendix = must_get_appendix(managed);
        assert!(appendix.contains("GUARDRAIL: X"));
    }

    #[test]
    fn tick_flash_counters_decrements_finished_sessions() {
        let mut pool = make_pool(2);
        let s = make_session("done");
        let id = s.id;
        pool.enqueue(s);
        pool.try_promote();
        must_get_active_mut(&mut pool, id)
            .session
            .transition_flash_remaining = 3;
        pool.finalize_and_teardown(id);
        pool.tick_flash_counters();
        assert_eq!(must_get_session(&pool, id).transition_flash_remaining, 2);
    }

    // --- Issue #558: finalize_retain_worktree (gate-failure recovery path) ---

    #[test]
    fn finalize_retain_worktree_moves_session_to_finished() {
        let mut pool = make_pool(2);
        let session = make_session_with_issue("retain", 558);
        let id = session.id;
        pool.enqueue(session);
        pool.try_promote();
        assert_eq!(pool.active_count(), 1);

        pool.finalize_retain_worktree(id);
        assert_eq!(pool.active_count(), 0);
        assert_eq!(pool.total_count(), 1);
    }

    #[test]
    fn finalize_retain_worktree_does_not_call_remove() {
        let mut pool = make_pool(2);
        let session = make_session_with_issue("keep-worktree", 558);
        let id = session.id;
        pool.enqueue(session);
        pool.try_promote();
        assert!(
            pool.worktree_exists("issue-558"),
            "worktree must exist after promotion"
        );

        pool.finalize_retain_worktree(id);

        assert!(
            pool.worktree_exists("issue-558"),
            "worktree must NOT have been removed after finalize_retain_worktree"
        );
    }

    #[test]
    fn finalize_retain_worktree_releases_file_claims() {
        let mut pool = make_pool(2);
        let session = make_session_with_issue("claimer", 558);
        let id = session.id;
        pool.enqueue(session);
        pool.try_promote();
        pool.file_claims.claim("src/a.rs", id);
        pool.file_claims.claim("src/b.rs", id);
        assert_eq!(pool.file_claims.total_claims(), 2);

        pool.finalize_retain_worktree(id);
        assert_eq!(pool.file_claims.total_claims(), 0);
    }

    #[test]
    fn finalize_and_teardown_calls_remove() {
        let mut pool = make_pool(2);
        let session = make_session_with_issue("teardown", 558);
        let id = session.id;
        pool.enqueue(session);
        pool.try_promote();
        assert!(pool.worktree_exists("issue-558"));

        pool.finalize_and_teardown(id);

        assert!(
            !pool.worktree_exists("issue-558"),
            "worktree MUST be removed after finalize_and_teardown"
        );
    }

    #[test]
    fn finalize_retain_worktree_idempotent_when_called_twice() {
        let mut pool = make_pool(2);
        let session = make_session_with_issue("idempotent", 558);
        let id = session.id;
        pool.enqueue(session);
        pool.try_promote();

        pool.finalize_retain_worktree(id);
        pool.finalize_retain_worktree(id);

        assert_eq!(
            pool.total_count(),
            1,
            "second call must not duplicate or panic"
        );
    }

    #[test]
    fn worktree_exists_returns_false_for_unknown_slug() {
        let pool = make_pool(2);
        assert!(!pool.worktree_exists("issue-99999"));
    }

    // --- Issue #707: HTTP-provider rendered-template injection ---

    use crate::agent_provider::test_fakes::{FakeClaudeProvider, FakeHttpProvider};
    use crate::session::types::SessionOrigin;
    use crate::templates::FakeRenderedStore;

    #[test]
    fn pool_injects_template_when_http_provider_and_command_set() {
        let mut pool = make_pool(1);
        let store = FakeRenderedStore::new().with("qwen", "implement", "# Template body");
        pool.set_rendered_template_store(Arc::new(store));
        pool.set_provider(Arc::new(FakeHttpProvider));
        pool.enqueue(make_session("work").with_active_command(Some("implement".into())));
        pool.try_promote();
        let managed = &pool.active[0];
        let appendix = must_get_appendix(managed);
        assert!(
            appendix.contains("# Template body"),
            "appendix missing template body: {appendix}"
        );
    }

    #[test]
    fn pool_does_not_inject_template_for_claude_provider_with_target_dir() {
        let mut pool = make_pool(1);
        let store = FakeRenderedStore::new().with("claude", "implement", "CLAUDE_BODY");
        pool.set_rendered_template_store(Arc::new(store));
        pool.set_provider(Arc::new(FakeClaudeProvider));
        pool.enqueue(make_session("work").with_active_command(Some("implement".into())));
        pool.try_promote();
        let managed = &pool.active[0];
        let appendix_opt = managed.system_prompt_appendix.as_deref().unwrap_or("");
        assert!(
            !appendix_opt.contains("CLAUDE_BODY"),
            "Claude provider must not get rendered template injection"
        );
    }

    #[test]
    fn pool_does_not_inject_template_when_no_active_command() {
        let mut pool = make_pool(1);
        let store = FakeRenderedStore::new().with("qwen", "implement", "BODY");
        pool.set_rendered_template_store(Arc::new(store));
        pool.set_provider(Arc::new(FakeHttpProvider));
        pool.enqueue(make_session("work"));
        pool.try_promote();
        let managed = &pool.active[0];
        assert!(managed.system_prompt_appendix.is_none());
    }

    #[test]
    fn pool_skips_injection_when_origin_is_orchestrator_l1() {
        let mut pool = make_pool(1);
        let store = FakeRenderedStore::new().with("qwen", "implement", "L1_FORBIDDEN");
        pool.set_rendered_template_store(Arc::new(store));
        pool.set_provider(Arc::new(FakeHttpProvider));
        pool.enqueue(
            make_session("work")
                .with_active_command(Some("implement".into()))
                .with_origin(SessionOrigin::OrchestratorL1),
        );
        pool.try_promote();
        let managed = &pool.active[0];
        let appendix_opt = managed.system_prompt_appendix.as_deref().unwrap_or("");
        assert!(!appendix_opt.contains("L1_FORBIDDEN"));
    }

    #[test]
    fn pool_skips_injection_when_origin_is_orchestrator_l2() {
        let mut pool = make_pool(1);
        let store = FakeRenderedStore::new().with("qwen", "implement", "L2_FORBIDDEN");
        pool.set_rendered_template_store(Arc::new(store));
        pool.set_provider(Arc::new(FakeHttpProvider));
        pool.enqueue(
            make_session("work")
                .with_active_command(Some("implement".into()))
                .with_origin(SessionOrigin::OrchestratorL2),
        );
        pool.try_promote();
        let managed = &pool.active[0];
        let appendix_opt = managed.system_prompt_appendix.as_deref().unwrap_or("");
        assert!(!appendix_opt.contains("L2_FORBIDDEN"));
    }

    #[test]
    fn pool_does_not_inject_when_store_is_absent() {
        let mut pool = make_pool(1);
        pool.set_provider(Arc::new(FakeHttpProvider));
        pool.enqueue(make_session("work").with_active_command(Some("implement".into())));
        pool.try_promote();
        let managed = &pool.active[0];
        assert!(managed.system_prompt_appendix.is_none());
    }

    #[test]
    fn pool_does_not_inject_when_store_returns_none() {
        let mut pool = make_pool(1);
        let store = FakeRenderedStore::new();
        pool.set_rendered_template_store(Arc::new(store));
        pool.set_provider(Arc::new(FakeHttpProvider));
        pool.enqueue(make_session("work").with_active_command(Some("implement".into())));
        pool.try_promote();
        let managed = &pool.active[0];
        assert!(managed.system_prompt_appendix.is_none());
    }

    #[test]
    fn pool_injected_template_appears_before_turboquant_compaction() {
        use crate::turboquant::adapter::TurboQuantAdapter;

        let mut pool = make_pool(1);
        let store = FakeRenderedStore::new().with(
            "qwen",
            "implement",
            "# Implement Command\n\nRender step-by-step instructions for issue tasks.",
        );
        pool.set_rendered_template_store(Arc::new(store));
        pool.set_provider(Arc::new(FakeHttpProvider));
        pool.set_guardrail_prompt(
            "Guardrail: never modify auth code without explicit approval.".into(),
        );
        pool.set_turboquant_adapter(Arc::new(TurboQuantAdapter::new(4)), 4096);
        pool.enqueue(make_session("work").with_active_command(Some("implement".into())));
        pool.try_promote();
        let managed = &pool.active[0];
        let appendix = must_get_appendix(managed);
        assert!(
            appendix.contains("Implement Command"),
            "template body must survive TurboQuant compaction: {appendix}"
        );
    }

    // --- Issue #739/#948: interactive_managed_mut ---

    #[test]
    fn interactive_managed_mut_returns_mutable_ref_for_live() {
        let mut pool = make_pool(2);
        create_unified(&mut pool, 42, false);

        let found = pool.interactive_managed_mut(42);
        assert!(found.is_some());
        assert!(
            found
                .unwrap()
                .session
                .bind_agent_session_id("mutated-id-42")
        );

        assert_eq!(
            pool.interactive_managed(42)
                .unwrap()
                .session
                .agent_session_id
                .as_deref(),
            Some("mutated-id-42"),
            "mutation through the mutable ref must persist"
        );
    }

    #[test]
    fn interactive_managed_mut_returns_none_when_absent() {
        let mut pool = make_pool(2);
        assert!(pool.interactive_managed_mut(99).is_none());
    }

    #[test]
    fn interactive_managed_mut_skips_terminated() {
        use crate::session::transition::TransitionReason;
        let mut pool = make_pool(2);
        let id = create_unified(&mut pool, 42, false);
        must_get_active_mut(&mut pool, id)
            .session
            .transition_to(SessionStatus::Killed, TransitionReason::UserKill)
            .unwrap();

        assert!(
            pool.interactive_managed_mut(42).is_none(),
            "terminated session must not be returned by _mut lookup"
        );
    }

    // --- interaction_appendix (#946) ---
    // Subset of try_promote's appendix: mode.system_prompt + guardrail +
    // knowledge, joined with "\n\n". No file-claims (no live session at
    // interaction launch). None when every part is empty.

    fn mode_with_prompt(prompt: &str) -> crate::session::types::SessionModeConfig {
        crate::session::types::SessionModeConfig {
            system_prompt: prompt.to_string(),
            allowed_tools: Vec::new(),
            permission_mode: None,
        }
    }

    #[test]
    fn interaction_appendix_none_when_all_empty() {
        let pool = make_pool(1);
        assert!(pool.interaction_appendix(None).is_none());
    }

    #[test]
    fn interaction_appendix_includes_mode_system_prompt() {
        let pool = make_pool(1);
        let mode = mode_with_prompt("Custom mode prompt");
        let r = pool
            .interaction_appendix(Some(&mode))
            .expect("mode prompt yields Some");
        assert!(r.contains("Custom mode prompt"));
    }

    #[test]
    fn interaction_appendix_includes_guardrail_prompt() {
        let mut pool = make_pool(1);
        pool.set_guardrail_prompt("GUARDRAIL: no unsafe".into());
        let r = pool
            .interaction_appendix(None)
            .expect("guardrail yields Some");
        assert!(r.contains("GUARDRAIL: no unsafe"));
    }

    #[test]
    fn interaction_appendix_includes_knowledge_appendix() {
        let mut pool = make_pool(1);
        pool.set_knowledge_appendix(Some("KNOWLEDGE: extra".into()));
        let r = pool
            .interaction_appendix(None)
            .expect("knowledge yields Some");
        assert!(r.contains("KNOWLEDGE: extra"));
    }

    #[test]
    fn interaction_appendix_joins_parts_with_double_newline() {
        let mut pool = make_pool(1);
        pool.set_guardrail_prompt("PART_B".into());
        pool.set_knowledge_appendix(Some("PART_C".into()));
        let mode = mode_with_prompt("PART_A");
        let r = pool.interaction_appendix(Some(&mode)).unwrap();
        assert!(r.contains("PART_A\n\nPART_B"));
        assert!(r.contains("PART_B\n\nPART_C"));
    }

    #[test]
    fn interaction_appendix_skips_blank_mode_prompt() {
        let mut pool = make_pool(1);
        pool.set_guardrail_prompt("GUARDRAIL_ONLY".into());
        let mode = mode_with_prompt("   ");
        let r = pool.interaction_appendix(Some(&mode)).unwrap();
        assert!(r.contains("GUARDRAIL_ONLY"));
        assert!(!r.starts_with("   "), "blank mode prompt not injected");
    }
}
