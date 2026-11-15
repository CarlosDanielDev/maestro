use crate::provider::types::{CheckConclusion, CheckRun, CheckStatus, CiStatus};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct PipelineRun {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    definition: Option<PipelineDefinition>,
    #[serde(default, rename = "createdDate")]
    created_date: Option<String>,
    #[serde(default, rename = "queueTime")]
    queue_time: Option<String>,
    #[serde(default, rename = "startTime")]
    start_time: Option<String>,
    #[serde(default, rename = "finishTime")]
    finish_time: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PipelineDefinition {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    name: String,
}

pub(super) fn refs_heads_branch(branch: &str) -> String {
    if branch.starts_with("refs/") {
        branch.to_string()
    } else {
        format!("refs/heads/{branch}")
    }
}

pub(super) fn parse_pipeline_runs_json(json_str: &str) -> Result<Vec<PipelineRun>> {
    if json_str.trim().is_empty() {
        return Ok(Vec::new());
    }
    let value: serde_json::Value = serde_json::from_str(json_str)
        .context("Failed to parse Azure DevOps pipeline runs JSON")?;
    let runs = match value {
        serde_json::Value::Array(_) => serde_json::from_value(value)?,
        serde_json::Value::Object(mut map) => map
            .remove("value")
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    Ok(runs)
}

pub(super) fn parse_check_runs_json(json_str: &str) -> Result<Vec<CheckRun>> {
    Ok(parse_pipeline_runs_json(json_str)?
        .into_iter()
        .map(|run| {
            let classification = classify_run(&run);
            CheckRun {
                name: run.display_name(),
                status: classification.check_status(),
                conclusion: classification.check_conclusion(),
                started_at: run
                    .start_time
                    .as_deref()
                    .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                elapsed_secs: None,
            }
        })
        .collect())
}

pub(super) fn aggregate_pipeline_runs(runs: Vec<PipelineRun>) -> CiStatus {
    if runs.is_empty() {
        return CiStatus::NoneConfigured;
    }

    let mut latest_by_definition: HashMap<String, PipelineRun> = HashMap::new();
    for run in runs {
        let key = run.definition_key();
        match latest_by_definition.get(&key) {
            Some(existing) if !run.is_newer_than(existing) => {}
            _ => {
                latest_by_definition.insert(key, run);
            }
        }
    }

    let mut success = 0usize;
    let mut failed = Vec::new();
    let mut in_progress = 0usize;
    let mut pending = 0usize;

    for run in latest_by_definition.values() {
        match classify_run(run) {
            RunClassification::Success => success += 1,
            RunClassification::Failure => failed.push(run.display_name()),
            RunClassification::InProgress => in_progress += 1,
            RunClassification::Pending => pending += 1,
        }
    }

    if !failed.is_empty() {
        return CiStatus::Failed {
            summary: format!(
                "{} failed, {} passed, {} in progress, {} pending: {}",
                failed.len(),
                success,
                in_progress,
                pending,
                failed.join(", ")
            ),
        };
    }

    if in_progress > 0 || pending > 0 {
        return CiStatus::Pending;
    }

    CiStatus::Passed
}

impl PipelineRun {
    fn definition_key(&self) -> String {
        if let Some(definition) = &self.definition {
            if let Some(id) = definition.id {
                return format!("definition:{id}");
            }
            if !definition.name.is_empty() {
                return format!("definition:{}", definition.name);
            }
        }
        self.id
            .map(|id| format!("run:{id}"))
            .unwrap_or_else(|| format!("run:{}", self.name))
    }

    fn display_name(&self) -> String {
        self.definition
            .as_ref()
            .map(|definition| definition.name.as_str())
            .filter(|name| !name.is_empty())
            .or_else(|| (!self.name.is_empty()).then_some(self.name.as_str()))
            .map(str::to_string)
            .unwrap_or_else(|| {
                self.id
                    .map(|id| format!("pipeline run {id}"))
                    .unwrap_or_else(|| "pipeline run".to_string())
            })
    }

    fn timestamp_key(&self) -> Option<&str> {
        self.created_date
            .as_deref()
            .or(self.queue_time.as_deref())
            .or(self.start_time.as_deref())
            .or(self.finish_time.as_deref())
    }

    fn is_newer_than(&self, other: &Self) -> bool {
        match (self.timestamp_key(), other.timestamp_key()) {
            (Some(left), Some(right)) => left > right,
            (Some(_), None) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunClassification {
    Success,
    Failure,
    InProgress,
    Pending,
}

impl RunClassification {
    fn check_status(self) -> CheckStatus {
        match self {
            Self::Success | Self::Failure => CheckStatus::Completed,
            Self::InProgress => CheckStatus::InProgress,
            Self::Pending => CheckStatus::Pending,
        }
    }

    fn check_conclusion(self) -> CheckConclusion {
        match self {
            Self::Success => CheckConclusion::Success,
            Self::Failure => CheckConclusion::Failure,
            Self::InProgress | Self::Pending => CheckConclusion::None,
        }
    }
}

fn classify_run(run: &PipelineRun) -> RunClassification {
    let status = run.status.as_deref().unwrap_or("").to_ascii_lowercase();
    let result = run.result.as_deref().unwrap_or("").to_ascii_lowercase();

    match result.as_str() {
        "succeeded" | "success" => return RunClassification::Success,
        "failed"
        | "failure"
        | "canceled"
        | "cancelled"
        | "partiallysucceeded"
        | "partially_succeeded"
        | "timedout"
        | "timed_out" => return RunClassification::Failure,
        _ => {}
    }

    match status.as_str() {
        "completed" => RunClassification::Failure,
        "inprogress" | "in_progress" | "cancelling" => RunClassification::InProgress,
        "notstarted" | "not_started" | "postponed" | "queued" | "waiting" | "pending" => {
            RunClassification::Pending
        }
        _ => RunClassification::Pending,
    }
}
