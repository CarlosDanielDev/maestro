use crate::session::types::Session;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MaestroState {
    pub sessions: Vec<Session>,
    pub total_cost_usd: f64,
    pub file_claims: HashMap<String, uuid::Uuid>,
    pub last_updated: Option<DateTime<Utc>>,
}

impl MaestroState {
    pub fn active_sessions(&self) -> Vec<&Session> {
        self.sessions
            .iter()
            .filter(|s| !s.status.is_terminal())
            .collect()
    }

    pub fn update_total_cost(&mut self) {
        self.total_cost_usd = self.sessions.iter().map(|s| s.cost_usd).sum();
    }
}
