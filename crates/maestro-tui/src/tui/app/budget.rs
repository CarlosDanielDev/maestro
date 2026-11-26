use super::App;
use super::helpers::session_label;
use super::types::PendingHook;
use crate::budget::projector::{DEFAULT_FLOOR_PCT, DefaultBudgetProjector};
use crate::budget::{BudgetAction, BudgetCheck, PreSpawnDecision};
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::session::transition::TransitionReason;
use crate::session::types::{Session, SessionStatus};
use crate::tui::activity_log::LogLevel;

impl App {
    /// Build the budget enforcer and the pre-spawn projector from one
    /// `[budget]` section (#1243). Called wherever the enforcer is built
    /// (startup and config reload) so the gate and the displayed estimate
    /// always use the same numbers.
    pub fn install_budget(&mut self, budget: &crate::config::BudgetConfig) {
        self.budget_enforcer = Some(crate::budget::BudgetEnforcer::new(
            budget.per_session_usd,
            budget.total_usd,
            budget.alert_threshold_pct,
        ));
        self.budget_projector = Some(Box::new(DefaultBudgetProjector::new(
            budget.per_session_usd,
            DEFAULT_FLOOR_PCT,
        )));
    }

    /// Park the session under `id` in the pre-spawn modal if the gate
    /// returns Warn/Block. Returns `true` when parked (caller MUST break
    /// out of the spawn loop). Used by both `session_lifecycle.rs` and
    /// `completion_pipeline.rs` so the gate logic stays in one place.
    pub(super) fn try_enter_prespawn_gate(&mut self, id: uuid::Uuid) -> bool {
        use crate::tui::app::types::TuiMode;
        // Every spawn loop calls this first, so the ▲ BYPASS chip is on
        // before a bypass session starts (#1411).
        self.note_spawn_bypass(id);
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

    /// Compute the pre-spawn budget decision for a session about to be
    /// spawned. Returns `Allow` when either the enforcer or projector is
    /// missing; production sets both via [`App::install_budget`] (#1243).
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

    /// Resolve the pre-spawn modal in response to a chord from
    /// [`crate::tui::budget_prespawn::chord`] (#776/#850, #1276). `S` marks
    /// the session as gate-bypass for the next spawn attempt and spawns it;
    /// on Block (#1243) `S` does nothing. `b` opens Settings › Budget with
    /// the gate under it, so `Esc` comes back to the gate with the new
    /// cap. `n` removes the parked session from the pool. Other chords are
    /// ignored.
    pub async fn resolve_budget_prespawn(&mut self, chord: char, session_id: uuid::Uuid) {
        use crate::tui::app::types::TuiMode;
        let blocked = self.pool.get_session(session_id).is_some_and(|session| {
            matches!(
                self.pre_spawn_decision(session),
                PreSpawnDecision::Block { .. }
            )
        });
        // #1276: answer only a session still parked in its pool slot. A
        // gate reached again for a session that started or was dropped
        // (e.g. through a stale back entry) closes and says so; it never
        // spawns twice.
        if !self.is_parked_at_gate(session_id) {
            self.tui_mode = TuiMode::Dashboard;
            self.drop_stale_gates_from_nav_stack();
            let toast = crate::tui::ds::toast::Toast::new(
                crate::tui::ds::toast::Severity::Info,
                "That session already started or left the queue: nothing to answer",
            );
            self.push_toast(toast, std::time::Instant::now());
            return;
        }
        match chord {
            'S' if blocked => {}
            'b' => {
                use crate::tui::screens::{ScreenAction, settings::SettingsTab};
                self.budget_gate_return = Some(session_id);
                crate::tui::screen_dispatch::handle_screen_action(
                    self,
                    ScreenAction::Push(TuiMode::Settings),
                );
                if let Some(screen) = self.screen_state.settings_screen.as_mut() {
                    screen.jump_to_tab(SettingsTab::Budget);
                }
            }
            'S' => {
                self.budget_skip_once.insert(session_id);
                self.tui_mode = TuiMode::Dashboard;
                self.note_spawn_bypass(session_id);
                let tx = self.pool.event_tx();
                if let Some(managed) = self.pool.get_active_mut(session_id) {
                    let label = session_label(&managed.session);
                    self.activity_log.push_simple(
                        label.clone(),
                        "Spawning session (post budget gate)...".into(),
                        LogLevel::Info,
                    );
                    if let Err(e) = managed.spawn(tx).await {
                        self.report_spawn_failure(label, &e);
                    }
                }
                self.drop_stale_gates_from_nav_stack();
            }
            'n' => {
                self.tui_mode = TuiMode::Dashboard;
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
                self.drop_stale_gates_from_nav_stack();
            }
            _ => {}
        }
    }

    /// After a key: if `b` left `budget_gate_return` parked and Settings
    /// is no longer open, open its gate again over the current screen, so
    /// the session never sits in a pool slot with nothing to answer it.
    /// A session that started or left the pool meanwhile is forgotten.
    /// Settings' own dialogs (bypass warning), the typed kill confirm and
    /// Confirm Exit (its ⌃Q hold included) wait; `handle_key` also skips
    /// this while a text field or the palette has the keys.
    pub(crate) fn restore_budget_gate(&mut self) {
        use crate::tui::app::types::TuiMode;
        self.drop_stale_gates_from_nav_stack();
        let Some(id) = self.budget_gate_return else {
            return;
        };
        match self.tui_mode {
            TuiMode::BudgetPreSpawn { session_id } if session_id == id => {
                self.budget_gate_return = None;
                return;
            }
            TuiMode::Settings
            | TuiMode::BypassWarning
            | TuiMode::ConfirmKill(_)
            | TuiMode::ConfirmExit
            | TuiMode::BudgetPreSpawn { .. } => return,
            _ => {}
        }
        self.budget_gate_return = None;
        if self.is_parked_at_gate(id) {
            self.tui_mode = TuiMode::BudgetPreSpawn { session_id: id };
        }
    }

    /// True while `id` waits at the gate: still Queued in its pool slot
    /// (promoted, never spawned).
    fn is_parked_at_gate(&mut self, id: uuid::Uuid) -> bool {
        (self.pool.get_active_mut(id)).is_some_and(|m| m.session.status == SessionStatus::Queued)
    }

    /// #1276: `b` pushes the gate on the back stack (so `Esc` in Settings
    /// returns to it). Drop every gate entry whose session is no longer
    /// parked, so no later back step reopens a gate for a session that
    /// started or was dropped.
    fn drop_stale_gates_from_nav_stack(&mut self) {
        use crate::tui::app::types::TuiMode;
        let stack = self.nav_stack.breadcrumbs().to_vec();
        let kept: Vec<TuiMode> = (stack.iter().copied())
            .filter(|m| match *m {
                TuiMode::BudgetPreSpawn { session_id } => self.is_parked_at_gate(session_id),
                _ => true,
            })
            .collect();
        if kept.len() != stack.len() {
            self.nav_stack.clear();
            for mode in kept {
                self.nav_stack.push(mode);
            }
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
                // #1266: the cap pauses running agents and new spawns wait
                // at the pre-spawn gate; maestro keeps running. Later cost
                // events find nothing Running and stay quiet.
                let out = self.pool.pause_all(TransitionReason::BudgetCap);
                // `--once` has no operator to raise the cap: end the run.
                if self.once_mode {
                    self.running = false;
                }
                if out.paused + out.stopped == 0 {
                    return;
                }
                self.activity_log.push_simple(
                    "MAESTRO".into(),
                    format!(
                        "GLOBAL BUDGET REACHED: ${:.2}/${:.2} — {} paused, {} stopped, new spawns wait · raise the cap, then r resumes",
                        self.total_cost,
                        enforcer.total_limit(),
                        out.paused,
                        out.stopped,
                    ),
                    LogLevel::Error,
                );
                self.pending_hooks.push(PendingHook {
                    hook: HookPoint::BudgetThreshold,
                    ctx: HookContext::new()
                        .with_cost(self.total_cost)
                        .with_var("MAESTRO_BUDGET_EXCEEDED", "true"),
                });
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

#[cfg(test)]
#[path = "budget_tests.rs"]
mod tests;
