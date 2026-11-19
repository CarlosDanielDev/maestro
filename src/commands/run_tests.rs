//! #1411: `maestro run` builds its App for the agent it resolved, so the
//! pool mode and the ▲ BYPASS chip follow that agent, not `agents.default`.

use super::setup_run_app;
use crate::config::{Config, LoadedConfig};
use crate::session::worktree::MockWorktreeManager;
use crate::state::store::StateStore;

const BODY: &str = r#"[project]
repo = "owner/repo"
base_branch = "main"
[sessions]
permission_mode = "default"
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[notifications]
[agents]
default = "claude"
[agents.claude]
kind = "claude"
command = "claude"
permission_mode = "default"
[agents.worker]
kind = "claude"
command = "claude"
permission_mode = "bypassPermissions"
"#;

#[test]
fn run_app_follows_the_run_agent_not_the_default() {
    let config: Config = toml::from_str(BODY).expect("config parses");
    let dir = tempfile::tempdir().expect("tempdir");
    for (agent, bypass) in [("worker", true), ("claude", false)] {
        let resolved = config.resolve_agent(Some(agent)).expect("agent resolves");
        let app = setup_run_app(
            LoadedConfig {
                config: config.clone(),
                path: dir.path().join("maestro.toml"),
            },
            StateStore::new(dir.path().join(format!("state-{agent}-1411.json"))),
            Box::new(MockWorktreeManager::new()),
            None,
            false,
            &resolved,
            true,
        );
        let pool_bypass = app.pool.permission_mode() == "bypassPermissions";
        assert_eq!(pool_bypass, bypass, "agent {agent}");
        assert_eq!(app.bypass_active, bypass, "chip for agent {agent}");
        assert_eq!(app.selected_agent_id, agent);
    }
}
