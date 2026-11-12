use crate::config::Config;
use crate::provider::{create_provider, github::client::RepoProvider};
use crate::work::assigner::WorkAssigner;
use crate::work::types::WorkItem;

pub async fn cmd_queue() -> anyhow::Result<()> {
    let config = Config::find_and_load()?;
    let provider_config = config.effective_provider_config();
    let client = create_provider(&provider_config)?;
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
    let no_completed = std::collections::HashSet::new();

    println!(
        "{:<10} {:<8} {:<50} {:<10} {:<15}",
        "Priority", "Issue", "Title", "Status", "Blocked By"
    );
    println!("{}", "-".repeat(93));

    for item in assigner.scheduled_items() {
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

    println!("\n{}", summary_line(assigner.all_items()));

    Ok(())
}

/// One-line queue summary. Readiness uses the same `WorkItem::is_ready`
/// predicate as the rows above — an issue counts as ready only when all of
/// its blockers are done, not merely because its status is Pending.
fn summary_line(items: &[WorkItem]) -> String {
    let no_completed = std::collections::HashSet::new();
    let ready = items
        .iter()
        .filter(|item| item.is_ready(&no_completed))
        .count();
    format!(
        "Total: {} issues ({} ready, {} blocked)",
        items.len(),
        ready,
        items.len() - ready
    )
}

pub async fn cmd_add(issue_number: u64) -> anyhow::Result<()> {
    let config = Config::find_and_load()?;
    let provider_config = config.effective_provider_config();
    let client = create_provider(&provider_config)?;
    client.add_label(issue_number, "maestro:ready").await?;
    println!("Added 'maestro:ready' label to issue #{}", issue_number);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::types::{Issue, Priority};

    fn make_issue(number: u64, priority: Priority, blocked_by: &[u64]) -> Issue {
        let mut labels: Vec<String> = blocked_by
            .iter()
            .map(|b| format!("blocked-by:#{}", b))
            .collect();
        labels.push(format!("priority:{:?}", priority));
        labels.push("maestro:ready".to_string());
        Issue {
            number,
            title: format!("Issue #{}", number),
            body: String::new(),
            labels,
            state: "open".to_string(),
            html_url: String::new(),
            milestone: None,
            assignees: vec![],
        }
    }

    #[test]
    fn summary_line_counts_blocked_by_dependencies_not_status() {
        // Fixture from #1146: one P1, two P2, one of them blocked by the P1.
        let items: Vec<WorkItem> = vec![
            make_issue(1, Priority::P1, &[]),
            make_issue(2, Priority::P2, &[]),
            make_issue(3, Priority::P2, &[1]),
        ]
        .into_iter()
        .map(WorkItem::from_issue)
        .collect();

        assert_eq!(summary_line(&items), "Total: 3 issues (2 ready, 1 blocked)");
    }
}
