use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

static BLOCKED_BY_RE: OnceLock<regex::Regex> = OnceLock::new();

fn blocked_by_regex() -> &'static regex::Regex {
    BLOCKED_BY_RE.get_or_init(|| regex::Regex::new(r"(?i)blocked-by:\s*#(\d+)").unwrap())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Priority {
    P0 = 0,
    P1 = 1,
    P2 = 2,
}

impl Default for Priority {
    fn default() -> Self {
        Self::P2
    }
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
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "maestro:ready",
            Self::InProgress => "maestro:in-progress",
            Self::Done => "maestro:done",
            Self::Failed => "maestro:failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "maestro:ready" => Some(Self::Ready),
            "maestro:in-progress" => Some(Self::InProgress),
            "maestro:done" => Some(Self::Done),
            "maestro:failed" => Some(Self::Failed),
            _ => None,
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

    /// Check if this issue has a specific maestro status label.
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
        }
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
}
