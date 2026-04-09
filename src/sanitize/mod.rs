mod analyzer;
mod config;
pub mod reporter;
mod scanner;
pub mod screen;
mod types;

use analyzer::ClaudeAnalyzer;
pub use analyzer::SmellAnalyzer;
pub use config::{OutputFormat, SanitizeConfig};
pub use scanner::{CodeScanner, RustScanner};
pub use types::{AnalysisResult, Finding, SanitizeReport, ScanResult, Severity, SmellCategory};
#[cfg(test)]
pub use types::SourceLocation;

pub async fn cmd_sanitize(config: SanitizeConfig) -> anyhow::Result<()> {
    use reporter::{JsonReporter, MarkdownReporter, ReportGenerator, TextReporter};

    // Phase 1: Scan
    eprintln!("Phase 1: Scanning...");
    let scanner = RustScanner::new();
    let scan_result = scanner.scan(&config.path).await?;
    eprintln!("  Found {} issues in Phase 1", scan_result.findings.len());

    // Phase 2: AI Analysis (optional)
    let analysis_result = if config.skip_ai {
        eprintln!("Phase 2: Skipped (--skip-ai)");
        AnalysisResult::default()
    } else {
        eprintln!("Phase 2: Analyzing with Claude...");
        let model = config.model.as_deref().unwrap_or("sonnet");
        let analyzer = ClaudeAnalyzer::new(model.to_string());

        let source_files = types::collect_rs_files(&config.path);

        match analyzer.analyze(&scan_result, &source_files).await {
            Ok(result) => {
                eprintln!("  Found {} issues in Phase 2", result.findings.len());
                result
            }
            Err(e) => {
                eprintln!(
                    "  Phase 2 failed: {}. Continuing with scan-only results.",
                    e
                );
                AnalysisResult::default()
            }
        }
    };

    // Phase 3: Generate report
    eprintln!("Phase 3: Generating report...");
    let report = SanitizeReport {
        scan: scan_result,
        analysis: analysis_result,
    };

    let output = match config.output {
        OutputFormat::Text => TextReporter.generate(&report, config.severity)?,
        OutputFormat::Json => JsonReporter.generate(&report, config.severity)?,
        OutputFormat::Markdown => MarkdownReporter.generate(&report, config.severity)?,
    };

    // Print report to stdout (progress messages went to stderr)
    println!("{}", output);

    Ok(())
}

#[cfg(test)]
mod pipeline_tests {
    use super::*;

    #[tokio::test]
    async fn cmd_sanitize_skip_ai_runs_scan_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let config = SanitizeConfig {
            path: dir.path().to_path_buf(),
            output: OutputFormat::Text,
            severity: Severity::Info,
            skip_ai: true,
            model: None,
        };

        let result = cmd_sanitize(config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn cmd_sanitize_json_output() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let config = SanitizeConfig {
            path: dir.path().to_path_buf(),
            output: OutputFormat::Json,
            severity: Severity::Info,
            skip_ai: true,
            model: None,
        };

        let result = cmd_sanitize(config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn cmd_sanitize_markdown_output() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let config = SanitizeConfig {
            path: dir.path().to_path_buf(),
            output: OutputFormat::Markdown,
            severity: Severity::Info,
            skip_ai: true,
            model: None,
        };

        let result = cmd_sanitize(config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn cmd_sanitize_severity_filtering() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let config = SanitizeConfig {
            path: dir.path().to_path_buf(),
            output: OutputFormat::Text,
            severity: Severity::Critical,
            skip_ai: true,
            model: None,
        };

        let result = cmd_sanitize(config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn cmd_sanitize_default_config_works() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let mut config = SanitizeConfig::default();
        config.path = dir.path().to_path_buf();
        config.skip_ai = true;

        let result = cmd_sanitize(config).await;
        assert!(result.is_ok());
    }
}
