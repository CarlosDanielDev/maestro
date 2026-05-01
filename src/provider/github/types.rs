use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::OnceLock;

static BLOCKED_BY_RE: OnceLock<regex::Regex> = OnceLock::new();

fn blocked_by_regex() -> &'static regex::Regex {
    BLOCKED_BY_RE.get_or_init(|| regex::Regex::new(r"(?i)blocked-by:\s*#(\d+)").unwrap())
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum Priority {
    P0 = 0,
    P1 = 1,
    #[default]
    P2 = 2,
}

impl Priority {
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "priority:P0" => Some(Self::P0),
            "priority:P1" => Some(Self::P1),
            "priority:P2" => Some(Self::P2),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaestroLabel {
    Ready,
    InProgress,
    Done,
    Failed,
}

impl MaestroLabel {
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "maestro:ready" => Some(Self::Ready),
            "maestro:in-progress" => Some(Self::InProgress),
            "maestro:done" => Some(Self::Done),
            "maestro:failed" => Some(Self::Failed),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "maestro:ready",
            Self::InProgress => "maestro:in-progress",
            Self::Done => "maestro:done",
            Self::Failed => "maestro:failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    Orchestrator,
    Vibe,
}

impl SessionMode {
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "mode:orchestrator" => Some(Self::Orchestrator),
            "mode:vibe" => Some(Self::Vibe),
            _ => None,
        }
    }

    pub fn as_config_str(&self) -> &'static str {
        match self {
            Self::Orchestrator => "orchestrator",
            Self::Vibe => "vibe",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhIssue {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
    pub state: String,
    pub html_url: String,
    #[serde(default)]
    pub milestone: Option<u64>,
    #[serde(default)]
    pub assignees: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhMilestone {
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub state: String,
    #[serde(default)]
    pub open_issues: u32,
    #[serde(default)]
    pub closed_issues: u32,
}

impl GhIssue {
    /// Extract the priority from labels. Defaults to P2.
    pub fn priority(&self) -> Priority {
        self.labels
            .iter()
            .find_map(|l| Priority::from_label(l))
            .unwrap_or_default()
    }

    /// Extract the maestro session mode from labels.
    pub fn session_mode(&self) -> Option<SessionMode> {
        self.labels.iter().find_map(|l| SessionMode::from_label(l))
    }

    /// Extract `blocked-by:#N` dependencies from labels.
    pub fn blocked_by_from_labels(&self) -> Vec<u64> {
        self.labels
            .iter()
            .filter_map(|l| {
                l.strip_prefix("blocked-by:#")
                    .and_then(|n| n.parse::<u64>().ok())
            })
            .collect()
    }

    /// Extract `blocked-by: #N` from issue body text (case-insensitive).
    pub fn blocked_by_from_body(&self) -> Vec<u64> {
        blocked_by_regex()
            .captures_iter(&self.body)
            .filter_map(|cap| cap[1].parse::<u64>().ok())
            .collect()
    }

    /// All blocking issue numbers (union of labels + body, deduplicated).
    pub fn all_blockers(&self) -> Vec<u64> {
        let mut blockers = self.blocked_by_from_labels();
        blockers.extend(self.blocked_by_from_body());
        blockers.sort_unstable();
        blockers.dedup();
        blockers
    }

    #[allow(dead_code)]
    pub fn has_maestro_label(&self, label: MaestroLabel) -> bool {
        self.labels.iter().any(|l| l == label.as_str())
    }

    /// Build a prompt for an unattended Claude session working on this issue.
    pub fn unattended_prompt(&self) -> String {
        format!(
            "Work on GitHub issue #{}.\n\nTitle: {}\n\nDescription:\n{}\n\n\
             IMPORTANT: You are running in unattended mode (no human at the terminal). \
             Do NOT use AskUserQuestion or ask for clarification — make your best judgment \
             and proceed autonomously. Read relevant source files first, then implement \
             the required changes.",
            self.number, self.title, self.body
        )
    }
}

/// A pull request from the GitHub API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhPullRequest {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub html_url: String,
    pub head_branch: String,
    pub base_branch: String,
    pub author: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub mergeable: bool,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    #[serde(default)]
    pub changed_files: u64,
}

/// The type of review action to submit on a pull request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PrReviewEvent {
    Approve,
    RequestChanges,
    #[default]
    Comment,
}

impl PrReviewEvent {
    pub fn as_gh_arg(&self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::RequestChanges => "request-changes",
            Self::Comment => "comment",
        }
    }

    #[allow(dead_code)] // Reason: used by PR review screen rendering
    pub fn label(&self) -> &'static str {
        match self {
            Self::Approve => "Approve",
            Self::RequestChanges => "Request Changes",
            Self::Comment => "Comment",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Comment => Self::Approve,
            Self::Approve => Self::RequestChanges,
            Self::RequestChanges => Self::Comment,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::Comment => Self::RequestChanges,
            Self::Approve => Self::Comment,
            Self::RequestChanges => Self::Approve,
        }
    }
}

/// Most recent error messages a PendingPr will retain for correlation.
pub const PENDING_PR_LAST_ERRORS_CAP: usize = 3;

/// Lifetime cap on Shift+P-triggered manual retries before the entry is
/// transitioned to `PermanentlyFailed`.
pub const PENDING_PR_MANUAL_RETRY_LIFETIME_CAP: u32 = 5;

/// A PR creation that failed and is queued for retry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPr {
    pub issue_number: u64,
    /// Additional issue numbers for unified PR sessions.
    pub issue_numbers: Vec<u64>,
    pub branch: String,
    pub base_branch: String,
    pub files_touched: Vec<String>,
    pub cost_usd: f64,
    pub attempt: u32,
    pub max_attempts: u32,
    pub last_error: String,
    pub last_attempt_at: DateTime<Utc>,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub status: PendingPrStatus,
    /// Most recent errors (oldest evicted at len = `PENDING_PR_LAST_ERRORS_CAP`).
    /// Used to detect deterministic-failure loops where every retry hits the
    /// same error and Shift+P would just queue another doomed attempt.
    #[serde(default)]
    pub last_errors: VecDeque<String>,
    /// Lifetime count of Shift+P-triggered manual retries. Capped at
    /// `PENDING_PR_MANUAL_RETRY_LIFETIME_CAP` to prevent infinite-loop
    /// abandonment when the underlying failure is deterministic.
    #[serde(default)]
    pub manual_retry_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingPrStatus {
    /// Will auto-retry at next_retry_at.
    RetryScheduled,
    /// All auto-retries exhausted, awaiting manual retry.
    AwaitingManualRetry,
    /// User triggered a manual retry, in progress.
    Retrying,
    /// Terminal state. Reached when (a) the same error has repeated
    /// `PENDING_PR_LAST_ERRORS_CAP` times — a deterministic failure that
    /// further retries cannot fix — or (b) `manual_retry_count` exceeds
    /// `PENDING_PR_MANUAL_RETRY_LIFETIME_CAP`. Both auto-retry and
    /// `trigger_manual_pr_retry` skip entries in this state. The user is
    /// expected to fix the underlying problem (e.g., file a bug, run
    /// `gh pr create` manually) and dismiss the entry.
    PermanentlyFailed,
}

/// Canonical PendingPr fixture for cross-module test reuse. Returns an entry
/// in the `AwaitingManualRetry` shape: 3/3 attempts spent, no errors stored,
/// no scheduled retry. Override fields after construction for variants.
#[cfg(test)]
pub(crate) fn awaiting_pending_pr(issue_number: u64) -> PendingPr {
    PendingPr {
        issue_number,
        issue_numbers: vec![],
        branch: format!("maestro/issue-{}", issue_number),
        base_branch: "main".into(),
        files_touched: vec![],
        cost_usd: 0.0,
        attempt: 3,
        max_attempts: 3,
        last_error: String::new(),
        last_attempt_at: chrono::Utc::now(),
        next_retry_at: None,
        status: PendingPrStatus::AwaitingManualRetry,
        last_errors: VecDeque::new(),
        manual_retry_count: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_issue(number: u64, labels: &[&str], body: &str) -> GhIssue {
        GhIssue {
            number,
            title: format!("Issue #{}", number),
            body: body.to_string(),
            labels: labels.iter().map(|s| s.to_string()).collect(),
            state: "open".to_string(),
            html_url: format!("https://github.com/owner/repo/issues/{}", number),
            milestone: None,
            assignees: vec![],
        }
    }

    // --- Issue #159: PendingPr serde tests ---

    #[test]
    fn pending_pr_status_serializes_as_snake_case() {
        let json = serde_json::to_string(&PendingPrStatus::RetryScheduled).unwrap();
        assert_eq!(json, r#""retry_scheduled""#);
        let json = serde_json::to_string(&PendingPrStatus::AwaitingManualRetry).unwrap();
        assert_eq!(json, r#""awaiting_manual_retry""#);
        let json = serde_json::to_string(&PendingPrStatus::Retrying).unwrap();
        assert_eq!(json, r#""retrying""#);
        let json = serde_json::to_string(&PendingPrStatus::PermanentlyFailed).unwrap();
        assert_eq!(json, r#""permanently_failed""#);
    }

    #[test]
    fn pending_pr_round_trips_via_serde() {
        let pending = PendingPr {
            issue_number: 42,
            issue_numbers: vec![],
            branch: "maestro/issue-42".to_string(),
            base_branch: "main".to_string(),
            files_touched: vec!["src/lib.rs".to_string()],
            cost_usd: 1.23,
            attempt: 1,
            max_attempts: 3,
            last_error: "network timeout".to_string(),
            last_attempt_at: Utc::now(),
            next_retry_at: Some(Utc::now()),
            status: PendingPrStatus::RetryScheduled,
            last_errors: VecDeque::from(vec!["boom".to_string(), "boom".to_string()]),
            manual_retry_count: 2,
        };
        let json = serde_json::to_string(&pending).unwrap();
        let rt: PendingPr = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.issue_number, 42);
        assert_eq!(rt.branch, "maestro/issue-42");
        assert_eq!(rt.attempt, 1);
        assert_eq!(rt.status, PendingPrStatus::RetryScheduled);
        assert_eq!(rt.last_errors.len(), 2);
        assert_eq!(rt.manual_retry_count, 2);
    }

    #[test]
    fn pending_pr_deserializes_with_default_for_new_fields() {
        // Existing on-disk state files do not have last_errors or
        // manual_retry_count. They MUST round-trip cleanly via #[serde(default)].
        let legacy_json = r#"{
            "issue_number": 7,
            "issue_numbers": [],
            "branch": "maestro/issue-7",
            "base_branch": "main",
            "files_touched": [],
            "cost_usd": 0.0,
            "attempt": 0,
            "max_attempts": 3,
            "last_error": "",
            "last_attempt_at": "2026-01-01T00:00:00Z",
            "next_retry_at": null,
            "status": "retry_scheduled"
        }"#;
        let p: PendingPr = serde_json::from_str(legacy_json).unwrap();
        assert!(p.last_errors.is_empty());
        assert_eq!(p.manual_retry_count, 0);
    }

    // Priority::from_label

    #[test]
    fn priority_from_label_p0() {
        assert_eq!(Priority::from_label("priority:P0"), Some(Priority::P0));
    }

    #[test]
    fn priority_from_label_p1() {
        assert_eq!(Priority::from_label("priority:P1"), Some(Priority::P1));
    }

    #[test]
    fn priority_from_label_p2() {
        assert_eq!(Priority::from_label("priority:P2"), Some(Priority::P2));
    }

    #[test]
    fn priority_from_label_unknown_returns_none() {
        assert_eq!(Priority::from_label("random-label"), None);
    }

    #[test]
    fn priority_from_label_empty_returns_none() {
        assert_eq!(Priority::from_label(""), None);
    }

    #[test]
    fn priority_default_is_p2() {
        assert_eq!(Priority::default(), Priority::P2);
    }

    // MaestroLabel

    #[test]
    fn maestro_label_as_str_ready() {
        assert_eq!(MaestroLabel::Ready.as_str(), "maestro:ready");
    }

    #[test]
    fn maestro_label_as_str_in_progress() {
        assert_eq!(MaestroLabel::InProgress.as_str(), "maestro:in-progress");
    }

    #[test]
    fn maestro_label_as_str_done() {
        assert_eq!(MaestroLabel::Done.as_str(), "maestro:done");
    }

    #[test]
    fn maestro_label_as_str_failed() {
        assert_eq!(MaestroLabel::Failed.as_str(), "maestro:failed");
    }

    #[test]
    fn maestro_label_from_str_ready() {
        assert_eq!(
            MaestroLabel::from_str("maestro:ready"),
            Some(MaestroLabel::Ready)
        );
    }

    #[test]
    fn maestro_label_from_str_in_progress() {
        assert_eq!(
            MaestroLabel::from_str("maestro:in-progress"),
            Some(MaestroLabel::InProgress)
        );
    }

    #[test]
    fn maestro_label_from_str_done() {
        assert_eq!(
            MaestroLabel::from_str("maestro:done"),
            Some(MaestroLabel::Done)
        );
    }

    #[test]
    fn maestro_label_from_str_failed() {
        assert_eq!(
            MaestroLabel::from_str("maestro:failed"),
            Some(MaestroLabel::Failed)
        );
    }

    #[test]
    fn maestro_label_from_str_unknown_returns_none() {
        assert_eq!(MaestroLabel::from_str("bug"), None);
    }

    #[test]
    fn maestro_label_from_str_empty_returns_none() {
        assert_eq!(MaestroLabel::from_str(""), None);
    }

    #[test]
    fn maestro_label_round_trips_as_str_from_str() {
        let variants = [
            MaestroLabel::Ready,
            MaestroLabel::InProgress,
            MaestroLabel::Done,
            MaestroLabel::Failed,
        ];
        for v in variants {
            assert_eq!(MaestroLabel::from_str(v.as_str()), Some(v));
        }
    }

    // SessionMode

    #[test]
    fn session_mode_from_label_orchestrator() {
        assert_eq!(
            SessionMode::from_label("mode:orchestrator"),
            Some(SessionMode::Orchestrator)
        );
    }

    #[test]
    fn session_mode_from_label_vibe() {
        assert_eq!(
            SessionMode::from_label("mode:vibe"),
            Some(SessionMode::Vibe)
        );
    }

    #[test]
    fn session_mode_from_label_unknown_returns_none() {
        assert_eq!(SessionMode::from_label("mode:unknown"), None);
    }

    #[test]
    fn session_mode_from_label_unrelated_label_returns_none() {
        assert_eq!(SessionMode::from_label("bug"), None);
    }

    // GhIssue::priority

    #[test]
    fn issue_priority_p0_from_labels() {
        let issue = make_issue(1, &["priority:P0", "maestro:ready"], "");
        assert_eq!(issue.priority(), Priority::P0);
    }

    #[test]
    fn issue_priority_p1_from_labels() {
        let issue = make_issue(2, &["priority:P1"], "");
        assert_eq!(issue.priority(), Priority::P1);
    }

    #[test]
    fn issue_priority_defaults_to_p2_when_no_priority_label() {
        let issue = make_issue(3, &["maestro:ready", "bug"], "");
        assert_eq!(issue.priority(), Priority::P2);
    }

    #[test]
    fn issue_priority_defaults_to_p2_with_no_labels() {
        let issue = make_issue(4, &[], "");
        assert_eq!(issue.priority(), Priority::P2);
    }

    // GhIssue::session_mode

    #[test]
    fn issue_session_mode_orchestrator() {
        let issue = make_issue(5, &["mode:orchestrator"], "");
        assert_eq!(issue.session_mode(), Some(SessionMode::Orchestrator));
    }

    #[test]
    fn issue_session_mode_vibe() {
        let issue = make_issue(6, &["mode:vibe"], "");
        assert_eq!(issue.session_mode(), Some(SessionMode::Vibe));
    }

    #[test]
    fn issue_session_mode_none_when_no_mode_label() {
        let issue = make_issue(7, &["priority:P0", "bug"], "");
        assert_eq!(issue.session_mode(), None);
    }

    // GhIssue::blocked_by_from_labels

    #[test]
    fn blocked_by_from_labels_single() {
        let issue = make_issue(10, &["blocked-by:#5"], "");
        assert_eq!(issue.blocked_by_from_labels(), vec![5u64]);
    }

    #[test]
    fn blocked_by_from_labels_multiple() {
        let issue = make_issue(10, &["blocked-by:#3", "blocked-by:#7", "maestro:ready"], "");
        let mut result = issue.blocked_by_from_labels();
        result.sort();
        assert_eq!(result, vec![3u64, 7u64]);
    }

    #[test]
    fn blocked_by_from_labels_empty_when_no_blocker_labels() {
        let issue = make_issue(10, &["bug", "priority:P1"], "");
        assert!(issue.blocked_by_from_labels().is_empty());
    }

    #[test]
    fn blocked_by_from_labels_ignores_malformed_label() {
        let issue = make_issue(10, &["blocked-by:5"], "");
        assert!(issue.blocked_by_from_labels().is_empty());
    }

    // GhIssue::blocked_by_from_body

    #[test]
    fn blocked_by_from_body_single_reference() {
        let issue = make_issue(10, &[], "This is blocked-by: #12\nSome other text.");
        assert_eq!(issue.blocked_by_from_body(), vec![12u64]);
    }

    #[test]
    fn blocked_by_from_body_multiple_references() {
        let issue = make_issue(10, &[], "blocked-by: #3\nblocked-by: #8\nDoes some thing.");
        let mut result = issue.blocked_by_from_body();
        result.sort();
        assert_eq!(result, vec![3u64, 8u64]);
    }

    #[test]
    fn blocked_by_from_body_empty_when_no_references() {
        let issue = make_issue(10, &[], "Just a plain description with no blockers.");
        assert!(issue.blocked_by_from_body().is_empty());
    }

    #[test]
    fn blocked_by_from_body_empty_body() {
        let issue = make_issue(10, &[], "");
        assert!(issue.blocked_by_from_body().is_empty());
    }

    #[test]
    fn blocked_by_from_body_case_insensitive() {
        let issue = make_issue(10, &[], "Blocked-By: #99");
        assert_eq!(issue.blocked_by_from_body(), vec![99u64]);
    }

    // GhIssue::all_blockers

    #[test]
    fn all_blockers_union_of_labels_and_body() {
        let issue = make_issue(10, &["blocked-by:#2"], "blocked-by: #5\nblocked-by: #2");
        let result = issue.all_blockers();
        assert_eq!(result, vec![2u64, 5u64]);
    }

    #[test]
    fn all_blockers_empty_when_no_blockers() {
        let issue = make_issue(10, &["bug"], "No blockers here.");
        assert!(issue.all_blockers().is_empty());
    }

    #[test]
    fn all_blockers_no_duplicates() {
        let issue = make_issue(10, &["blocked-by:#7", "blocked-by:#7"], "blocked-by: #7");
        let result = issue.all_blockers();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], 7u64);
    }

    // GhIssue::has_maestro_label

    #[test]
    fn has_maestro_label_returns_true_when_present() {
        let issue = make_issue(10, &["maestro:ready", "bug"], "");
        assert!(issue.has_maestro_label(MaestroLabel::Ready));
    }

    #[test]
    fn has_maestro_label_returns_false_when_absent() {
        let issue = make_issue(10, &["bug"], "");
        assert!(!issue.has_maestro_label(MaestroLabel::InProgress));
    }

    #[test]
    fn has_maestro_label_returns_false_with_no_labels() {
        let issue = make_issue(10, &[], "");
        assert!(!issue.has_maestro_label(MaestroLabel::Done));
    }

    // --- PrReviewEvent ---

    #[test]
    fn pr_review_event_default_is_comment() {
        assert_eq!(PrReviewEvent::default(), PrReviewEvent::Comment);
    }

    #[test]
    fn pr_review_event_approve_as_gh_arg() {
        assert_eq!(PrReviewEvent::Approve.as_gh_arg(), "approve");
    }

    #[test]
    fn pr_review_event_request_changes_as_gh_arg() {
        assert_eq!(PrReviewEvent::RequestChanges.as_gh_arg(), "request-changes");
    }

    #[test]
    fn pr_review_event_comment_as_gh_arg() {
        assert_eq!(PrReviewEvent::Comment.as_gh_arg(), "comment");
    }

    #[test]
    fn pr_review_event_label_approve() {
        assert_eq!(PrReviewEvent::Approve.label(), "Approve");
    }

    #[test]
    fn pr_review_event_label_request_changes() {
        assert_eq!(PrReviewEvent::RequestChanges.label(), "Request Changes");
    }

    #[test]
    fn pr_review_event_label_comment() {
        assert_eq!(PrReviewEvent::Comment.label(), "Comment");
    }

    #[test]
    fn pr_review_event_next_cycles_forward() {
        assert_eq!(PrReviewEvent::Comment.next(), PrReviewEvent::Approve);
        assert_eq!(PrReviewEvent::Approve.next(), PrReviewEvent::RequestChanges);
        assert_eq!(PrReviewEvent::RequestChanges.next(), PrReviewEvent::Comment);
    }

    #[test]
    fn pr_review_event_prev_cycles_backward() {
        assert_eq!(PrReviewEvent::Comment.prev(), PrReviewEvent::RequestChanges);
        assert_eq!(PrReviewEvent::Approve.prev(), PrReviewEvent::Comment);
        assert_eq!(PrReviewEvent::RequestChanges.prev(), PrReviewEvent::Approve);
    }

    // --- GhPullRequest ---

    #[test]
    fn gh_pull_request_round_trips_via_serde() {
        let pr = GhPullRequest {
            number: 42,
            title: "Fix bug".to_string(),
            body: "## Summary\nFixes issue".to_string(),
            state: "open".to_string(),
            html_url: "https://github.com/owner/repo/pull/42".to_string(),
            head_branch: "fix/bug".to_string(),
            base_branch: "main".to_string(),
            author: "bot".to_string(),
            labels: vec!["enhancement".to_string()],
            draft: false,
            mergeable: true,
            additions: 10,
            deletions: 5,
            changed_files: 3,
        };
        let json = serde_json::to_string(&pr).unwrap();
        let rt: GhPullRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.number, 42);
        assert_eq!(rt.title, "Fix bug");
        assert_eq!(rt.head_branch, "fix/bug");
        assert_eq!(rt.additions, 10);
        assert_eq!(rt.changed_files, 3);
    }
}
