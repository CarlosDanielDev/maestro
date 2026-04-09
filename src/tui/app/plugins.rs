use super::App;
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::tui::activity_log::LogLevel;

impl App {
    pub async fn fire_plugin_hook(&mut self, hook: HookPoint, ctx: HookContext) {
        let Some(ref runner) = self.plugin_runner else {
            return;
        };
        let results = runner.fire(hook, &ctx).await;
        for result in results {
            let level = if result.success {
                LogLevel::Info
            } else {
                LogLevel::Error
            };
            let msg = if result.success {
                format!(
                    "Plugin '{}' completed ({}ms)",
                    result.plugin_name, result.duration_ms
                )
            } else {
                format!(
                    "Plugin '{}' failed: {}",
                    result.plugin_name,
                    result.output.lines().next().unwrap_or("unknown error")
                )
            };
            self.activity_log.push_simple("PLUGIN".into(), msg, level);
        }
    }
}
