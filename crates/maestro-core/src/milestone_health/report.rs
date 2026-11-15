//! Aggregated health report for one milestone (#500).

use crate::milestone_health::types::{DorResult, GraphAnomaly};

#[derive(Debug, Clone, Default)]
pub struct HealthReport {
    pub dor: Vec<DorResult>,
    pub anomalies: Vec<GraphAnomaly>,
}

impl HealthReport {
    pub fn ready_count(&self) -> usize {
        self.dor.iter().filter(|r| r.passed()).count()
    }

    pub fn not_ready_count(&self) -> usize {
        self.dor.iter().filter(|r| !r.passed()).count()
    }

    pub fn anomaly_count(&self) -> usize {
        self.anomalies.len()
    }

    pub fn summary_line(&self) -> String {
        format!(
            "{} issues ready, {} issues not ready, {} graph anomalies found.",
            self.ready_count(),
            self.not_ready_count(),
            self.anomaly_count()
        )
    }

    pub fn is_healthy(&self) -> bool {
        self.not_ready_count() == 0 && self.anomaly_count() == 0 && !self.dor.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::milestone_health::types::{IssueType, MissingField};

    fn passing(num: u64) -> DorResult {
        DorResult {
            issue_number: num,
            issue_type: IssueType::Feature,
            missing: vec![],
        }
    }

    fn failing(num: u64) -> DorResult {
        DorResult {
            issue_number: num,
            issue_type: IssueType::Feature,
            missing: vec![MissingField::Section("Blocked By")],
        }
    }

    // D-1
    #[test]
    fn ready_count_counts_passed_results() {
        let report = HealthReport {
            dor: vec![passing(1), passing(2), failing(3)],
            anomalies: vec![],
        };
        assert_eq!(report.ready_count(), 2);
    }

    // D-2
    #[test]
    fn not_ready_count_counts_failed_results() {
        let report = HealthReport {
            dor: vec![passing(1), passing(2), failing(3)],
            anomalies: vec![],
        };
        assert_eq!(report.not_ready_count(), 1);
    }

    // D-3
    #[test]
    fn anomaly_count_reflects_vec_length() {
        let report = HealthReport {
            dor: vec![],
            anomalies: vec![
                GraphAnomaly::MissingDependencyGraphSection,
                GraphAnomaly::MissingSequenceLine,
            ],
        };
        assert_eq!(report.anomaly_count(), 2);
    }

    // D-4
    #[test]
    fn summary_line_exact_format_all_ready_no_anomalies() {
        let report = HealthReport {
            dor: vec![passing(1), passing(2), passing(3)],
            anomalies: vec![],
        };
        assert_eq!(
            report.summary_line(),
            "3 issues ready, 0 issues not ready, 0 graph anomalies found."
        );
    }

    // D-5
    #[test]
    fn summary_line_exact_format_mixed() {
        let report = HealthReport {
            dor: vec![passing(1), passing(2), failing(3), failing(4), failing(5)],
            anomalies: vec![GraphAnomaly::MissingSequenceLine],
        };
        assert_eq!(
            report.summary_line(),
            "2 issues ready, 3 issues not ready, 1 graph anomalies found."
        );
    }

    // D-6
    #[test]
    fn is_healthy_true_when_all_pass_and_no_anomalies() {
        let report = HealthReport {
            dor: vec![passing(1), passing(2)],
            anomalies: vec![],
        };
        assert!(report.is_healthy());
    }

    // D-7
    #[test]
    fn is_healthy_false_when_any_fail() {
        let report = HealthReport {
            dor: vec![passing(1), failing(2)],
            anomalies: vec![],
        };
        assert!(!report.is_healthy());
    }

    // D-8
    #[test]
    fn is_healthy_false_when_anomalies_present_even_if_all_dor_pass() {
        let report = HealthReport {
            dor: vec![passing(1), passing(2), passing(3)],
            anomalies: vec![GraphAnomaly::MissingSequenceLine],
        };
        assert!(!report.is_healthy());
    }

    // D-9
    #[test]
    fn is_healthy_false_empty_issues_with_anomalies() {
        let report = HealthReport {
            dor: vec![],
            anomalies: vec![GraphAnomaly::MissingDependencyGraphSection],
        };
        assert!(!report.is_healthy());
    }
}
