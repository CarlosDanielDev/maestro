use async_trait::async_trait;
use std::path::PathBuf;

use super::types::{AnalysisResult, Finding, ScanResult};

/// Trait for AI-powered code smell analysis.
#[async_trait]
pub trait SmellAnalyzer: Send + Sync {
    async fn analyze(
        &self,
        scan: &ScanResult,
        source_files: &[PathBuf],
    ) -> anyhow::Result<AnalysisResult>;
}

/// Production analyzer that spawns Claude CLI.
pub struct ClaudeAnalyzer {
    model: String,
}

impl ClaudeAnalyzer {
    pub fn new(model: String) -> Self {
        Self { model }
    }
}

#[async_trait]
impl SmellAnalyzer for ClaudeAnalyzer {
    async fn analyze(
        &self,
        scan: &ScanResult,
        source_files: &[PathBuf],
    ) -> anyhow::Result<AnalysisResult> {
        let prompt = build_prompt(scan, source_files).await?;

        let output = tokio::process::Command::new("claude")
            .args(["--print", "--output-format", "text", "--model", &self.model])
            .arg(&prompt)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to spawn claude CLI: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("Claude CLI exited with non-zero status: {}", stderr);
        }

        let raw_text = String::from_utf8_lossy(&output.stdout);
        let findings = parse_response(&raw_text);

        Ok(AnalysisResult { findings })
    }
}

const SMELL_CATALOG: &str = r#"You are a code smell analyzer. Analyze the provided source files for these code smells from Fowler's Refactoring catalog:
- feature_envy: A function that accesses another module's data more than its own.
- data_clumps: Groups of variables that appear together in multiple places.
- primitive_obsession: Using primitive types instead of domain-specific types.
- divergent_change: A module that changes for multiple unrelated reasons.
- shotgun_surgery: A single change requires edits across many modules.
- duplicated_code: Identical or near-identical code blocks in multiple locations.

Do NOT report long_method or large_class (already detected by static analysis).
Do NOT report any unused/dead code (already detected by static analysis)."#;

const OUTPUT_INSTRUCTIONS: &str = r#"Respond with ONLY a JSON array of findings. Each finding must match this exact schema:
[
  {
    "severity": "critical" | "warning" | "info",
    "category": "feature_envy" | "data_clumps" | "primitive_obsession" | "divergent_change" | "shotgun_surgery" | "duplicated_code",
    "location": {
      "file": "relative/path.rs",
      "line_start": 42,
      "line_end": 95
    },
    "message": "Human-readable explanation of the smell",
    "dead_lines": 0
  }
]
No markdown, no explanation, no code fences. ONLY the JSON array.
If no smells are found, respond with: []"#;

async fn build_prompt(scan: &ScanResult, source_files: &[PathBuf]) -> anyhow::Result<String> {
    let mut prompt = String::new();

    prompt.push_str(SMELL_CATALOG);
    prompt.push_str("\n\n");

    // Phase 1 context
    if scan.findings.is_empty() {
        prompt.push_str("Phase 1 scanner found no issues.\n\n");
    } else {
        prompt
            .push_str("Phase 1 scanner already found these issues (DO NOT report these again):\n");
        for f in &scan.findings {
            prompt.push_str(&format!(
                "- {}:{}-{}: {:?}\n",
                f.location.file.display(),
                f.location.line_start,
                f.location.line_end,
                f.category
            ));
        }
        prompt.push('\n');
    }

    // Embed source files
    prompt.push_str("Analyze these source files:\n\n");
    for path in source_files {
        match tokio::fs::read_to_string(path).await {
            Ok(contents) => {
                prompt.push_str(&format!("=== FILE: {} ===\n", path.display()));
                prompt.push_str(&contents);
                prompt.push_str("\n=== END FILE ===\n\n");
            }
            Err(e) => {
                tracing::warn!("Failed to read {}: {}", path.display(), e);
            }
        }
    }

    prompt.push_str(OUTPUT_INSTRUCTIONS);

    Ok(prompt)
}

/// Parse Claude's response into findings, with multiple fallback strategies.
pub(crate) fn parse_response(raw_text: &str) -> Vec<Finding> {
    let trimmed = raw_text.trim();

    // Strategy 1: Direct parse
    if let Ok(findings) = serde_json::from_str::<Vec<Finding>>(trimmed) {
        return findings;
    }

    // Strategy 2: Extract from markdown fences
    static FENCE_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?s)```(?:json)?\s*\n?(.*?)```").unwrap());
    let fence_re = &*FENCE_RE;
    if let Some(caps) = fence_re.captures(trimmed)
        && let Ok(findings) = serde_json::from_str::<Vec<Finding>>(&caps[1])
    {
        return findings;
    }

    // Strategy 3: Bracket extraction + Strategy 4: Partial recovery
    if let (Some(start), Some(end)) = (trimmed.find('['), trimmed.rfind(']'))
        && start < end
    {
        if let Ok(findings) = serde_json::from_str::<Vec<Finding>>(&trimmed[start..=end]) {
            return findings;
        }

        if let Ok(values) = serde_json::from_str::<Vec<serde_json::Value>>(&trimmed[start..=end]) {
            let findings: Vec<Finding> = values
                .into_iter()
                .filter_map(|v| serde_json::from_value(v).ok())
                .collect();
            if !findings.is_empty() {
                return findings;
            }
        }
    }

    tracing::error!(
        "Failed to parse analyzer response (first 500 chars): {}",
        &trimmed[..trimmed.len().min(500)]
    );
    Vec::new()
}

/// Mock analyzer that returns canned results (for testing).
#[cfg(test)]
pub struct MockSmellAnalyzer {
    pub result: AnalysisResult,
}

#[cfg(test)]
#[async_trait]
impl SmellAnalyzer for MockSmellAnalyzer {
    async fn analyze(
        &self,
        _scan: &ScanResult,
        _source_files: &[PathBuf],
    ) -> anyhow::Result<AnalysisResult> {
        Ok(self.result.clone())
    }
}

/// Mock analyzer that always fails (for testing error paths).
#[cfg(test)]
pub struct FailingAnalyzer;

#[cfg(test)]
#[async_trait]
impl SmellAnalyzer for FailingAnalyzer {
    async fn analyze(
        &self,
        _scan: &ScanResult,
        _source_files: &[PathBuf],
    ) -> anyhow::Result<AnalysisResult> {
        anyhow::bail!("analyzer unavailable")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sanitize::{Severity, SmellCategory, SourceLocation};

    // -- parse_response tests --

    #[test]
    fn parse_response_valid_json_array() {
        let json = r#"[
            {
                "severity": "warning",
                "category": "feature_envy",
                "location": {"file": "src/foo.rs", "line_start": 10, "line_end": 20},
                "message": "accesses other module data",
                "dead_lines": 0
            }
        ]"#;
        let findings = parse_response(json);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert_eq!(findings[0].category, SmellCategory::FeatureEnvy);
    }

    #[test]
    fn parse_response_fenced_json() {
        let text = r#"Here are the findings:
```json
[
    {
        "severity": "critical",
        "category": "duplicated_code",
        "location": {"file": "src/a.rs", "line_start": 1, "line_end": 50},
        "message": "duplicated block",
        "dead_lines": 0
    }
]
```
That's all."#;
        let findings = parse_response(text);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].category, SmellCategory::DuplicatedCode);
    }

    #[test]
    fn parse_response_bracket_extraction() {
        let text = r#"The analysis found: [
            {
                "severity": "info",
                "category": "primitive_obsession",
                "location": {"file": "src/b.rs", "line_start": 5, "line_end": 5},
                "message": "use domain type",
                "dead_lines": 0
            }
        ] end of results."#;
        let findings = parse_response(text);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].category, SmellCategory::PrimitiveObsession);
    }

    #[test]
    fn parse_response_garbage_returns_empty() {
        let findings = parse_response("this is not json at all");
        assert!(findings.is_empty());
    }

    #[test]
    fn parse_response_empty_array() {
        let findings = parse_response("[]");
        assert!(findings.is_empty());
    }

    #[test]
    fn parse_response_partial_recovery() {
        let json = r#"[
            {
                "severity": "warning",
                "category": "feature_envy",
                "location": {"file": "src/ok.rs", "line_start": 1, "line_end": 10},
                "message": "valid entry",
                "dead_lines": 0
            },
            {"invalid": "entry", "missing": "fields"}
        ]"#;
        let findings = parse_response(json);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].message, "valid entry");
    }

    // -- build_prompt tests --

    #[tokio::test]
    async fn build_prompt_includes_scan_context() {
        let scan = ScanResult {
            findings: vec![Finding {
                severity: Severity::Warning,
                category: SmellCategory::UnusedFunction,
                location: SourceLocation {
                    file: PathBuf::from("src/dead.rs"),
                    line_start: 1,
                    line_end: 10,
                },
                message: "unused".to_string(),
                dead_lines: 10,
            }],
        };
        let prompt = build_prompt(&scan, &[]).await.unwrap();
        assert!(prompt.contains("src/dead.rs"));
        assert!(prompt.contains("DO NOT report these again"));
    }

    #[tokio::test]
    async fn build_prompt_empty_scan_says_no_issues() {
        let scan = ScanResult::default();
        let prompt = build_prompt(&scan, &[]).await.unwrap();
        assert!(prompt.contains("found no issues"));
    }

    #[tokio::test]
    async fn build_prompt_embeds_file_contents() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        std::fs::write(&file_path, "fn main() {}").unwrap();

        let scan = ScanResult::default();
        let prompt = build_prompt(&scan, &[file_path]).await.unwrap();
        assert!(prompt.contains("fn main() {}"));
        assert!(prompt.contains("=== FILE:"));
        assert!(prompt.contains("=== END FILE ==="));
    }

    // -- Mock tests --

    #[tokio::test]
    async fn mock_analyzer_returns_canned_result() {
        let mock = MockSmellAnalyzer {
            result: AnalysisResult {
                findings: vec![Finding {
                    severity: Severity::Info,
                    category: SmellCategory::DataClumps,
                    location: SourceLocation {
                        file: PathBuf::from("test.rs"),
                        line_start: 1,
                        line_end: 1,
                    },
                    message: "data clump".to_string(),
                    dead_lines: 0,
                }],
            },
        };
        let result = mock.analyze(&ScanResult::default(), &[]).await.unwrap();
        assert_eq!(result.findings.len(), 1);
    }

    #[tokio::test]
    async fn failing_analyzer_returns_error() {
        let analyzer = FailingAnalyzer;
        let result = analyzer.analyze(&ScanResult::default(), &[]).await;
        assert!(result.is_err());
    }
}
