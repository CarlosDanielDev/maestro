use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConcurrencyConfig {
    /// Labels that mark a task as "heavy" (resource-intensive).
    #[serde(default)]
    pub heavy_task_labels: Vec<String>,
    /// Maximum number of heavy tasks that can run concurrently.
    #[serde(default = "default_heavy_task_limit")]
    pub heavy_task_limit: usize,
}

impl Default for ConcurrencyConfig {
    fn default() -> Self {
        Self {
            heavy_task_labels: Vec::new(),
            heavy_task_limit: default_heavy_task_limit(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Interval in seconds for work assigner ticks. Default: 10.
    #[serde(default = "default_work_tick_interval")]
    pub work_tick_interval_secs: u64,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            work_tick_interval_secs: default_work_tick_interval(),
        }
    }
}

fn default_heavy_task_limit() -> usize {
    2
}
fn default_work_tick_interval() -> u64 {
    10
}
