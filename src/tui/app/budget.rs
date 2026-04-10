use super::App;
use super::helpers::session_label;
use super::types::PendingHook;
use crate::budget::{BudgetAction, BudgetCheck};
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::session::types::SessionStatus;
use crate::tui::activity_log::LogLevel;

impl App {
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
                    managed.session.status = SessionStatus::Errored;
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
