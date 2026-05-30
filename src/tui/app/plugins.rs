use super::App;
use crate::plugins::hooks::{HookContext, HookPoint};
use crate::session::manager::SessionEvent;
use crate::session::types::StreamEvent;
use crate::tui::activity_log::LogLevel;

impl App {
    pub async fn fire_plugin_hook(&mut self, hook: HookPoint, ctx: HookContext) {
        let Some(ref runner) = self.plugin_runner else {
            return;
        };
        let results = runner.fire(hook, &ctx).await;
        // The owning session id (if any) was placed on the context via
        // `HookContext::with_session`; reuse it to route a HookResponse event
        // through the normal session pipeline so the per-agent call log shows
        // hook activity (#887).
        let session_id = ctx
            .vars
            .get("MAESTRO_SESSION_ID")
            .and_then(|s| uuid::Uuid::parse_str(s).ok());
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

            if let Some(session_id) = session_id {
                self.handle_session_event(SessionEvent {
                    session_id,
                    event: StreamEvent::HookResponse {
                        hook_name: result.plugin_name,
                        exit_code: result.exit_code,
                        stdout: result.stdout,
                        stderr: result.stderr,
                    },
                });
            }
        }
    }
}
