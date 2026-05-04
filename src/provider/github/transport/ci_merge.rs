use super::{GhCliClient, argv_refs};
use crate::provider::types::{CheckRun, CiStatus, MergeMethod};
use crate::util::validate_gh_arg;
use anyhow::Result;

pub(super) async fn ci_status_for_branch(client: &GhCliClient, branch: &str) -> Result<CiStatus> {
    validate_gh_arg(branch, "branch")?;
    let mut argv = vec![
        "pr".to_string(),
        "view".to_string(),
        branch.to_string(),
        "--json".to_string(),
        "statusCheckRollup,mergeStateStatus".to_string(),
    ];
    append_repo_arg(client, &mut argv);
    let json_str = client.run_gh(&argv_refs(&argv)).await?;
    crate::provider::github::ci::parse_ci_json(&json_str)
}

pub(super) async fn ci_status_for_pr(client: &GhCliClient, pr_number: u64) -> Result<CiStatus> {
    let mut argv = vec![
        "pr".to_string(),
        "view".to_string(),
        pr_number.to_string(),
        "--json".to_string(),
        "statusCheckRollup,mergeStateStatus".to_string(),
    ];
    append_repo_arg(client, &mut argv);
    let json_str = client.run_gh(&argv_refs(&argv)).await?;
    crate::provider::github::ci::parse_ci_json(&json_str)
}

pub(super) async fn ci_check_runs_for_pr(
    client: &GhCliClient,
    pr_number: u64,
) -> Result<Vec<CheckRun>> {
    let mut argv = vec![
        "pr".to_string(),
        "checks".to_string(),
        pr_number.to_string(),
        "--json".to_string(),
        "name,status,conclusion,startedAt,completedAt".to_string(),
    ];
    append_repo_arg(client, &mut argv);
    let json_str = client.run_gh(&argv_refs(&argv)).await?;
    crate::provider::github::ci::parse_check_details(&json_str)
}

pub(super) async fn ci_logs_for_check(client: &GhCliClient, check_id: &str) -> Result<String> {
    validate_gh_arg(check_id, "check_id")?;
    if check_id.parse::<u64>().is_ok() {
        let mut argv = vec![
            "run".to_string(),
            "view".to_string(),
            check_id.to_string(),
            "--log-failed".to_string(),
        ];
        append_repo_arg(client, &mut argv);
        let full_log = client.run_gh(&argv_refs(&argv)).await?;
        return Ok(crate::provider::github::ci::truncate_log(&full_log, 4000));
    }

    let mut list_argv = vec![
        "run".to_string(),
        "list".to_string(),
        "--branch".to_string(),
        check_id.to_string(),
        "--status".to_string(),
        "failure".to_string(),
        "--limit".to_string(),
        "1".to_string(),
        "--json".to_string(),
        "databaseId".to_string(),
    ];
    append_repo_arg(client, &mut list_argv);
    let runs_json = client.run_gh(&argv_refs(&list_argv)).await?;
    let runs: Vec<serde_json::Value> = serde_json::from_str(&runs_json)?;
    let run_id = runs
        .first()
        .and_then(|r| r.get("databaseId"))
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("No failed run found for {}", check_id))?;

    let mut view_argv = vec![
        "run".to_string(),
        "view".to_string(),
        run_id.to_string(),
        "--log-failed".to_string(),
    ];
    append_repo_arg(client, &mut view_argv);
    let full_log = client.run_gh(&argv_refs(&view_argv)).await?;
    Ok(crate::provider::github::ci::truncate_log(&full_log, 4000))
}

pub(super) async fn merge_pr(
    client: &GhCliClient,
    pr_number: u64,
    method: MergeMethod,
) -> Result<()> {
    let mut argv = vec![
        "pr".to_string(),
        "merge".to_string(),
        pr_number.to_string(),
        method.flag().to_string(),
        "--delete-branch".to_string(),
    ];
    append_repo_arg(client, &mut argv);
    client.run_gh(&argv_refs(&argv)).await?;
    Ok(())
}

fn append_repo_arg(client: &GhCliClient, argv: &mut Vec<String>) {
    if let Some(repo) = client.repo_arg() {
        argv.extend(["--repo".to_string(), repo.to_string()]);
    }
}
