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
    /// Subprocess exit code; `-1` when killed by signal, timed out, or failed
    /// to spawn (#887).
    pub exit_code: i32,
    /// Raw stdout, kept separately from `output` so the call log can render it
    /// untouched (#887).
    pub stdout: String,
    /// Raw stderr (#887).
    pub stderr: String,
    pub duration_ms: u64,
}

/// Executes plugins registered for specific hook points.
pub struct PluginRunner {
    plugins: Vec<PluginConfig>,
    timeout: Duration,
    /// False when the workspace config is untrusted (#1045): plugin
    /// commands are reported as skipped instead of executed.
    trusted: bool,
}

impl PluginRunner {
    pub fn new(plugins: Vec<PluginConfig>, timeout_secs: u64) -> Self {
        Self::with_trust(plugins, timeout_secs, true)
    }

    /// `trusted = false` makes `fire` skip every plugin run — plugin `run`
    /// strings come from the repo config and must not execute without the
    /// workspace trust gate (#1045).
    pub fn with_trust(plugins: Vec<PluginConfig>, timeout_secs: u64, trusted: bool) -> Self {
        Self {
            plugins,
            timeout: Duration::from_secs(timeout_secs),
            trusted,
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

        // Workspace trust gate (#1045): plugin `run` strings come from the
        // repo config; they are reported as skipped instead of executed
        // while the workspace is untrusted.
        if !self.trusted {
            let message = "skipped: workspace config is not trusted (run `maestro trust`)";
            return PluginResult {
                plugin_name: plugin.name.clone(),
                success: false,
                output: message.to_string(),
                exit_code: -1,
                stdout: String::new(),
                stderr: message.to_string(),
                duration_ms: start.elapsed().as_millis() as u64,
            };
        }

        let result = tokio::time::timeout(self.timeout, async {
            // CWE-78: parse into argv and exec directly — no `sh -c`, so a
            // plugin `run` string can't inject via `;`, `|`, `$(...)`, etc.
            // Residual argv[0] risk: the program name is repo-chosen. When
            // the workspace is untrusted, `execute_plugin` skips before this
            // point (#1045).
            let Some((program, args)) = crate::util::parse_command(&plugin.run) else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "empty or malformed plugin command",
                ));
            };
            let mut cmd = Command::new(&program);
            cmd.args(&args);

            // Defense-in-depth: re-validate env vars at execution time.
            // HookContext::with_var() validates at insertion, but ctx.vars
            // is a public HashMap that can be mutated directly.
            for (key, value) in &ctx.vars {
                if let Err(e) = crate::util::validate_env_var_name(key) {
                    tracing::warn!("Skipping unsafe env var {}: {}", key, e);
                    continue;
                }
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
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let combined = if stderr.is_empty() {
                    stdout.clone()
                } else {
                    format!("{}\n{}", stdout.trim(), stderr.trim())
                };
                PluginResult {
                    plugin_name: plugin.name.clone(),
                    success: output.status.success(),
                    output: combined,
                    // `code()` is `None` when killed by a signal — map to -1.
                    exit_code: output.status.code().unwrap_or(-1),
                    stdout,
                    stderr,
                    duration_ms,
                }
            }
            Ok(Err(e)) => {
                let msg = format!("Failed to execute: {}", e);
                PluginResult {
                    plugin_name: plugin.name.clone(),
                    success: false,
                    output: msg.clone(),
                    exit_code: -1,
                    stdout: String::new(),
                    stderr: msg,
                    duration_ms,
                }
            }
            Err(_) => {
                let msg = format!("Plugin timed out after {}s", self.timeout.as_secs());
                PluginResult {
                    plugin_name: plugin.name.clone(),
                    success: false,
                    output: msg.clone(),
                    exit_code: -1,
                    stdout: String::new(),
                    stderr: msg,
                    duration_ms,
                }
            }
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
        // `printenv` reads the var from the child's environment. Structured
        // exec does not expand `$VAR` in args (no shell) — the env var is still
        // set on the process, so printenv is the correct probe.
        let plugins = vec![make_plugin(
            "env-test",
            "session_started",
            "printenv MAESTRO_SESSION_ID",
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
        // `exit` is a shell builtin; use `false` (a real binary) under structured exec.
        let plugins = vec![make_plugin("fail", "session_completed", "false")];
        let runner = PluginRunner::new(plugins, 5);
        let ctx = HookContext::new();
        let results = runner.fire(HookPoint::SessionCompleted, &ctx).await;
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
    }

    #[tokio::test]
    async fn fire_rejects_non_maestro_env_vars() {
        // `printenv PATH` reads the value the child actually received, so the
        // assertion below verifies the injected PATH=/evil was filtered out.
        let plugins = vec![make_plugin("env-test", "session_started", "printenv PATH")];
        let runner = PluginRunner::new(plugins, 5);
        let mut ctx = HookContext::new();
        // Manually inject a dangerous env var (bypassing with_var validation)
        ctx.vars.insert("PATH".into(), "/evil".into());
        ctx.vars
            .insert("MAESTRO_SESSION_ID".into(), "safe-id".into());
        let results = runner.fire(HookPoint::SessionStarted, &ctx).await;
        assert_eq!(results.len(), 1);
        // The plugin should NOT have received PATH=/evil
        // (it gets the system PATH instead)
        assert!(!results[0].output.contains("/evil"));
    }

    #[test]
    fn hook_context_with_var_rejects_non_maestro() {
        let ctx = HookContext::new().with_var("PATH", "/evil");
        assert!(!ctx.vars.contains_key("PATH"));
    }

    #[test]
    fn hook_context_with_var_accepts_maestro_prefix() {
        let ctx = HookContext::new().with_var("MAESTRO_CUSTOM", "value");
        assert_eq!(ctx.vars.get("MAESTRO_CUSTOM").unwrap(), "value");
    }
}

/// Workspace trust gate tests (#1045) — unix-only: marker probe is a script.
#[cfg(all(test, unix))]
mod workspace_trust_tests {
    use super::*;

    fn marker_workspace(tag: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join(format!("hook-{tag}.sh"));
        let marker = dir.path().join(format!("marker-{tag}.txt"));
        std::fs::write(
            &script,
            format!("#!/bin/sh\ntouch '{}'\necho hooked\n", marker.display()),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        (dir, marker)
    }

    #[tokio::test]
    async fn workspace_trust_untrusted_runner_does_not_execute_plugin() {
        let (dir, marker) = marker_workspace("untrusted");
        let plugin = PluginConfig {
            name: "hook".into(),
            on: "session_started".into(),
            run: dir.path().join("hook-untrusted.sh").display().to_string(),
            timeout_secs: None,
        };
        let runner = PluginRunner::with_trust(vec![plugin], 5, false);
        let ctx = HookContext::new();

        let results = runner.fire(HookPoint::SessionStarted, &ctx).await;

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(
            results[0].output.contains("not trusted"),
            "output: {}",
            results[0].output
        );
        assert!(
            !marker.exists(),
            "untrusted plugin run must not be executed"
        );
    }

    #[tokio::test]
    async fn workspace_trust_trusted_runner_executes_plugin() {
        let (dir, marker) = marker_workspace("trusted");
        let plugin = PluginConfig {
            name: "hook".into(),
            on: "session_started".into(),
            run: dir.path().join("hook-trusted.sh").display().to_string(),
            timeout_secs: None,
        };
        let runner = PluginRunner::with_trust(vec![plugin], 5, true);
        let ctx = HookContext::new();

        let results = runner.fire(HookPoint::SessionStarted, &ctx).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].success, "output: {}", results[0].output);
        assert!(marker.exists(), "trusted plugin run must execute");
    }
}
