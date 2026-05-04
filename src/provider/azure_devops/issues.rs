use anyhow::{Context, Result};

pub(super) fn azure_tags_field(labels: &[String]) -> Option<String> {
    (!labels.is_empty()).then(|| format!("System.Tags={}", labels.join("; ")))
}

pub(super) fn build_create_work_item_args(
    organization: &str,
    project: &str,
    title: &str,
    body: &str,
    labels: &[String],
    iteration_path: Option<&str>,
) -> Vec<String> {
    let mut args = vec![
        "boards".to_string(),
        "work-item".to_string(),
        "create".to_string(),
        "--type".to_string(),
        "User Story".to_string(),
        "--title".to_string(),
        title.to_string(),
        "--description".to_string(),
        body.to_string(),
        "--org".to_string(),
        organization.to_string(),
        "--project".to_string(),
        project.to_string(),
    ];

    if let Some(tags_field) = azure_tags_field(labels) {
        args.push("--fields".to_string());
        args.push(tags_field);
    }

    if let Some(iteration_path) = iteration_path.filter(|path| !path.is_empty()) {
        args.push("--iteration".to_string());
        args.push(iteration_path.to_string());
    }

    args.extend(["-o".to_string(), "json".to_string()]);
    args
}

pub(super) fn parse_created_work_item_id(json_str: &str) -> Result<u64> {
    let v: serde_json::Value =
        serde_json::from_str(json_str).context("Failed to parse work item creation response")?;
    v.get("id")
        .and_then(|n| n.as_u64())
        .ok_or_else(|| anyhow::anyhow!("Missing 'id' in work item creation response"))
}
