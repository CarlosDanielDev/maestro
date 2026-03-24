use super::hooks::{HookContext, HookPoint};
use crate::config::PluginConfig;
use std::time::Duration;
use tokio::process::Command;

/// Result of a plugin execution.
#[derive(Debug, Clone)]
pub struct PluginResult {
    pub plugin_name: String,
    pub success: bool,
    pub output: String,
    pub duration_ms: u64,
}

/// Executes plugins registered for specific hook points.
pub struct PluginRunner {
    plugins: Vec<PluginConfig>,
    timeout: Duration,
}

impl PluginRunner {
    pub fn new(plugins: Vec<PluginConfig>, timeout_secs: u64) -> Self {
        Self {
            plugins,
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    /// Get all plugins registered for a specific hook point.
    pub fn plugins_for(&self, hook: HookPoint) -> Vec<&PluginConfig> {
        self.plugins
            .iter()
            .filter(|p| p.on == hook.as_str())
            .collect()
    }

    /// Execute all plugins for a hook point with the given context.
    /// Returns results for each plugin. Plugins run sequentially.
    pub async fn fire(&self, hook: HookPoint, ctx: &HookContext) -> Vec<PluginResult> {
        let matching = self.plugins_for(hook);
        let mut results = Vec::new();

        for plugin in matching {
            let result = self.execute_plugin(plugin, ctx).await;
            results.push(result);
        }

        results
    }

    async fn execute_plugin(&self, plugin: &PluginConfig, ctx: &HookContext) -> PluginResult {
        let start = std::time::Instant::now();

        let result = tokio::time::timeout(self.timeout, async {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", &plugin.run]);

            // Inject environment variables from context
            for (key, value) in &ctx.vars {
                cmd.env(key, value);
            }

            // Add hook point as env var
            cmd.env("MAESTRO_HOOK", plugin.on.as_str());
            cmd.env("MAESTRO_PLUGIN_NAME", &plugin.name);

            cmd.output().await
        })
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = if stderr.is_empty() {
                    stdout.to_string()
                } else {
                    format!("{}\n{}", stdout.trim(), stderr.trim())
                };
                PluginResult {
                    plugin_name: plugin.name.clone(),
                    success: output.status.success(),
                    output: combined,
                    duration_ms,
                }
            }
            Ok(Err(e)) => PluginResult {
                plugin_name: plugin.name.clone(),
                success: false,
                output: format!("Failed to execute: {}", e),
                duration_ms,
            },
            Err(_) => PluginResult {
                plugin_name: plugin.name.clone(),
                success: false,
                output: format!("Plugin timed out after {}s", self.timeout.as_secs()),
                duration_ms,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_plugin(name: &str, on: &str, run: &str) -> PluginConfig {
        PluginConfig {
            name: name.into(),
            on: on.into(),
            run: run.into(),
            timeout_secs: None,
        }
    }

    #[test]
    fn plugins_for_filters_by_hook() {
        let plugins = vec![
            make_plugin("a", "session_completed", "echo done"),
            make_plugin("b", "session_started", "echo start"),
            make_plugin("c", "session_completed", "echo also done"),
        ];
        let runner = PluginRunner::new(plugins, 30);
        let matching = runner.plugins_for(HookPoint::SessionCompleted);
        assert_eq!(matching.len(), 2);
        assert_eq!(matching[0].name, "a");
        assert_eq!(matching[1].name, "c");
    }

    #[test]
    fn plugins_for_returns_empty_when_no_match() {
        let plugins = vec![make_plugin("a", "session_started", "echo start")];
        let runner = PluginRunner::new(plugins, 30);
        let matching = runner.plugins_for(HookPoint::PrCreated);
        assert!(matching.is_empty());
    }

    #[tokio::test]
    async fn fire_executes_matching_plugins() {
        let plugins = vec![make_plugin("echo-test", "session_completed", "echo hello")];
        let runner = PluginRunner::new(plugins, 5);
        let ctx = HookContext::new();
        let results = runner.fire(HookPoint::SessionCompleted, &ctx).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(results[0].output.contains("hello"));
    }

    #[tokio::test]
    async fn fire_passes_env_vars() {
        let plugins = vec![make_plugin(
            "env-test",
            "session_started",
            "echo $MAESTRO_SESSION_ID",
        )];
        let runner = PluginRunner::new(plugins, 5);
        let ctx = HookContext::new().with_session("test-id-123", None);
        let results = runner.fire(HookPoint::SessionStarted, &ctx).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(results[0].output.contains("test-id-123"));
    }

    #[tokio::test]
    async fn fire_handles_timeout() {
        let plugins = vec![make_plugin("slow", "session_completed", "sleep 10")];
        let runner = PluginRunner::new(plugins, 1);
        let ctx = HookContext::new();
        let results = runner.fire(HookPoint::SessionCompleted, &ctx).await;
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0].output.contains("timed out"));
    }

    #[tokio::test]
    async fn fire_handles_failed_command() {
        let plugins = vec![make_plugin("fail", "session_completed", "exit 1")];
        let runner = PluginRunner::new(plugins, 5);
        let ctx = HookContext::new();
        let results = runner.fire(HookPoint::SessionCompleted, &ctx).await;
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
    }
}
