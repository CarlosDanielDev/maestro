use crate::config::Config;
use crate::provider::github::client::{GhCliClient, GitHubClient};
use crate::work::assigner::WorkAssigner;
use crate::work::types::WorkItem;

pub async fn cmd_queue() -> anyhow::Result<()> {
    let config = Config::find_and_load()?;
    let client = GhCliClient::new();
    let label_refs: Vec<&str> = config
        .github
        .issue_filter_labels
        .iter()
        .map(|s| s.as_str())
        .collect();
    let issues = client.list_issues(&label_refs).await?;

    if issues.is_empty() {
        println!(
            "No issues found with labels: {:?}",
            config.github.issue_filter_labels
        );
        return Ok(());
    }

    let items: Vec<WorkItem> = issues.into_iter().map(WorkItem::from_issue).collect();
    let assigner = WorkAssigner::new(items);

    println!(
        "{:<10} {:<8} {:<50} {:<10} {:<15}",
        "Priority", "Issue", "Title", "Status", "Blocked By"
    );
    println!("{}", "-".repeat(93));

    for item in assigner.all_items() {
        let blocked_str = if item.blocked_by.is_empty() {
            "-".to_string()
        } else {
            item.blocked_by
                .iter()
                .map(|n| format!("#{}", n))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let title: String = if item.title().chars().count() > 48 {
            let truncated: String = item.title().chars().take(45).collect();
            format!("{}...", truncated)
        } else {
            item.title().to_string()
        };
        let no_completed = std::collections::HashSet::new();
        let ready_str = if item.is_ready(&no_completed) {
            "Ready"
        } else {
            "Blocked"
        };
        println!(
            "{:<10} #{:<7} {:<50} {:<10} {}",
            format!("{:?}", item.priority),
            item.number(),
            title,
            ready_str,
            blocked_str
        );
    }

    let counts = assigner.count_by_status();
    println!(
        "\nTotal: {} issues ({} ready, {} blocked)",
        assigner.total(),
        counts.pending,
        assigner.total() - counts.pending
    );

    Ok(())
}

pub async fn cmd_add(issue_number: u64) -> anyhow::Result<()> {
    let client = GhCliClient::new();
    client.add_label(issue_number, "maestro:ready").await?;
    println!("Added 'maestro:ready' label to issue #{}", issue_number);
    Ok(())
}
