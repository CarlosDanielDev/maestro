use crate::config::Config;
use crate::models::ModelRouter;
use crate::prompts::PromptBuilder;
use crate::session::types::SessionModeConfig;
use crate::work::assigner::WorkAssigner;

/// A fully prepared session assignment ready for execution.
pub struct SessionAssignment {
    pub issue_number: u64,
    pub title: String,
    pub prompt: String,
    pub model: String,
    pub mode: String,
    pub mode_config: Option<SessionModeConfig>,
}

/// Inputs the service needs to make assignment decisions.
pub struct AssignmentContext<'a> {
    pub available_slots: usize,
    pub config: &'a Config,
    pub model_router: Option<&'a ModelRouter>,
}

/// Trait for work assignment logic — enables mock injection in tests.
pub trait WorkAssignmentTrait {
    fn assign_work(&mut self, ctx: &AssignmentContext<'_>) -> Vec<SessionAssignment>;
}

/// Production work assignment service wrapping the existing WorkAssigner.
pub struct WorkAssignmentService {
    inner: WorkAssigner,
}

impl WorkAssignmentService {
    pub fn new(inner: WorkAssigner) -> Self {
        Self { inner }
    }

    /// Access inner WorkAssigner for read operations.
    pub fn inner(&self) -> &WorkAssigner {
        &self.inner
    }

    /// Access inner WorkAssigner for write operations.
    pub fn inner_mut(&mut self) -> &mut WorkAssigner {
        &mut self.inner
    }
}

impl WorkAssignmentTrait for WorkAssignmentService {
    fn assign_work(&mut self, ctx: &AssignmentContext<'_>) -> Vec<SessionAssignment> {
        if ctx.available_slots == 0 {
            return Vec::new();
        }

        let heavy_labels = &ctx.config.concurrency.heavy_task_labels;
        let heavy_limit = ctx.config.concurrency.heavy_task_limit;

        let all_ready = self.inner.next_ready(ctx.available_slots);
        let mut assignments = Vec::new();
        let mut heavy_count = 0usize;

        for item in all_ready {
            let is_heavy = !heavy_labels.is_empty()
                && item.issue.labels.iter().any(|l| heavy_labels.contains(l));

            if is_heavy && heavy_count >= heavy_limit {
                continue;
            }

            let prompt = PromptBuilder::build_issue_prompt(&item.issue, ctx.config);
            let mode = crate::modes::mode_from_labels(&item.issue.labels)
                .or_else(|| {
                    item.mode
                        .map(|session_mode| session_mode.as_config_str().to_string())
                })
                .unwrap_or_else(|| ctx.config.sessions.default_mode.clone());
            let mode_config = crate::modes::resolve_session_mode_config(&mode, Some(ctx.config));
            let model = ctx
                .model_router
                .map(|r| r.resolve(&item.issue).to_string())
                .unwrap_or_else(|| ctx.config.sessions.default_model.clone());

            if is_heavy {
                heavy_count += 1;
            }

            assignments.push(SessionAssignment {
                issue_number: item.issue.number,
                title: item.issue.title.clone(),
                prompt,
                model,
                mode,
                mode_config,
            });
        }

        // Mark items in-progress atomically
        for a in &assignments {
            self.inner.mark_in_progress(a.issue_number);
        }

        assignments
    }
}

// --- App delegation (thin wrapper for TUI tick) ---

use super::App;
use crate::provider::github::labels::LabelManager;
use crate::session::types::Session;
use crate::tui::activity_log::LogLevel;

impl App {
    pub async fn tick_work_assigner(&mut self) -> anyhow::Result<()> {
        // In continuous mode, only advance when no issue is running and not paused
        if let Some(ref cont) = self.continuous_mode
            && !cont.can_advance()
        {
            return Ok(());
        }

        let Some(config) = self.config.as_ref() else {
            return Ok(());
        };

        let available_slots = self
            .pool
            .max_concurrent()
            .saturating_sub(self.pool.active_count());

        let ctx = AssignmentContext {
            available_slots,
            config,
            model_router: self.model_router.as_ref(),
        };

        let assignments = {
            let Some(service) = self.work_assignment_service.as_mut() else {
                return Ok(());
            };
            service.assign_work(&ctx)
        };

        for assignment in assignments {
            // Update GitHub labels (non-fatal on error)
            if let Some(client) = &self.github_client {
                if !self.gh_auth_ok {
                    self.log_gh_auth_skip(assignment.issue_number, "label update");
                } else {
                    let label_mgr = LabelManager::new(client.as_ref());
                    if let Err(e) = label_mgr.mark_in_progress(assignment.issue_number).await {
                        self.check_gh_auth_error(&e);
                        self.activity_log.push_simple(
                            format!("#{}", assignment.issue_number),
                            format!("Label update failed: {}", e),
                            LogLevel::Error,
                        );
                    }
                }
            }

            let mut session = Session::new(
                assignment.prompt,
                assignment.model,
                assignment.mode,
                Some(assignment.issue_number),
                None,
            )
            .with_mode_config(assignment.mode_config);
            session.issue_title = Some(assignment.title);

            // Track in continuous mode
            if let Some(ref mut cont) = self.continuous_mode {
                cont.set_current_issue(assignment.issue_number);
                self.activity_log.push_simple(
                    "CONTINUOUS".into(),
                    format!("Advancing to next issue: #{}", assignment.issue_number),
                    LogLevel::Info,
                );
            } else {
                self.activity_log.push_simple(
                    format!("#{}", assignment.issue_number),
                    "Assigned from work queue".into(),
                    LogLevel::Info,
                );
            }

            self.add_session(session).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "work_assigner_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "work_assigner_app_state_tests.rs"]
mod app_state_tests;
