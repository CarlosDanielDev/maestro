use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Severity level for findings. Custom Ord: Critical < Warning < Info
/// so that Critical sorts first in ascending order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

impl Severity {
    fn rank(self) -> u8 {
        match self {
            Self::Critical => 0,
            Self::Warning => 1,
            Self::Info => 2,
        }
    }
}

impl Ord for Severity {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.rank().cmp(&other.rank())
    }
}

impl PartialOrd for Severity {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Categories of code smells and dead code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmellCategory {
    // Dead-code variants
    UnusedFunction,
    UnusedStruct,
    UnusedEnum,
    UnusedImport,
    UnusedModule,
    UnusedFile,
    // Fowler catalog smells
    LongMethod,
    LargeClass,
    FeatureEnvy,
    DataClumps,
    PrimitiveObsession,
    DivergentChange,
    ShotgunSurgery,
    DuplicatedCode,
}

impl SmellCategory {
    #[allow(dead_code)] // Reason: category predicate — used in sanitize filtering
    pub fn is_dead_code(self) -> bool {
        matches!(
            self,
            Self::UnusedFunction
                | Self::UnusedStruct
                | Self::UnusedEnum
                | Self::UnusedImport
                | Self::UnusedModule
                | Self::UnusedFile
        )
    }
}

/// Location in source code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: PathBuf,
    pub line_start: u32,
    pub line_end: u32,
}

/// A single finding from scanning or analysis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    pub category: SmellCategory,
    pub location: SourceLocation,
    pub message: String,
    pub dead_lines: u32,
}

/// Phase 1 scan output.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanResult {
    pub findings: Vec<Finding>,
}

/// Phase 2 AI analysis output.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub findings: Vec<Finding>,
}

/// Combined report from all phases.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SanitizeReport {
    pub scan: ScanResult,
    pub analysis: AnalysisResult,
}

impl SanitizeReport {
    /// Merge all findings from both phases, sorted by severity (Critical first)
    /// then by file path.
    pub fn all_findings(&self) -> Vec<&Finding> {
        let mut combined: Vec<&Finding> = self
            .scan
            .findings
            .iter()
            .chain(self.analysis.findings.iter())
            .collect();
        combined.sort_by(|a, b| {
            a.severity
                .cmp(&b.severity)
                .then_with(|| a.location.file.cmp(&b.location.file))
        });
        combined
    }

    /// Total dead lines across all findings in both phases.
    pub fn total_dead_lines(&self) -> u32 {
        self.scan
            .findings
            .iter()
            .chain(self.analysis.findings.iter())
            .map(|f| f.dead_lines)
            .sum()
    }
}

/// Collect all `.rs` files under a directory.
pub fn collect_rs_files(root: &std::path::Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs") && e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(file: &str) -> SourceLocation {
        SourceLocation {
            file: PathBuf::from(file),
            line_start: 1,
            line_end: 1,
        }
    }

    fn finding(severity: Severity, file: &str, dead: u32) -> Finding {
        Finding {
            severity,
            category: SmellCategory::LongMethod,
            location: loc(file),
            message: "test".to_string(),
            dead_lines: dead,
        }
    }

    // -- Severity ordering --

    #[test]
    fn severity_critical_less_than_warning() {
        assert!(Severity::Critical < Severity::Warning);
    }

    #[test]
    fn severity_warning_less_than_info() {
        assert!(Severity::Warning < Severity::Info);
    }

    #[test]
    fn severity_critical_sorts_first() {
        let mut v = vec![Severity::Info, Severity::Critical, Severity::Warning];
        v.sort();
        assert_eq!(
            v,
            vec![Severity::Critical, Severity::Warning, Severity::Info]
        );
    }

    #[test]
    fn severity_equal_variants_compare_equal() {
        assert_eq!(Severity::Warning, Severity::Warning);
    }

    // -- Severity serde --

    #[test]
    fn severity_serde_roundtrip() {
        for s in [Severity::Info, Severity::Warning, Severity::Critical] {
            let json = serde_json::to_string(&s).unwrap();
            let rt: Severity = serde_json::from_str(&json).unwrap();
            assert_eq!(rt, s);
        }
    }

    // -- SmellCategory::is_dead_code --

    #[test]
    fn is_dead_code_true_for_unused_variants() {
        assert!(SmellCategory::UnusedFunction.is_dead_code());
        assert!(SmellCategory::UnusedStruct.is_dead_code());
        assert!(SmellCategory::UnusedEnum.is_dead_code());
        assert!(SmellCategory::UnusedImport.is_dead_code());
        assert!(SmellCategory::UnusedModule.is_dead_code());
        assert!(SmellCategory::UnusedFile.is_dead_code());
    }

    #[test]
    fn is_dead_code_false_for_fowler_smells() {
        assert!(!SmellCategory::LongMethod.is_dead_code());
        assert!(!SmellCategory::LargeClass.is_dead_code());
        assert!(!SmellCategory::FeatureEnvy.is_dead_code());
        assert!(!SmellCategory::DataClumps.is_dead_code());
        assert!(!SmellCategory::PrimitiveObsession.is_dead_code());
        assert!(!SmellCategory::DivergentChange.is_dead_code());
        assert!(!SmellCategory::ShotgunSurgery.is_dead_code());
        assert!(!SmellCategory::DuplicatedCode.is_dead_code());
    }

    // -- SmellCategory serde --

    #[test]
    fn smell_category_serde_roundtrip() {
        let variants = [
            SmellCategory::UnusedFunction,
            SmellCategory::LongMethod,
            SmellCategory::DuplicatedCode,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let rt: SmellCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(rt, v);
        }
    }

    // -- SourceLocation serde --

    #[test]
    fn source_location_serde_roundtrip() {
        let loc = SourceLocation {
            file: PathBuf::from("src/main.rs"),
            line_start: 42,
            line_end: 50,
        };
        let json = serde_json::to_string(&loc).unwrap();
        let rt: SourceLocation = serde_json::from_str(&json).unwrap();
        assert_eq!(rt, loc);
    }

    // -- Finding serde --

    #[test]
    fn finding_serde_roundtrip() {
        let f = Finding {
            severity: Severity::Warning,
            category: SmellCategory::LongMethod,
            location: loc("src/foo.rs"),
            message: "method too long".to_string(),
            dead_lines: 0,
        };
        let json = serde_json::to_string(&f).unwrap();
        let rt: Finding = serde_json::from_str(&json).unwrap();
        assert_eq!(rt, f);
    }

    // -- ScanResult / AnalysisResult serde --

    #[test]
    fn scan_result_serde_roundtrip() {
        let sr = ScanResult { findings: vec![] };
        let json = serde_json::to_string(&sr).unwrap();
        let rt: ScanResult = serde_json::from_str(&json).unwrap();
        assert!(rt.findings.is_empty());
    }

    #[test]
    fn analysis_result_serde_roundtrip() {
        let ar = AnalysisResult { findings: vec![] };
        let json = serde_json::to_string(&ar).unwrap();
        let rt: AnalysisResult = serde_json::from_str(&json).unwrap();
        assert!(rt.findings.is_empty());
    }

    // -- SanitizeReport --

    #[test]
    fn all_findings_empty_report() {
        let report = SanitizeReport::default();
        assert!(report.all_findings().is_empty());
    }

    #[test]
    fn all_findings_scan_only() {
        let report = SanitizeReport {
            scan: ScanResult {
                findings: vec![finding(Severity::Info, "src/a.rs", 0)],
            },
            analysis: AnalysisResult::default(),
        };
        assert_eq!(report.all_findings().len(), 1);
    }

    #[test]
    fn all_findings_combines_scan_and_analysis() {
        let report = SanitizeReport {
            scan: ScanResult {
                findings: vec![finding(Severity::Warning, "src/b.rs", 0)],
            },
            analysis: AnalysisResult {
                findings: vec![finding(Severity::Critical, "src/a.rs", 0)],
            },
        };
        assert_eq!(report.all_findings().len(), 2);
    }

    #[test]
    fn all_findings_sorted_by_severity_then_file() {
        let report = SanitizeReport {
            scan: ScanResult {
                findings: vec![
                    finding(Severity::Info, "src/z.rs", 0),
                    finding(Severity::Critical, "src/z.rs", 0),
                    finding(Severity::Warning, "src/a.rs", 0),
                    finding(Severity::Warning, "src/z.rs", 0),
                ],
            },
            analysis: AnalysisResult::default(),
        };
        let sorted = report.all_findings();
        assert_eq!(sorted[0].severity, Severity::Critical);
        assert_eq!(sorted[1].severity, Severity::Warning);
        assert_eq!(sorted[1].location.file, PathBuf::from("src/a.rs"));
        assert_eq!(sorted[2].severity, Severity::Warning);
        assert_eq!(sorted[2].location.file, PathBuf::from("src/z.rs"));
        assert_eq!(sorted[3].severity, Severity::Info);
    }

    #[test]
    fn all_findings_mixed_sources_sorted() {
        let report = SanitizeReport {
            scan: ScanResult {
                findings: vec![finding(Severity::Info, "src/c.rs", 0)],
            },
            analysis: AnalysisResult {
                findings: vec![finding(Severity::Critical, "src/b.rs", 0)],
            },
        };
        let sorted = report.all_findings();
        assert_eq!(sorted[0].severity, Severity::Critical);
        assert_eq!(sorted[1].severity, Severity::Info);
    }

    #[test]
    fn total_dead_lines_sums_correctly() {
        let report = SanitizeReport {
            scan: ScanResult {
                findings: vec![
                    finding(Severity::Warning, "a.rs", 10),
                    finding(Severity::Info, "b.rs", 20),
                ],
            },
            analysis: AnalysisResult {
                findings: vec![finding(Severity::Critical, "c.rs", 5)],
            },
        };
        assert_eq!(report.total_dead_lines(), 35);
    }

    #[test]
    fn total_dead_lines_zero_when_empty() {
        let report = SanitizeReport::default();
        assert_eq!(report.total_dead_lines(), 0);
    }

    #[test]
    fn sanitize_report_serde_roundtrip() {
        let report = SanitizeReport {
            scan: ScanResult {
                findings: vec![finding(Severity::Warning, "src/x.rs", 10)],
            },
            analysis: AnalysisResult {
                findings: vec![finding(Severity::Critical, "src/y.rs", 5)],
            },
        };
        let json = serde_json::to_string(&report).unwrap();
        let rt: SanitizeReport = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.scan.findings.len(), 1);
        assert_eq!(rt.analysis.findings.len(), 1);
    }
}
