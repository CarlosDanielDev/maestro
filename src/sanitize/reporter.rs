use super::{AnalysisResult, Finding, SanitizeReport, ScanResult, Severity, SmellCategory};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Trait for generating sanitize reports in different formats.
pub trait ReportGenerator {
    fn generate(&self, report: &SanitizeReport, min_severity: Severity) -> anyhow::Result<String>;
}

/// Filter findings to only those at or above the minimum severity threshold.
/// Since Severity Ord has Critical < Warning < Info, "at or above" means <= min_severity.
fn filter_findings(findings: Vec<&Finding>, min_severity: Severity) -> Vec<&Finding> {
    findings
        .into_iter()
        .filter(|f| f.severity <= min_severity)
        .collect()
}

fn count_by_severity(findings: &[&Finding]) -> (usize, usize, usize) {
    let critical = findings.iter().filter(|f| f.severity == Severity::Critical).count();
    let warning = findings.iter().filter(|f| f.severity == Severity::Warning).count();
    let info = findings.iter().filter(|f| f.severity == Severity::Info).count();
    (critical, warning, info)
}

fn unique_files(findings: &[&Finding]) -> usize {
    let files: std::collections::HashSet<&PathBuf> =
        findings.iter().map(|f| &f.location.file).collect();
    files.len()
}

fn group_by_category<'a>(findings: &[&'a Finding]) -> BTreeMap<SmellCategory, Vec<&'a Finding>> {
    let mut map: BTreeMap<SmellCategory, Vec<&Finding>> = BTreeMap::new();
    for f in findings {
        map.entry(f.category).or_default().push(f);
    }
    map
}

fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "CRITICAL",
        Severity::Warning => "WARNING",
        Severity::Info => "INFO",
    }
}

// -- Text Reporter --

#[derive(Default)]
pub struct TextReporter;

impl ReportGenerator for TextReporter {
    fn generate(&self, report: &SanitizeReport, min_severity: Severity) -> anyhow::Result<String> {
        let all = report.all_findings();
        let filtered = filter_findings(all, min_severity);

        if filtered.is_empty() {
            return Ok("No issues found.".to_string());
        }

        let (critical, warning, info) = count_by_severity(&filtered);
        let dead_lines = report.total_dead_lines();
        let file_count = unique_files(&filtered);

        let mut out = String::new();
        out.push_str(&format!(
            "Found {} issues ({} critical, {} warnings, {} info) — {} dead lines across {} files\n\n",
            filtered.len(), critical, warning, info, dead_lines, file_count
        ));

        let grouped = group_by_category(&filtered);

        for (cat, group) in &grouped {
            out.push_str(&format!("{:?} ({})\n", cat, group.len()));
            for f in group {
                out.push_str(&format!(
                    "  {} {}:{}-{}: {}\n",
                    severity_label(f.severity),
                    f.location.file.display(),
                    f.location.line_start,
                    f.location.line_end,
                    f.message
                ));
            }
            out.push('\n');
        }

        Ok(out)
    }
}

// -- JSON Reporter --

#[derive(Default)]
pub struct JsonReporter;

impl ReportGenerator for JsonReporter {
    fn generate(&self, report: &SanitizeReport, min_severity: Severity) -> anyhow::Result<String> {
        // Build a filtered report for serialization
        let filtered_report = SanitizeReport {
            scan: ScanResult {
                findings: report
                    .scan
                    .findings
                    .iter()
                    .filter(|f| f.severity <= min_severity)
                    .cloned()
                    .collect(),
            },
            analysis: AnalysisResult {
                findings: report
                    .analysis
                    .findings
                    .iter()
                    .filter(|f| f.severity <= min_severity)
                    .cloned()
                    .collect(),
            },
        };

        Ok(serde_json::to_string_pretty(&filtered_report)?)
    }
}

// -- Markdown Reporter --

#[derive(Default)]
pub struct MarkdownReporter;

impl ReportGenerator for MarkdownReporter {
    fn generate(&self, report: &SanitizeReport, min_severity: Severity) -> anyhow::Result<String> {
        let all = report.all_findings();
        let filtered = filter_findings(all, min_severity);

        if filtered.is_empty() {
            return Ok("# Sanitize Report\n\nNo issues found.\n".to_string());
        }

        let (critical, warning, info) = count_by_severity(&filtered);
        let dead_lines = report.total_dead_lines();

        let mut out = String::new();
        out.push_str("# Sanitize Report\n\n");
        out.push_str("## Summary\n\n");
        out.push_str("| Severity | Count |\n");
        out.push_str("|----------|-------|\n");
        out.push_str(&format!("| CRITICAL | {} |\n", critical));
        out.push_str(&format!("| WARNING | {} |\n", warning));
        out.push_str(&format!("| INFO | {} |\n", info));
        out.push_str(&format!("\n**Total findings:** {} — **Dead lines:** {}\n\n", filtered.len(), dead_lines));

        let grouped = group_by_category(&filtered);

        for (cat, group) in &grouped {
            out.push_str(&format!("## {:?}\n\n", cat));
            for f in group {
                out.push_str(&format!(
                    "- **{}** `{}:{}`: {}\n",
                    severity_label(f.severity),
                    f.location.file.display(),
                    f.location.line_start,
                    f.message
                ));
            }
            out.push('\n');
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sanitize::SourceLocation;

    fn make_finding(severity: Severity, file: &str, dead: u32) -> Finding {
        Finding {
            severity,
            category: SmellCategory::UnusedFunction,
            location: SourceLocation {
                file: PathBuf::from(file),
                line_start: 10,
                line_end: 20,
            },
            message: format!("test message for {file}"),
            dead_lines: dead,
        }
    }

    fn make_report(scan_findings: Vec<Finding>, analysis_findings: Vec<Finding>) -> SanitizeReport {
        SanitizeReport {
            scan: ScanResult {
                findings: scan_findings,
            },
            analysis: AnalysisResult {
                findings: analysis_findings,
            },
        }
    }

    // -- Text format --

    #[test]
    fn text_format_contains_severity_indicators_and_summary() {
        let report = make_report(
            vec![make_finding(Severity::Critical, "src/a.rs", 10)],
            vec![make_finding(Severity::Warning, "src/b.rs", 5)],
        );

        let result = TextReporter::default()
            .generate(&report, Severity::Info)
            .unwrap();

        assert!(result.contains("CRITICAL"));
        assert!(result.contains("WARNING"));
        assert!(result.contains("2"));
        assert!(result.contains("15"));
        assert!(result.contains("src/a.rs"));
    }

    #[test]
    fn text_format_empty_report_shows_no_issues_found() {
        let report = SanitizeReport::default();
        let result = TextReporter::default()
            .generate(&report, Severity::Info)
            .unwrap();
        assert!(result.to_lowercase().contains("no issues found"));
    }

    // -- JSON format --

    #[test]
    fn json_format_valid_json_deserializes_to_sanitize_report() {
        let report = make_report(
            vec![make_finding(Severity::Warning, "src/lib.rs", 3)],
            vec![make_finding(Severity::Critical, "src/main.rs", 0)],
        );

        let result = JsonReporter::default()
            .generate(&report, Severity::Info)
            .unwrap();

        let deserialized: SanitizeReport =
            serde_json::from_str(&result).expect("output must be valid JSON");

        assert_eq!(deserialized.scan.findings.len(), 1);
        assert_eq!(deserialized.analysis.findings.len(), 1);
        assert_eq!(deserialized.scan.findings[0].severity, Severity::Warning);
        assert_eq!(
            deserialized.analysis.findings[0].severity,
            Severity::Critical
        );
    }

    // -- Markdown format --

    #[test]
    fn markdown_format_contains_headers_and_file_line_references() {
        let report = make_report(
            vec![Finding {
                severity: Severity::Critical,
                category: SmellCategory::UnusedFunction,
                location: SourceLocation {
                    file: PathBuf::from("src/foo.rs"),
                    line_start: 42,
                    line_end: 55,
                },
                message: "unused function detected".to_string(),
                dead_lines: 13,
            }],
            vec![],
        );

        let result = MarkdownReporter::default()
            .generate(&report, Severity::Info)
            .unwrap();

        assert!(result.contains('#'));
        assert!(result.contains("src/foo.rs"));
        assert!(result.contains("42"));
        assert!(result.to_lowercase().contains("critical"));
    }

    #[test]
    fn markdown_format_empty_report_shows_no_issues_found() {
        let report = SanitizeReport::default();
        let result = MarkdownReporter::default()
            .generate(&report, Severity::Info)
            .unwrap();
        assert!(result.to_lowercase().contains("no issues found"));
    }

    // -- Severity filtering --

    #[test]
    fn severity_filter_warning_threshold_excludes_info_findings() {
        let report = make_report(
            vec![
                make_finding(Severity::Critical, "src/a.rs", 0),
                make_finding(Severity::Warning, "src/b.rs", 0),
                make_finding(Severity::Info, "src/c.rs", 0),
            ],
            vec![],
        );

        let result = TextReporter::default()
            .generate(&report, Severity::Warning)
            .unwrap();

        assert!(result.contains("src/a.rs"));
        assert!(result.contains("src/b.rs"));
        assert!(!result.contains("src/c.rs"));
    }

    #[test]
    fn severity_filter_critical_only_excludes_all_info_findings() {
        let report = make_report(
            vec![
                make_finding(Severity::Info, "src/x.rs", 0),
                make_finding(Severity::Info, "src/y.rs", 0),
            ],
            vec![],
        );

        let result = TextReporter::default()
            .generate(&report, Severity::Critical)
            .unwrap();

        assert!(!result.contains("src/x.rs"));
        assert!(!result.contains("src/y.rs"));
        assert!(result.to_lowercase().contains("no issues found"));
    }
}
