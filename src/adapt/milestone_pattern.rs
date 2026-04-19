//! Detect the dominant milestone-naming pattern on a GitHub repo so the
//! `adapt` planner emits titles that match existing conventions instead of
//! diverging into an `M0/M1/M2` scheme the user doesn't use.
//!
//! Pure logic, no I/O. Fed a list of milestone titles, returns the best
//! guess at the pattern plus a natural-language hint suitable for
//! inclusion in the Claude planning prompt.

use regex::Regex;
use std::sync::OnceLock;

/// Detected milestone-naming pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MilestonePattern {
    /// Semantic version — `v1.2.3`, `v0.14.0`, etc.
    Semver,
    /// Short-form `M0`, `M1:`, `Mn — Title`, etc. (what adapt emitted previously).
    MStyle,
    /// Quarterly or sprint-style cadence (e.g. `Q2-2026`, `Sprint 42`).
    Cadence,
    /// None of the recognized patterns dominated.
    Unknown,
}

/// Threshold (fraction, 0.0–1.0) above which a pattern is considered dominant.
const DOMINANCE_THRESHOLD: f64 = 0.6;

fn semver_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^v\d+\.\d+\.\d+$").unwrap())
}

fn mstyle_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^M\d+(\b|:|\s|$)").unwrap())
}

fn cadence_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(q[1-4][-\s]\d{4}|sprint[-\s]\d+)").unwrap())
}

fn classify(title: &str) -> Option<MilestonePattern> {
    let t = title.trim();
    if semver_re().is_match(t) {
        return Some(MilestonePattern::Semver);
    }
    if mstyle_re().is_match(t) {
        return Some(MilestonePattern::MStyle);
    }
    if cadence_re().is_match(t) {
        return Some(MilestonePattern::Cadence);
    }
    None
}

/// Inspect milestone titles and return the dominant pattern, or `Unknown`
/// if no pattern exceeds [`DOMINANCE_THRESHOLD`] or `titles` is empty.
pub fn detect_pattern(titles: &[&str]) -> MilestonePattern {
    if titles.is_empty() {
        return MilestonePattern::Unknown;
    }

    let mut semver = 0usize;
    let mut mstyle = 0usize;
    let mut cadence = 0usize;
    let mut classified = 0usize;

    for title in titles {
        match classify(title) {
            Some(MilestonePattern::Semver) => {
                semver += 1;
                classified += 1;
            }
            Some(MilestonePattern::MStyle) => {
                mstyle += 1;
                classified += 1;
            }
            Some(MilestonePattern::Cadence) => {
                cadence += 1;
                classified += 1;
            }
            _ => {}
        }
    }

    if classified == 0 {
        return MilestonePattern::Unknown;
    }

    let total = titles.len() as f64;
    let top = [
        (semver, MilestonePattern::Semver),
        (mstyle, MilestonePattern::MStyle),
        (cadence, MilestonePattern::Cadence),
    ]
    .into_iter()
    .max_by_key(|(n, _)| *n)
    .unwrap();

    if (top.0 as f64) / total >= DOMINANCE_THRESHOLD {
        top.1
    } else {
        MilestonePattern::Unknown
    }
}

/// Extract the highest semver title (by lexicographic compare on the
/// `(major, minor, patch)` tuple) and return a natural-language "next slot"
/// suggestion for use in the planner prompt.
pub fn next_semver_slot(titles: &[&str]) -> Option<String> {
    let re = semver_re();
    let mut versions: Vec<(u32, u32, u32)> = titles
        .iter()
        .filter_map(|t| {
            let t = t.trim();
            if !re.is_match(t) {
                return None;
            }
            let rest = &t[1..];
            let parts: Vec<&str> = rest.split('.').collect();
            if parts.len() != 3 {
                return None;
            }
            Some((
                parts[0].parse().ok()?,
                parts[1].parse().ok()?,
                parts[2].parse().ok()?,
            ))
        })
        .collect();

    if versions.is_empty() {
        return None;
    }
    versions.sort();
    let (major, minor, _) = *versions.last().unwrap();
    Some(format!("v{}.{}.0", major, minor + 1))
}

/// Build the natural-language hint passed to the Claude planner.
/// Returns `None` when no useful hint can be derived (falls back to default behavior).
pub fn build_planner_hint(titles: &[&str]) -> Option<String> {
    let pattern = detect_pattern(titles);
    match pattern {
        MilestonePattern::Semver => {
            let example = next_semver_slot(titles).unwrap_or_else(|| "v0.1.0".to_string());
            Some(format!(
                "The project uses semantic versioning for milestone titles (e.g. `v0.14.0`, `v1.0.0`). \
                 Every new milestone title MUST match the regex `^v\\d+\\.\\d+\\.\\d+$`. \
                 Suggested next slot: `{}`. Do NOT emit titles like `M0:`, `Phase 1`, or `Foundation`. \
                 Put the descriptive name in the milestone description, not the title.",
                example
            ))
        }
        MilestonePattern::MStyle => Some(
            "The project uses short-form milestone titles prefixed with `M<number>:` (e.g. `M0: Foundation`). \
             Match this convention for new milestones."
                .to_string(),
        ),
        MilestonePattern::Cadence => Some(
            "The project uses a cadence-based milestone naming (e.g. `Q2-2026`, `Sprint 42`). \
             Match this convention; infer the next slot from the highest existing."
                .to_string(),
        ),
        MilestonePattern::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_titles_returns_unknown() {
        assert_eq!(detect_pattern(&[]), MilestonePattern::Unknown);
    }

    #[test]
    fn detects_semver_when_all_titles_are_semver() {
        let titles = vec!["v0.1.0", "v0.2.0", "v0.14.0", "v1.0.0"];
        assert_eq!(detect_pattern(&titles), MilestonePattern::Semver);
    }

    #[test]
    fn detects_semver_when_mostly_semver_above_threshold() {
        // 5/6 = 83% semver, above the 60% threshold
        let titles = vec!["v0.1.0", "v0.2.0", "v0.3.0", "v0.4.0", "v0.5.0", "M0: odd"];
        assert_eq!(detect_pattern(&titles), MilestonePattern::Semver);
    }

    #[test]
    fn detects_mstyle_when_all_titles_are_m_style() {
        let titles = vec!["M0: Foundation", "M1: Core", "M2 — Testing", "M3: Polish"];
        assert_eq!(detect_pattern(&titles), MilestonePattern::MStyle);
    }

    #[test]
    fn detects_cadence_for_quarter_style() {
        let titles = vec!["Q1-2026", "Q2 2026", "Q3-2026"];
        assert_eq!(detect_pattern(&titles), MilestonePattern::Cadence);
    }

    #[test]
    fn detects_cadence_for_sprint_style() {
        let titles = vec!["Sprint 40", "Sprint 41", "sprint-42"];
        assert_eq!(detect_pattern(&titles), MilestonePattern::Cadence);
    }

    #[test]
    fn returns_unknown_when_no_titles_classify() {
        let titles = vec!["Release A", "Release B"];
        assert_eq!(detect_pattern(&titles), MilestonePattern::Unknown);
    }

    #[test]
    fn returns_unknown_when_no_pattern_dominates() {
        // 50/50 split — below the 60% threshold
        let titles = vec!["v0.1.0", "v0.2.0", "M0: foo", "M1: bar"];
        assert_eq!(detect_pattern(&titles), MilestonePattern::Unknown);
    }

    #[test]
    fn detects_semver_with_v_prefix_only_not_raw_numbers() {
        // Raw "1.0.0" (no v prefix) should NOT match
        let titles = vec!["1.0.0", "2.0.0", "3.0.0"];
        assert_eq!(detect_pattern(&titles), MilestonePattern::Unknown);
    }

    #[test]
    fn detects_semver_ignores_suffixes() {
        // "v0.14.0-rc1" isn't strict semver per our regex; should not classify
        let titles = vec!["v0.14.0-rc1", "v0.15.0-beta", "v0.1.0"];
        // Only 1/3 classifies — below threshold
        assert_eq!(detect_pattern(&titles), MilestonePattern::Unknown);
    }

    // -- next_semver_slot --

    #[test]
    fn next_semver_slot_picks_max_version() {
        let titles = vec!["v0.1.0", "v0.14.0", "v0.5.0"];
        assert_eq!(next_semver_slot(&titles), Some("v0.15.0".to_string()));
    }

    #[test]
    fn next_semver_slot_ignores_non_semver_titles() {
        let titles = vec!["v0.14.0", "M0: foo", "not a version"];
        assert_eq!(next_semver_slot(&titles), Some("v0.15.0".to_string()));
    }

    #[test]
    fn next_semver_slot_returns_none_when_no_semver_present() {
        let titles = vec!["M0: foo"];
        assert_eq!(next_semver_slot(&titles), None);
    }

    #[test]
    fn next_semver_slot_considers_major_bump() {
        let titles = vec!["v0.21.0", "v1.0.0"];
        assert_eq!(next_semver_slot(&titles), Some("v1.1.0".to_string()));
    }

    // -- build_planner_hint --

    #[test]
    fn hint_for_semver_project_mentions_regex_and_next_slot() {
        let titles = vec!["v0.1.0", "v0.13.0", "v0.14.0"];
        let hint = build_planner_hint(&titles).expect("semver project should produce a hint");
        assert!(hint.contains("semantic versioning"));
        assert!(hint.contains(r"^v\d+\.\d+\.\d+$"));
        assert!(hint.contains("v0.15.0"));
        assert!(hint.to_lowercase().contains("do not emit titles like"));
    }

    #[test]
    fn hint_for_mstyle_project_mentions_m_prefix() {
        let titles = vec!["M0: Foo", "M1: Bar", "M2: Baz"];
        let hint = build_planner_hint(&titles).expect("m-style project should produce a hint");
        assert!(hint.contains("M<number>"));
    }

    #[test]
    fn hint_returns_none_for_empty_titles() {
        assert_eq!(build_planner_hint(&[]), None);
    }

    #[test]
    fn hint_returns_none_for_unknown_pattern() {
        let titles = vec!["Release A", "Release B"];
        assert_eq!(build_planner_hint(&titles), None);
    }

    #[test]
    fn hint_for_semver_with_only_one_milestone_works() {
        let titles = vec!["v0.1.0"];
        let hint = build_planner_hint(&titles).expect("single semver title still works");
        assert!(hint.contains("v0.2.0"));
    }
}
