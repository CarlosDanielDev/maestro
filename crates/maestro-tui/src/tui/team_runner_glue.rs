//! TUI ↔ session::team_runner bridge (#881).
//!
//! `RealTeamLauncher` is the production `TeamLauncher` impl used by the
//! `TuiCommand::RunTeam` arm. It owns a copy of the data-event sender
//! and the active `ProviderConfig` so each per-issue spawn can issue
//! the same `gh issue view`-backed fetch path that `LaunchSession`
//! uses today — promotion into the `SessionPool` happens via the
//! existing `TuiDataEvent::Issue` handler.
//!
//! Living in `src/tui/` keeps `src/session/team_runner.rs` free of TUI
//! types so its tests can stay pure async + trait-based.

use crate::config::ProviderConfig;
use crate::provider::create_provider;
use crate::provider::github::client::RepoProvider;
use crate::session::team_runner::TeamLauncher;
use crate::state::types::IssueNumber;
use crate::tui::app;
use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

pub(super) struct RealTeamLauncher {
    pub tx: UnboundedSender<app::TuiDataEvent>,
    pub provider_config: ProviderConfig,
}

#[async_trait]
impl TeamLauncher for RealTeamLauncher {
    async fn spawn_for_issue(&self, issue: IssueNumber, agent_id: String) -> Result<Uuid, String> {
        let client = create_provider(&self.provider_config)
            .map_err(|e| format!("provider init failed: {e}"))?;
        let fetched = client.get_issue(issue).await.map_err(|e| e.to_string())?;
        // Hand the fetched issue back through the same data-event the
        // single-issue launch path uses. The data-handler turns this
        // into a queued session inside the pool.
        let _ = self
            .tx
            .send(app::TuiDataEvent::Issue(Ok(fetched), None, Some(agent_id)));
        // The pool assigns the actual `Session.id` on promotion; the
        // runner's `Uuid` is only used as a per-spawn ack token. Return
        // a fresh nil-equivalent — the value is intentionally unused
        // by the runner's outcome (it only checks Ok vs Err).
        Ok(Uuid::nil())
    }
}
