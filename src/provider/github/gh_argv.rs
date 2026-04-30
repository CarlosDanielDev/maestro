//! Pure builders for `gh` CLI argv vectors.
//!
//! Every `GhCliClient` method that shells out builds its argv via a function
//! in this module. The functions are pure (no I/O) so they can be snapshot-
//! tested directly. This is the guard that catches wire-level bugs like the
//! 2026-03-20 `gh pr create --json number` regression: any change to the
//! argv shape produces a snapshot diff and forces a reviewer to look.
//!
//! Conventions
//! - Each function returns `Vec<String>` so call sites don't have to manage
//!   borrow lifetimes for `format!`-derived args.
//! - Call sites convert with `.iter().map(String::as_str).collect()`.
//! - Tests live in `#[cfg(test)] mod tests` at the bottom; one snapshot
//!   per builder using literal `Vec<&str>` for readability.

use crate::provider::github::types::PrReviewEvent;

pub(crate) fn build_create_pr_argv(
    head_branch: &str,
    base_branch: &str,
    title: &str,
    body: &str,
) -> Vec<String> {
    vec![
        "pr".into(),
        "create".into(),
        "--head".into(),
        head_branch.into(),
        "--base".into(),
        base_branch.into(),
        "--title".into(),
        title.into(),
        "--body".into(),
        body.into(),
    ]
}

pub(crate) fn build_list_prs_for_branch_argv(head_branch: &str) -> Vec<String> {
    vec![
        "pr".into(),
        "list".into(),
        "--head".into(),
        head_branch.into(),
        "--state".into(),
        "open".into(),
        "--json".into(),
        "number".into(),
    ]
}

pub(crate) fn build_get_issue_argv(issue_number: u64) -> Vec<String> {
    vec![
        "issue".into(),
        "view".into(),
        issue_number.to_string(),
        "--json".into(),
        "number,title,body,labels,state,url".into(),
    ]
}

pub(crate) fn build_add_label_argv(issue_number: u64, label: &str) -> Vec<String> {
    vec![
        "issue".into(),
        "edit".into(),
        issue_number.to_string(),
        "--add-label".into(),
        label.into(),
    ]
}

pub(crate) fn build_remove_label_argv(issue_number: u64, label: &str) -> Vec<String> {
    vec![
        "issue".into(),
        "edit".into(),
        issue_number.to_string(),
        "--remove-label".into(),
        label.into(),
    ]
}

pub(crate) fn build_submit_pr_review_argv(
    pr_number: u64,
    event: PrReviewEvent,
    body: &str,
) -> Vec<String> {
    let mut argv = vec![
        "pr".into(),
        "review".into(),
        pr_number.to_string(),
        format!("--{}", event.as_gh_arg()),
    ];
    if !body.is_empty() {
        argv.push("--body".into());
        argv.push(body.into());
    }
    argv
}

pub(crate) fn build_list_labels_argv() -> Vec<String> {
    vec![
        "label".into(),
        "list".into(),
        "--json".into(),
        "name".into(),
        "--limit".into(),
        "200".into(),
    ]
}

pub(crate) fn build_create_label_argv(name: &str, color: &str) -> Vec<String> {
    vec![
        "label".into(),
        "create".into(),
        name.into(),
        "--color".into(),
        color.into(),
        "--description".into(),
        "Managed by Maestro".into(),
        "--force".into(),
    ]
}

pub(crate) fn build_create_milestone_argv(title: &str, description: &str) -> Vec<String> {
    vec![
        "api".into(),
        "repos/{owner}/{repo}/milestones".into(),
        "--method".into(),
        "POST".into(),
        "-f".into(),
        format!("title={}", title),
        "-f".into(),
        format!("description={}", description),
    ]
}

/// Builds the argv portion only — the JSON payload is sent over stdin via
/// `run_gh_with_stdin`. Tests assert both pieces.
pub(crate) fn build_create_issue_argv() -> Vec<String> {
    vec![
        "api".into(),
        "repos/{owner}/{repo}/issues".into(),
        "--method".into(),
        "POST".into(),
        "--input".into(),
        "-".into(),
    ]
}

pub(crate) fn build_patch_milestone_description_argv(milestone_number: u64) -> Vec<String> {
    vec![
        "api".into(),
        format!("repos/{{owner}}/{{repo}}/milestones/{}", milestone_number),
        "--method".into(),
        "PATCH".into(),
        "--input".into(),
        "-".into(),
    ]
}

pub(crate) fn build_list_issues_argv(labels_csv: Option<&str>) -> Vec<String> {
    let mut argv = vec![
        "issue".into(),
        "list".into(),
        "--state".into(),
        "open".into(),
        "--limit".into(),
        "100".into(),
        "--json".into(),
        "number,title,body,labels,state,url,milestone".into(),
    ];
    if let Some(csv) = labels_csv
        && !csv.is_empty()
    {
        argv.push("--label".into());
        argv.push(csv.into());
    }
    argv
}

pub(crate) fn build_list_issues_by_milestone_argv(milestone: &str) -> Vec<String> {
    vec![
        "issue".into(),
        "list".into(),
        "--milestone".into(),
        milestone.into(),
        "--state".into(),
        "open".into(),
        "--limit".into(),
        "100".into(),
        "--json".into(),
        "number,title,body,labels,state,url,milestone".into(),
    ]
}

pub(crate) fn build_list_milestones_argv(state: &str) -> Vec<String> {
    vec![
        "api".into(),
        format!("repos/{{owner}}/{{repo}}/milestones?state={}", state),
        "--paginate".into(),
    ]
}

pub(crate) fn build_list_open_prs_argv(json_fields: &str) -> Vec<String> {
    vec![
        "pr".into(),
        "list".into(),
        "--state".into(),
        "open".into(),
        "--limit".into(),
        "100".into(),
        "--json".into(),
        json_fields.into(),
    ]
}

pub(crate) fn build_get_pr_argv(pr_number: u64, json_fields: &str) -> Vec<String> {
    vec![
        "pr".into(),
        "view".into(),
        pr_number.to_string(),
        "--json".into(),
        json_fields.into(),
    ]
}

pub(crate) fn build_create_issue_dupe_check_argv() -> Vec<String> {
    vec![
        "issue".into(),
        "list".into(),
        "--state".into(),
        "all".into(),
        "--limit".into(),
        "1000".into(),
        "--json".into(),
        "number,title,state".into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| (*a).to_string()).collect()
    }

    #[test]
    fn create_pr_argv_locks_today_format() {
        // Regression guard for the 2026-03-20 `--json number` bug.
        assert_eq!(
            build_create_pr_argv("feat/x", "main", "Title", "Body"),
            s(&[
                "pr", "create", "--head", "feat/x", "--base", "main", "--title", "Title", "--body",
                "Body",
            ])
        );
        // The fact that there is NO `--json` is the assertion.
        assert!(
            !build_create_pr_argv("a", "b", "c", "d").contains(&"--json".to_string()),
            "gh pr create does NOT accept --json; reintroducing it breaks every auto-PR"
        );
    }

    #[test]
    fn list_prs_for_branch_argv() {
        assert_eq!(
            build_list_prs_for_branch_argv("feat/x"),
            s(&[
                "pr", "list", "--head", "feat/x", "--state", "open", "--json", "number",
            ])
        );
    }

    #[test]
    fn get_issue_argv() {
        // Note: gh issue view --json field set must NOT include `milestone` —
        // that field is invalid on `issue view` (only `milestoneTitle` /
        // `milestoneNumber` are valid; we currently omit it entirely).
        let argv = build_get_issue_argv(42);
        assert_eq!(
            argv,
            s(&[
                "issue",
                "view",
                "42",
                "--json",
                "number,title,body,labels,state,url",
            ])
        );
        // Lock that we don't request `milestone` — that would fail at runtime.
        let json_field = &argv[4];
        assert!(
            !json_field.split(',').any(|f| f == "milestone"),
            "`milestone` is not a valid --json field on `gh issue view`; \
             use --json milestoneNumber or fetch via `gh api` if needed."
        );
    }

    #[test]
    fn add_label_argv() {
        assert_eq!(
            build_add_label_argv(7, "maestro:done"),
            s(&["issue", "edit", "7", "--add-label", "maestro:done",])
        );
    }

    #[test]
    fn remove_label_argv() {
        assert_eq!(
            build_remove_label_argv(7, "maestro:in-progress"),
            s(&[
                "issue",
                "edit",
                "7",
                "--remove-label",
                "maestro:in-progress",
            ])
        );
    }

    #[test]
    fn submit_pr_review_argv_with_body() {
        assert_eq!(
            build_submit_pr_review_argv(99, PrReviewEvent::Approve, "LGTM"),
            s(&["pr", "review", "99", "--approve", "--body", "LGTM",])
        );
    }

    #[test]
    fn submit_pr_review_argv_omits_body_when_empty() {
        let argv = build_submit_pr_review_argv(99, PrReviewEvent::Comment, "");
        assert_eq!(argv, s(&["pr", "review", "99", "--comment"]));
        assert!(!argv.contains(&"--body".to_string()));
    }

    #[test]
    fn submit_pr_review_argv_request_changes() {
        let argv = build_submit_pr_review_argv(99, PrReviewEvent::RequestChanges, "see comments");
        assert_eq!(
            argv,
            s(&[
                "pr",
                "review",
                "99",
                "--request-changes",
                "--body",
                "see comments",
            ])
        );
    }

    #[test]
    fn list_labels_argv() {
        assert_eq!(
            build_list_labels_argv(),
            s(&["label", "list", "--json", "name", "--limit", "200"])
        );
    }

    #[test]
    fn create_label_argv() {
        assert_eq!(
            build_create_label_argv("maestro:ready", "0E8A16"),
            s(&[
                "label",
                "create",
                "maestro:ready",
                "--color",
                "0E8A16",
                "--description",
                "Managed by Maestro",
                "--force",
            ])
        );
    }

    #[test]
    fn create_milestone_argv() {
        assert_eq!(
            build_create_milestone_argv("v1.0", "First release"),
            s(&[
                "api",
                "repos/{owner}/{repo}/milestones",
                "--method",
                "POST",
                "-f",
                "title=v1.0",
                "-f",
                "description=First release",
            ])
        );
    }

    #[test]
    fn create_issue_argv_uses_stdin_input() {
        // The issue body is sent over stdin to `run_gh_with_stdin`; the
        // argv only carries the route + method.
        assert_eq!(
            build_create_issue_argv(),
            s(&[
                "api",
                "repos/{owner}/{repo}/issues",
                "--method",
                "POST",
                "--input",
                "-",
            ])
        );
    }

    #[test]
    fn patch_milestone_description_argv() {
        assert_eq!(
            build_patch_milestone_description_argv(28),
            s(&[
                "api",
                "repos/{owner}/{repo}/milestones/28",
                "--method",
                "PATCH",
                "--input",
                "-",
            ])
        );
    }

    #[test]
    fn list_issues_argv_with_labels() {
        assert_eq!(
            build_list_issues_argv(Some("a,b,c")),
            s(&[
                "issue",
                "list",
                "--state",
                "open",
                "--limit",
                "100",
                "--json",
                "number,title,body,labels,state,url,milestone",
                "--label",
                "a,b,c",
            ])
        );
    }

    #[test]
    fn list_issues_argv_without_labels() {
        let argv = build_list_issues_argv(None);
        assert!(!argv.contains(&"--label".to_string()));
        assert_eq!(
            argv.last().unwrap(),
            "number,title,body,labels,state,url,milestone"
        );
    }

    #[test]
    fn list_issues_argv_with_empty_labels_treated_as_none() {
        let argv = build_list_issues_argv(Some(""));
        assert!(!argv.contains(&"--label".to_string()));
    }

    #[test]
    fn list_issues_by_milestone_argv() {
        assert_eq!(
            build_list_issues_by_milestone_argv("v0.17.0"),
            s(&[
                "issue",
                "list",
                "--milestone",
                "v0.17.0",
                "--state",
                "open",
                "--limit",
                "100",
                "--json",
                "number,title,body,labels,state,url,milestone",
            ])
        );
    }

    #[test]
    fn list_milestones_argv() {
        assert_eq!(
            build_list_milestones_argv("open"),
            s(&[
                "api",
                "repos/{owner}/{repo}/milestones?state=open",
                "--paginate",
            ])
        );
    }

    #[test]
    fn list_open_prs_argv() {
        assert_eq!(
            build_list_open_prs_argv("number,title,state"),
            s(&[
                "pr",
                "list",
                "--state",
                "open",
                "--limit",
                "100",
                "--json",
                "number,title,state",
            ])
        );
    }

    #[test]
    fn get_pr_argv() {
        assert_eq!(
            build_get_pr_argv(541, "number,title"),
            s(&["pr", "view", "541", "--json", "number,title"])
        );
    }

    #[test]
    fn create_issue_dupe_check_argv() {
        assert_eq!(
            build_create_issue_dupe_check_argv(),
            s(&[
                "issue",
                "list",
                "--state",
                "all",
                "--limit",
                "1000",
                "--json",
                "number,title,state",
            ])
        );
    }
}
