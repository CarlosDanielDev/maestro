use crate::config::Config;
use crate::models::ModelRouter;
use crate::prompts::PromptBuilder;
use crate::work::assigner::WorkAssigner;

/// A fully prepared session assignment ready for execution.
pub struct SessionAssignment {
    pub issue_number: u64,
    pub title: String,
    pub prompt: String,
    pub model: String,
    pub mode: String,
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
            let mode = item
                .mode
                .map(|m| m.as_config_str().to_string())
                .unwrap_or_else(|| ctx.config.sessions.default_mode.clone());
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
            );
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
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::models::ModelRouter;
    use crate::provider::github::types::GhIssue;
    use crate::work::assigner::WorkAssigner;
    use crate::work::types::{WorkItem, WorkStatus};
    use std::collections::HashMap;

    fn make_config() -> Config {
        toml::from_str(
            r#"
            [project]
            repo = "owner/repo"
            base_branch = "main"
            [sessions]
            [budget]
            [github]
            [notifications]
            "#,
        )
        .unwrap()
    }

    fn make_config_with_heavy(labels: &[&str], limit: usize) -> Config {
        let labels_toml = labels
            .iter()
            .map(|l| format!(r#""{}""#, l))
            .collect::<Vec<_>>()
            .join(", ");
        toml::from_str(&format!(
            r#"
            [project]
            repo = "owner/repo"
            base_branch = "main"
            [sessions]
            [budget]
            [github]
            [notifications]
            [concurrency]
            heavy_task_labels = [{}]
            heavy_task_limit = {}
            "#,
            labels_toml, limit
        ))
        .unwrap()
    }

    fn make_work_item(number: u64, labels: &[&str]) -> WorkItem {
        WorkItem::from_issue(GhIssue {
            number,
            title: format!("Issue #{}", number),
            body: String::new(),
            labels: labels.iter().map(|s| s.to_string()).collect(),
            state: "open".to_string(),
            html_url: String::new(),
            milestone: None,
            assignees: vec![],
        })
    }

    #[test]
    fn assign_work_returns_empty_when_no_slots_available() {
        let items = vec![make_work_item(1, &[])];
        let assigner = WorkAssigner::new(items);
        let mut service = WorkAssignmentService::new(assigner);
        let config = make_config();
        let ctx = AssignmentContext {
            available_slots: 0,
            config: &config,
            model_router: None,
        };

        let assignments = service.assign_work(&ctx);

        assert!(assignments.is_empty());
        assert_eq!(
            service.inner().all_items()[0].status,
            WorkStatus::Pending,
            "mark_in_progress must not be called when slots == 0"
        );
    }

    #[test]
    fn assign_work_returns_assignment_when_slot_available() {
        let items = vec![make_work_item(42, &[])];
        let assigner = WorkAssigner::new(items);
        let mut service = WorkAssignmentService::new(assigner);
        let config = make_config();
        let ctx = AssignmentContext {
            available_slots: 1,
            config: &config,
            model_router: None,
        };

        let assignments = service.assign_work(&ctx);

        assert_eq!(assignments.len(), 1);
        let a = &assignments[0];
        assert_eq!(a.issue_number, 42);
        assert_eq!(a.title, "Issue #42");
        assert!(!a.prompt.is_empty(), "prompt must not be empty");
        assert_eq!(a.model, config.sessions.default_model);
        assert_eq!(a.mode, config.sessions.default_mode);
        assert_eq!(
            service.inner().all_items()[0].status,
            WorkStatus::InProgress
        );
    }

    #[test]
    fn assign_work_does_not_exceed_heavy_task_limit() {
        let items = vec![
            make_work_item(10, &["heavy", "priority:P0"]),
            make_work_item(11, &["heavy", "priority:P0"]),
        ];
        let assigner = WorkAssigner::new(items);
        let mut service = WorkAssignmentService::new(assigner);
        let config = make_config_with_heavy(&["heavy"], 1);
        let ctx = AssignmentContext {
            available_slots: 2,
            config: &config,
            model_router: None,
        };

        let assignments = service.assign_work(&ctx);

        assert_eq!(assignments.len(), 1, "heavy limit of 1 must be respected");
        assert_eq!(assignments[0].issue_number, 10);
        let item_11 = service
            .inner()
            .all_items()
            .iter()
            .find(|i| i.number() == 11)
            .unwrap();
        assert_eq!(item_11.status, WorkStatus::Pending);
    }

    #[test]
    fn assign_work_uses_model_router_when_present() {
        let items = vec![make_work_item(5, &["priority:P0"])];
        let assigner = WorkAssigner::new(items);
        let mut service = WorkAssignmentService::new(assigner);
        let config = make_config();
        let mut rules = HashMap::new();
        rules.insert("priority:P0".to_string(), "claude-opus-4".to_string());
        let router = ModelRouter::new(rules, config.sessions.default_model.clone());
        let ctx = AssignmentContext {
            available_slots: 1,
            config: &config,
            model_router: Some(&router),
        };

        let assignments = service.assign_work(&ctx);

        assert_eq!(assignments.len(), 1);
        assert_eq!(
            assignments[0].model, "claude-opus-4",
            "router must override config default when a rule matches"
        );
    }

    #[test]
    fn assign_work_uses_item_mode_over_config_default() {
        let items = vec![make_work_item(7, &["mode:vibe"])];
        let assigner = WorkAssigner::new(items);
        let mut service = WorkAssignmentService::new(assigner);
        let config = make_config();
        let ctx = AssignmentContext {
            available_slots: 1,
            config: &config,
            model_router: None,
        };

        let assignments = service.assign_work(&ctx);

        assert_eq!(assignments.len(), 1);
        assert_eq!(
            assignments[0].mode, "vibe",
            "item mode label must override config default_mode"
        );
    }

    #[test]
    fn assign_work_respects_available_slots_count() {
        let items = vec![
            make_work_item(1, &[]),
            make_work_item(2, &[]),
            make_work_item(3, &[]),
        ];
        let assigner = WorkAssigner::new(items);
        let mut service = WorkAssignmentService::new(assigner);
        let config = make_config();
        let ctx = AssignmentContext {
            available_slots: 1,
            config: &config,
            model_router: None,
        };

        let assignments = service.assign_work(&ctx);

        assert_eq!(assignments.len(), 1);
    }
}
