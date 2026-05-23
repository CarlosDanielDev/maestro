use super::App;
use super::helpers::session_label;
use super::types::PendingHook;
use crate::budget::{BudgetAction, BudgetCheck, PreSpawnDecision};
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus};
use crate::tui::activity_log::LogLevel;

impl App {
    /// Compute the pre-spawn budget decision for a session about to be
    /// spawned. Returns `Allow` when either the enforcer or projector is
    /// missing — the gate is a no-op until production wires both via
    /// config + builder (#776/#850).
    /// Park the session under `id` in the pre-spawn modal if the gate
    /// returns Warn/Block. Returns `true` when parked (caller MUST break
    /// out of the spawn loop). Used by both `session_lifecycle.rs` and
    /// `completion_pipeline.rs` so the gate logic stays in one place.
    pub(super) fn try_enter_prespawn_gate(&mut self, id: uuid::Uuid) -> bool {
        use crate::tui::app::types::TuiMode;
        let decision = self
            .pool
            .get_session(id)
            .map(|session| self.pre_spawn_decision(session))
            .unwrap_or(PreSpawnDecision::Allow);
        if matches!(decision, PreSpawnDecision::Allow) {
            return false;
        }
        let label = self
            .pool
            .get_session(id)
            .map(session_label)
            .unwrap_or_else(|| format!("S-{}", &id.to_string()[..8]));
        self.activity_log.push_simple(
            label,
            "Pre-spawn budget gate fired — awaiting confirmation".into(),
            LogLevel::Warn,
        );
        self.tui_mode = TuiMode::BudgetPreSpawn { session_id: id };
        true
    }

    pub fn pre_spawn_decision(&self, session: &Session) -> PreSpawnDecision {
        if self.budget_skip_once.contains(&session.id) {
            return PreSpawnDecision::Allow;
        }
        let Some(ref enforcer) = self.budget_enforcer else {
            return PreSpawnDecision::Allow;
        };
        let Some(ref projector) = self.budget_projector else {
            return PreSpawnDecision::Allow;
        };
        let projected = projector.projected_turn_cost(session);
        enforcer.check_pre_spawn(self.total_cost, projected)
    }

    /// Resolve the pre-spawn modal in response to a single-letter chord
    /// (#776/#850). `y`/`s` mark the session as gate-bypass for the next
    /// spawn attempt and resume from `BudgetPreSpawn`; `n` removes the
    /// parked session from the pool. Other chords are ignored.
    pub async fn resolve_budget_prespawn(&mut self, chord: char, session_id: uuid::Uuid) {
        use crate::tui::app::types::TuiMode;
        match chord {
            'y' | 's' => {
                self.budget_skip_once.insert(session_id);
                self.tui_mode = TuiMode::Overview;
                let tx = self.pool.event_tx();
                if let Some(managed) = self.pool.get_active_mut(session_id) {
                    let label = session_label(&managed.session);
                    self.activity_log.push_simple(
                        label.clone(),
                        "Spawning session (post budget gate)...".into(),
                        LogLevel::Info,
                    );
                    if let Err(e) = managed.spawn(tx).await {
                        self.activity_log.push_simple(
                            label,
                            format!("Spawn failed: {}", e),
                            LogLevel::Error,
                        );
                    }
                }
            }
            'n' => {
                self.tui_mode = TuiMode::Overview;
                let label = self
                    .pool
                    .get_session(session_id)
                    .map(session_label)
                    .unwrap_or_else(|| format!("S-{}", &session_id.to_string()[..8]));
                let _ = self.pool.kill_session(session_id).await;
                self.activity_log.push_simple(
                    label,
                    "Spawn cancelled by budget gate".into(),
                    LogLevel::Warn,
                );
            }
            _ => {}
        }
    }

    pub(super) fn check_budget(&mut self, session_id: uuid::Uuid) {
        let Some(ref mut enforcer) = self.budget_enforcer else {
            return;
        };

        let session_cost = self
            .pool
            .get_active_mut(session_id)
            .map(|m| m.session.cost_usd)
            .unwrap_or(0.0);

        match enforcer.check_session(session_cost) {
            BudgetAction::Kill => {
                if let Some(managed) = self.pool.get_active_mut(session_id) {
                    let _ = managed
                        .session
                        .transition_to(SessionStatus::Errored, TransitionReason::StreamError);
                    let label = session_label(&managed.session);
                    self.activity_log.push_simple(
                        label,
                        format!(
                            "BUDGET EXCEEDED: ${:.2}/${:.2} per-session limit",
                            session_cost,
                            enforcer.per_session_limit()
                        ),
                        LogLevel::Error,
                    );
                }
            }
            BudgetAction::Alert(pct) => {
                if enforcer.record_alert(session_id)
                    && let Some(managed) = self.pool.get_active_mut(session_id)
                {
                    let label = session_label(&managed.session);
                    self.activity_log.push_simple(
                        label,
                        format!("Budget warning: {}% of per-session limit used", pct),
                        LogLevel::Warn,
                    );
                }
            }
            BudgetAction::Ok => {}
        }

        match enforcer.check_global(self.total_cost) {
            BudgetAction::Kill => {
                self.activity_log.push_simple(
                    "MAESTRO".into(),
                    format!(
                        "GLOBAL BUDGET EXCEEDED: ${:.2}/${:.2} — stopping all sessions",
                        self.total_cost,
                        enforcer.total_limit()
                    ),
                    LogLevel::Error,
                );
                self.pending_hooks.push(PendingHook {
                    hook: HookPoint::BudgetThreshold,
                    ctx: HookContext::new()
                        .with_cost(self.total_cost)
                        .with_var("MAESTRO_BUDGET_EXCEEDED", "true"),
                });
                self.running = false;
            }
            BudgetAction::Alert(pct) => {
                if !enforcer.global_alert_sent() {
                    enforcer.mark_global_alert_sent();
                    self.activity_log.push_simple(
                        "MAESTRO".into(),
                        format!("Global budget warning: {}% used", pct),
                        LogLevel::Warn,
                    );
                }
            }
            BudgetAction::Ok => {}
        }
    }
}
