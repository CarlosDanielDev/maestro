use super::App;
use crate::provider::github::labels::LabelManager;
use crate::prompts::PromptBuilder;
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

        // Collect ready items and mark them in-progress (scoped borrow)
        let ready_items = {
            let Some(assigner) = self.work_assigner.as_mut() else {
                return Ok(());
            };
            let Some(config) = self.config.as_ref() else {
                return Ok(());
            };

            let available_slots = self
                .pool
                .max_concurrent()
                .saturating_sub(self.pool.active_count());
            if available_slots == 0 {
                return Ok(());
            }

            let heavy_labels = &config.concurrency.heavy_task_labels;
            let heavy_limit = config.concurrency.heavy_task_limit;

            // Get all ready items, then filter by heavy task limit
            let all_ready = assigner.next_ready(available_slots);
            let mut items: Vec<(u64, String, String, String, String)> = Vec::new();
            let mut heavy_count_projected = 0usize;

            for item in all_ready {
                let is_heavy = !heavy_labels.is_empty()
                    && item.issue.labels.iter().any(|l| heavy_labels.contains(l));

                if is_heavy && heavy_count_projected >= heavy_limit {
                    // Skip — heavy task limit reached
                    continue;
                }

                let prompt = PromptBuilder::build_issue_prompt(&item.issue, config);
                let mode = item
                    .mode
                    .map(|m| m.as_config_str().to_string())
                    .unwrap_or_else(|| config.sessions.default_mode.clone());
                let model = self
                    .model_router
                    .as_ref()
                    .map(|r| r.resolve(&item.issue).to_string())
                    .unwrap_or_else(|| config.sessions.default_model.clone());

                if is_heavy {
                    heavy_count_projected += 1;
                }
                items.push((
                    item.issue.number,
                    prompt,
                    mode,
                    item.issue.title.clone(),
                    model,
                ));
            }

            // Mark in-progress within this scope
            for (issue_number, _, _, _, _) in &items {
                assigner.mark_in_progress(*issue_number);
            }

            items
        };

        let items = ready_items;

        for (issue_number, prompt, mode, title, model) in items {
            // Update GitHub labels (non-fatal on error)
            if let Some(client) = &self.github_client {
                if !self.gh_auth_ok {
                    self.log_gh_auth_skip(issue_number, "label update");
                } else {
                    let label_mgr = LabelManager::new(client.as_ref());
                    if let Err(e) = label_mgr.mark_in_progress(issue_number).await {
                        self.check_gh_auth_error(&e);
                        self.activity_log.push_simple(
                            format!("#{}", issue_number),
                            format!("Label update failed: {}", e),
                            LogLevel::Error,
                        );
                    }
                }
            }

            let mut session = Session::new(prompt, model, mode, Some(issue_number));
            session.issue_title = Some(title);

            // Track in continuous mode
            if let Some(ref mut cont) = self.continuous_mode {
                cont.set_current_issue(issue_number);
                self.activity_log.push_simple(
                    "CONTINUOUS".into(),
                    format!("Advancing to next issue: #{}", issue_number),
                    LogLevel::Info,
                );
            } else {
                self.activity_log.push_simple(
                    format!("#{}", issue_number),
                    "Assigned from work queue".into(),
                    LogLevel::Info,
                );
            }

            self.add_session(session).await?;
        }

        Ok(())
    }
}
