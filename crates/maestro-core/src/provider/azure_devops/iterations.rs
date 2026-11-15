use crate::provider::types::Milestone;
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AzureIteration {
    pub(super) number: u64,
    pub(super) title: String,
    pub(super) path: String,
    pub(super) description: String,
    pub(super) finish_date: Option<NaiveDate>,
}

pub(super) fn stable_iteration_number(path: &str) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(path.trim().as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    // Reason: Azure iteration identifiers are UUID strings, not provider-compatible
    // u64s; hashing the stable iteration path gives create/list round-trip behavior
    // without adding provider-specific state storage.
    u64::from_be_bytes(bytes)
}

pub(super) fn parse_iterations_json(json_str: &str) -> Result<Vec<AzureIteration>> {
    let raw: Vec<Value> =
        serde_json::from_str(json_str).context("Failed to parse Azure DevOps iterations JSON")?;
    parse_iteration_nodes(&raw)
}

pub(super) fn parse_iteration_json(json_str: &str) -> Result<AzureIteration> {
    let raw: Value =
        serde_json::from_str(json_str).context("Failed to parse Azure DevOps iteration JSON")?;
    parse_iteration_value(&raw)
}

pub(super) fn filter_iterations_by_state(
    iterations: Vec<AzureIteration>,
    state: &str,
    today: NaiveDate,
) -> Result<Vec<AzureIteration>> {
    match state {
        "all" => Ok(iterations),
        "open" => Ok(iterations
            .into_iter()
            .filter(|iteration| {
                iteration
                    .finish_date
                    .is_none_or(|finish_date| finish_date >= today)
            })
            .collect()),
        "closed" => Ok(iterations
            .into_iter()
            .filter(|iteration| {
                iteration
                    .finish_date
                    .is_some_and(|finish_date| finish_date < today)
            })
            .collect()),
        _ => anyhow::bail!(
            "Invalid milestone state: {:?}. Must be open, closed, or all",
            state
        ),
    }
}

pub(super) fn iterations_to_milestones(
    iterations: Vec<AzureIteration>,
    today: NaiveDate,
) -> Vec<Milestone> {
    iterations
        .into_iter()
        .map(|iteration| iteration_to_milestone(iteration, today))
        .collect()
}

pub(super) fn iteration_path_for_milestone_number(
    iterations: &[AzureIteration],
    number: u64,
) -> Option<String> {
    iterations
        .iter()
        .find(|iteration| iteration.number == number)
        .map(|iteration| iteration.path.clone())
}

pub(super) fn iteration_state(iteration: &AzureIteration, today: NaiveDate) -> String {
    if iteration
        .finish_date
        .is_some_and(|finish_date| finish_date < today)
    {
        "closed".to_string()
    } else {
        "open".to_string()
    }
}

fn parse_iteration_nodes(nodes: &[Value]) -> Result<Vec<AzureIteration>> {
    let mut iterations = Vec::new();
    for node in nodes {
        flatten_iteration_node(node, &mut iterations)?;
    }
    Ok(iterations)
}

fn flatten_iteration_node(node: &Value, iterations: &mut Vec<AzureIteration>) -> Result<()> {
    iterations.push(parse_iteration_value(node)?);
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for child in children {
            flatten_iteration_node(child, iterations)?;
        }
    }
    Ok(())
}

fn parse_iteration_value(value: &Value) -> Result<AzureIteration> {
    let title = value
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("Missing 'name' field in Azure DevOps iteration JSON"))?
        .to_string();
    let path = value
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("Missing 'path' field in Azure DevOps iteration JSON"))?
        .to_string();
    let description = value
        .get("description")
        .or_else(|| value.pointer("/attributes/description"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let finish_date = value
        .pointer("/attributes/finishDate")
        .and_then(Value::as_str)
        .map(parse_finish_date)
        .transpose()?;

    Ok(AzureIteration {
        number: stable_iteration_number(&path),
        title,
        path,
        description,
        finish_date,
    })
}

fn parse_finish_date(raw: &str) -> Result<NaiveDate> {
    if let Ok(date_time) = DateTime::parse_from_rfc3339(raw) {
        return Ok(date_time.date_naive());
    }
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .with_context(|| format!("Failed to parse Azure DevOps iteration finishDate {raw:?}"))
}

fn iteration_to_milestone(iteration: AzureIteration, today: NaiveDate) -> Milestone {
    let state = iteration_state(&iteration, today);
    Milestone {
        number: iteration.number,
        title: iteration.title,
        description: iteration.description,
        state,
        open_issues: 0,
        closed_issues: 0,
    }
}

pub(super) fn today_utc() -> NaiveDate {
    Utc::now().date_naive()
}
