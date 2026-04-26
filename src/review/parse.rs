//! Parser for the structured JSON block embedded in `/review` PR comments.
//!
//! Contract: `docs/api-contracts/review-comment.json`. The comment contains
//! a fenced code block:
//!
//! ```text
//! ```json maestro-review
//! { "version": 1, "pr_number": ..., "concerns": [...] }
//! ```
//! ```
//!
//! Anything outside the fence is treated as decorative human prose.

#![deny(clippy::unwrap_used)]
// Reason: Phase 1 foundation for #327. The parser is invoked from the TUI
// review panel that ships in Phase 2; tests exercise it today.
#![allow(dead_code)]

use crate::review::types::ReviewReport;
use regex::Regex;
use std::fmt::Write as _;
use std::sync::LazyLock;

/// Errors raised while extracting and decoding a review report from PR
/// comment markdown. Typed at the seam (RUST-GUARDRAILS §2) so callers can
/// distinguish "no report present" from "malformed JSON".
#[derive(Debug, PartialEq)]
pub enum ParseError {
    /// No `json maestro-review` fence was found in the input.
    MissingFence,
    /// The fence was found but JSON deserialization failed.
    MalformedJson(String),
    /// The fenced block decoded but the schema version was unsupported.
    UnsupportedVersion { found: u8, supported: u8 },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingFence => write!(
                f,
                "no ```json maestro-review fenced block found in PR comment"
            ),
            Self::MalformedJson(msg) => write!(f, "malformed JSON in review fence: {msg}"),
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "unsupported review report version {found} (parser supports {supported})"
            ),
        }
    }
}

impl std::error::Error for ParseError {}

// `(?s)` makes `.` cross newlines so the JSON body can be multi-line.
// INVARIANT: the literal pattern is verified by the unit tests below; if it
// were ever malformed the test suite would fail before shipping. Same
// `LazyLock<Regex>` shape as `src/util/validation.rs`.
#[allow(clippy::expect_used)]
static FENCE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)```json\s+maestro-review\s*\n(.*?)\n```").expect("infallible regex literal")
});

/// Parse a `/review` PR comment body into a structured `ReviewReport`.
pub fn parse_review_comment(body: &str) -> Result<ReviewReport, ParseError> {
    let captures = FENCE_RE.captures(body).ok_or(ParseError::MissingFence)?;
    let json = captures.get(1).ok_or(ParseError::MissingFence)?.as_str();

    let report: ReviewReport =
        serde_json::from_str(json).map_err(|e| ParseError::MalformedJson(e.to_string()))?;

    if report.version != ReviewReport::SCHEMA_VERSION {
        return Err(ParseError::UnsupportedVersion {
            found: report.version,
            supported: ReviewReport::SCHEMA_VERSION,
        });
    }

    Ok(report)
}

/// Render a `ReviewReport` into a markdown PR comment body. Pure inverse
/// of `parse_review_comment` — round-trips losslessly.
pub fn render_review_comment(report: &ReviewReport) -> String {
    let (critical, warning, suggestion) = report.severity_counts();
    let total = report.concerns.len();
    let json = serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string());

    let mut out = String::new();
    // `write!` into String is infallible; the underlying `fmt::Write`
    // impl never returns Err.
    let _ = write!(out, "## Review by `{}`\n\n", report.reviewer);
    let _ = write!(
        out,
        "**{total} concern(s)** ({critical} critical, {warning} warning, {suggestion} suggestion)\n\n"
    );
    for c in &report.concerns {
        match c.line {
            Some(l) => {
                let _ = writeln!(
                    out,
                    "- **[{}]** `{}:{l}` — {}",
                    c.severity.label(),
                    c.file.display(),
                    c.message
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    "- **[{}]** `{}` — {}",
                    c.severity.label(),
                    c.file.display(),
                    c.message
                );
            }
        }
    }
    out.push_str("\n```json maestro-review\n");
    out.push_str(&json);
    out.push_str("\n```\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::types::{Concern, ConcernId, ConcernStatus, PrNumber, Severity};
    use std::path::PathBuf;

    fn sample_report() -> ReviewReport {
        let mut r = ReviewReport::new(PrNumber(42), "claude");
        r.concerns.push(Concern {
            id: ConcernId::new(),
            severity: Severity::Critical,
            file: PathBuf::from("src/auth.rs"),
            line: Some(84),
            message: "Constant-time compare missing".into(),
            suggested_diff: Some("@@ -1 +1 @@\n-bad\n+good".into()),
            status: ConcernStatus::Pending,
        });
        r.concerns.push(Concern {
            id: ConcernId::new(),
            severity: Severity::Warning,
            file: PathBuf::from("src/lib.rs"),
            line: None,
            message: "Module docs missing".into(),
            suggested_diff: None,
            status: ConcernStatus::Pending,
        });
        r
    }

    #[test]
    fn parse_well_formed_comment_returns_report() {
        let body = render_review_comment(&sample_report());
        let parsed = parse_review_comment(&body).expect("parse");
        assert_eq!(parsed.concerns.len(), 2);
        assert_eq!(parsed.pr_number, PrNumber(42));
        assert_eq!(parsed.reviewer, "claude");
    }

    #[test]
    fn parse_missing_fence_returns_err() {
        let body = "Just some PR comment text without any fenced block.";
        assert_eq!(parse_review_comment(body), Err(ParseError::MissingFence));
    }

    #[test]
    fn parse_malformed_json_returns_err() {
        let body = "```json maestro-review\n{not valid json}\n```";
        match parse_review_comment(body) {
            Err(ParseError::MalformedJson(_)) => (),
            other => panic!("expected MalformedJson, got {other:?}"),
        }
    }

    #[test]
    fn parse_empty_concerns_array_is_ok() {
        let body = r#"```json maestro-review
{"version":1,"pr_number":1,"reviewer":"x","concerns":[]}
```"#;
        let r = parse_review_comment(body).expect("parse");
        assert!(r.concerns.is_empty());
    }

    #[test]
    fn parse_unknown_severity_returns_malformed_json() {
        let id = uuid::Uuid::new_v4();
        let body = format!(
            r#"```json maestro-review
{{"version":1,"pr_number":1,"reviewer":"x","concerns":[{{"id":"{id}","severity":"meh","file":"a.rs","message":"m"}}]}}
```"#
        );
        match parse_review_comment(&body) {
            Err(ParseError::MalformedJson(_)) => (),
            other => panic!("expected MalformedJson, got {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_unsupported_version() {
        let body = r#"```json maestro-review
{"version":99,"pr_number":1,"reviewer":"x","concerns":[]}
```"#;
        match parse_review_comment(body) {
            Err(ParseError::UnsupportedVersion {
                found: 99,
                supported: 1,
            }) => (),
            other => panic!("expected UnsupportedVersion, got {other:?}"),
        }
    }

    #[test]
    fn round_trip_render_then_parse_is_lossless() {
        let original = sample_report();
        let rendered = render_review_comment(&original);
        let parsed = parse_review_comment(&rendered).expect("round-trip");
        assert_eq!(parsed, original);
    }

    #[test]
    fn render_includes_human_summary_outside_fence() {
        let body = render_review_comment(&sample_report());
        assert!(body.contains("## Review by `claude`"));
        assert!(body.contains("(1 critical, 1 warning, 0 suggestion)"));
    }

    #[test]
    fn parse_ignores_prose_around_fence() {
        let body = format!(
            "Some intro text.\n\n{}\n\nTrailing prose.\n",
            render_review_comment(&sample_report())
        );
        let parsed = parse_review_comment(&body).expect("parse");
        assert_eq!(parsed.concerns.len(), 2);
    }
}
