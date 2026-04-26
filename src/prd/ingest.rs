//! Heuristic PRD-from-markdown parser (#321).
//!
//! Extracts Vision, Goals, Non-Goals, and Stakeholders from a markdown
//! body that uses conventional `##`-level headings. No LLM call: this is
//! a deterministic pure function so the same input always produces the
//! same output and tests stay fast/offline.
//!
//! Section keywords are matched case-insensitively and tolerate
//! near-synonyms commonly seen in real PRDs:
//! - **Vision**: `vision`, `mission`, `purpose`, `overview`
//! - **Goals**: `goals`, `objectives`, `success criteria`, `acceptance criteria`
//! - **Non-Goals**: `non-goals`, `non goals`, `out of scope`, `what we're not`
//! - **Stakeholders**: `stakeholders`, `team`, `owners`, `roles`

#![deny(clippy::unwrap_used)]
#![allow(dead_code)]

/// Result of parsing a PRD markdown body. Fields are `Option`/`Vec`
/// because partial extraction is normal — most PRDs won't carry all four
/// sections. The caller merges these into a live `Prd` without
/// overwriting user-edited values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IngestedPrd {
    pub vision: Option<String>,
    pub goals: Vec<String>,
    pub non_goals: Vec<String>,
    pub stakeholders: Vec<(String, String)>,
}

impl IngestedPrd {
    pub fn is_empty(&self) -> bool {
        self.vision.is_none()
            && self.goals.is_empty()
            && self.non_goals.is_empty()
            && self.stakeholders.is_empty()
    }

    pub fn summary(&self) -> String {
        format!(
            "vision={} goals={} non-goals={} stakeholders={}",
            if self.vision.is_some() { "yes" } else { "no" },
            self.goals.len(),
            self.non_goals.len(),
            self.stakeholders.len(),
        )
    }
}

/// Parse a PRD markdown body. Returns an empty `IngestedPrd` if no
/// recognizable sections are present (caller treats that as "no PRD
/// content found").
pub fn parse_markdown(body: &str) -> IngestedPrd {
    let sections = split_into_sections(body);
    let mut out = IngestedPrd::default();

    for section in &sections {
        match classify_heading(&section.heading) {
            Some(SectionKind::Vision) if out.vision.is_none() => {
                out.vision = first_paragraph(&section.body);
            }
            Some(SectionKind::Vision) => {
                // Already populated — first matching Vision section wins.
            }
            Some(SectionKind::Goals) => {
                let items = extract_list_items(&section.body);
                for item in items {
                    if !out.goals.iter().any(|g| g.eq_ignore_ascii_case(&item)) {
                        out.goals.push(item);
                    }
                }
            }
            Some(SectionKind::NonGoals) => {
                let items = extract_list_items(&section.body);
                for item in items {
                    if !out.non_goals.iter().any(|g| g.eq_ignore_ascii_case(&item)) {
                        out.non_goals.push(item);
                    }
                }
            }
            Some(SectionKind::Stakeholders) => {
                let entries = extract_stakeholders(&section.body);
                for (name, role) in entries {
                    if !out
                        .stakeholders
                        .iter()
                        .any(|(n, _)| n.eq_ignore_ascii_case(&name))
                    {
                        out.stakeholders.push((name, role));
                    }
                }
            }
            None => {}
        }
    }

    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SectionKind {
    Vision,
    Goals,
    NonGoals,
    Stakeholders,
}

#[derive(Debug)]
struct Section {
    heading: String,
    body: String,
}

fn split_into_sections(body: &str) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    let mut current: Option<Section> = None;

    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(heading) = strip_heading_marker(trimmed) {
            if let Some(s) = current.take() {
                sections.push(s);
            }
            current = Some(Section {
                heading: heading.trim().to_string(),
                body: String::new(),
            });
            continue;
        }
        if let Some(s) = current.as_mut() {
            s.body.push_str(line);
            s.body.push('\n');
        }
    }
    if let Some(s) = current {
        sections.push(s);
    }
    sections
}

/// Static lookup avoids `format!`+`String::repeat` allocations on every
/// non-heading line during section-splitting.
const HEADING_PREFIXES: [&str; 6] = ["# ", "## ", "### ", "#### ", "##### ", "###### "];

fn strip_heading_marker(line: &str) -> Option<&str> {
    HEADING_PREFIXES.iter().find_map(|p| line.strip_prefix(p))
}

fn classify_heading(heading: &str) -> Option<SectionKind> {
    let lower = heading.to_lowercase();
    let lower = lower.trim();

    let vision_keywords = ["vision", "mission", "purpose", "overview"];
    let goal_keywords = [
        "goals",
        "goal",
        "objectives",
        "objective",
        "success criteria",
        "acceptance criteria",
    ];
    let non_goal_keywords = ["non-goals", "non goals", "non-goal", "out of scope"];
    let stakeholder_keywords = ["stakeholders", "stakeholder", "team", "owners", "roles"];

    // Order matters: Non-Goals must be checked BEFORE Goals so "Non-Goals"
    // doesn't match the "goals" keyword first.
    if non_goal_keywords.iter().any(|k| matches_section(lower, k)) {
        return Some(SectionKind::NonGoals);
    }
    if vision_keywords.iter().any(|k| matches_section(lower, k)) {
        return Some(SectionKind::Vision);
    }
    if goal_keywords.iter().any(|k| matches_section(lower, k)) {
        return Some(SectionKind::Goals);
    }
    if stakeholder_keywords
        .iter()
        .any(|k| matches_section(lower, k))
    {
        return Some(SectionKind::Stakeholders);
    }
    None
}

/// A heading matches a keyword when it equals the keyword OR starts with
/// it followed by separator (space, `:`, `&`). Avoids false positives
/// like "Goal Setting Strategy" matching "goal".
fn matches_section(heading: &str, keyword: &str) -> bool {
    if heading == keyword {
        return true;
    }
    if let Some(rest) = heading.strip_prefix(keyword) {
        return rest
            .chars()
            .next()
            .is_some_and(|c| matches!(c, ' ' | ':' | '&' | '/' | ','));
    }
    false
}

fn first_paragraph(body: &str) -> Option<String> {
    let mut buf = String::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !buf.is_empty() {
                break;
            }
            continue;
        }
        if trimmed.starts_with("```") || trimmed == "---" {
            break;
        }
        if !buf.is_empty() {
            buf.push(' ');
        }
        buf.push_str(trimmed);
    }
    if buf.is_empty() { None } else { Some(buf) }
}

fn extract_list_items(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(item) = strip_list_marker(trimmed) {
            let cleaned = strip_inline_emphasis(item.trim());
            if !cleaned.is_empty() {
                out.push(cleaned);
            }
        }
    }
    out
}

/// Strip leading bullet (`-`, `*`, `+`) or ordered (`1.`, `2)`) marker,
/// and also strip a GFM task-list checkbox (`[ ]`, `[x]`, `[X]`) if
/// present — otherwise the checkbox text bleeds into the goal label and
/// renders as a duplicated `[ ] [ ] ...` in the PRD UI.
fn strip_list_marker(line: &str) -> Option<&str> {
    let after_marker = if let Some(rest) = line.strip_prefix("- ") {
        rest
    } else if let Some(rest) = line.strip_prefix("* ") {
        rest
    } else if let Some(rest) = line.strip_prefix("+ ") {
        rest
    } else {
        // Ordered: `<digits>. ` or `<digits>) `
        let mut chars = line.char_indices();
        let mut digit_end = 0;
        let mut saw_digit = false;
        for (i, c) in chars.by_ref() {
            if c.is_ascii_digit() {
                digit_end = i + c.len_utf8();
                saw_digit = true;
            } else {
                break;
            }
        }
        if saw_digit
            && let Some(after) = line.get(digit_end..)
            && (after.starts_with(". ") || after.starts_with(") "))
        {
            line.get(digit_end + 2..)?
        } else {
            return None;
        }
    };
    Some(strip_gfm_checkbox(after_marker))
}

fn strip_gfm_checkbox(s: &str) -> &str {
    s.strip_prefix("[ ] ")
        .or_else(|| s.strip_prefix("[x] "))
        .or_else(|| s.strip_prefix("[X] "))
        .unwrap_or(s)
}

/// Strip a single layer of `**bold**` / `*italic*` wrapping a leading
/// fragment so item titles don't carry markdown noise into the PRD UI.
fn strip_inline_emphasis(text: &str) -> String {
    text.trim_start_matches("**")
        .trim_end_matches("**")
        .trim_start_matches('*')
        .trim_end_matches('*')
        .to_string()
}

/// Stakeholders sections may use `- Name — Role` / `- Name (Role)` /
/// `| Name | Role |` table rows. We extract the obvious shapes.
fn extract_stakeholders(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(item) = strip_list_marker(trimmed) {
            let parts = split_name_role(item);
            if let Some(pair) = parts {
                out.push(pair);
            }
            continue;
        }
        if let Some(row) = parse_table_row(trimmed) {
            // Skip table header / separator rows.
            if row.0.eq_ignore_ascii_case("name") || row.0.starts_with("---") {
                continue;
            }
            out.push(row);
        }
    }
    out
}

fn split_name_role(item: &str) -> Option<(String, String)> {
    // Try `Name — Role` (em-dash), then `Name - Role`, then `Name (Role)`.
    for sep in [" — ", " – ", " -- ", " - "] {
        if let Some((name, role)) = item.split_once(sep) {
            return Some((name.trim().to_string(), role.trim().to_string()));
        }
    }
    if let Some(open) = item.find(" (")
        && let Some(close) = item.rfind(')')
        && close > open
    {
        let name = item[..open].trim().to_string();
        let role = item[open + 2..close].trim().to_string();
        if !name.is_empty() && !role.is_empty() {
            return Some((name, role));
        }
    }
    None
}

fn parse_table_row(line: &str) -> Option<(String, String)> {
    if !line.starts_with('|') {
        return None;
    }
    let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
    if cells.len() < 2 {
        return None;
    }
    let name = cells[0].to_string();
    let role = cells[1].to_string();
    if name.is_empty() || role.is_empty() {
        return None;
    }
    Some((name, role))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_body_returns_empty_ingest() {
        assert!(parse_markdown("").is_empty());
    }

    #[test]
    fn single_vision_section_extracts_first_paragraph() {
        let body = "## Vision\n\nMaestro orchestrates Claude sessions.\n\nFollow-up paragraph.";
        let r = parse_markdown(body);
        assert_eq!(
            r.vision.as_deref(),
            Some("Maestro orchestrates Claude sessions.")
        );
    }

    #[test]
    fn vision_synonyms_match() {
        for hdr in ["Vision", "Mission", "Purpose", "Overview"] {
            let body = format!("## {hdr}\n\nThe pitch.");
            let r = parse_markdown(&body);
            assert_eq!(r.vision.as_deref(), Some("The pitch."));
        }
    }

    #[test]
    fn vision_with_amp_subtitle_still_classifies() {
        let body = "## Vision & Purpose\n\nWhy we exist.";
        let r = parse_markdown(body);
        assert_eq!(r.vision.as_deref(), Some("Why we exist."));
    }

    #[test]
    fn goals_bullet_list_is_extracted() {
        let body = "## Goals\n\n- Ship v1\n- Reduce p99 latency\n- Onboard 10 teams";
        let r = parse_markdown(body);
        assert_eq!(
            r.goals,
            vec!["Ship v1", "Reduce p99 latency", "Onboard 10 teams"]
        );
    }

    #[test]
    fn goals_numbered_list_is_extracted() {
        let body =
            "## Success Criteria\n\n1. First criterion\n2. Second criterion\n3. Third criterion";
        let r = parse_markdown(body);
        assert_eq!(
            r.goals,
            vec!["First criterion", "Second criterion", "Third criterion"]
        );
    }

    #[test]
    fn goals_dedup_case_insensitive() {
        let body = "## Goals\n\n- Build it\n\n## Objectives\n\n- BUILD IT\n- And ship it";
        let r = parse_markdown(body);
        assert_eq!(r.goals, vec!["Build it", "And ship it"]);
    }

    #[test]
    fn non_goals_classify_before_goals() {
        let body = "## Non-Goals\n\n- Multi-tenant SaaS\n- Mobile app";
        let r = parse_markdown(body);
        assert!(r.goals.is_empty());
        assert_eq!(r.non_goals, vec!["Multi-tenant SaaS", "Mobile app"]);
    }

    #[test]
    fn out_of_scope_maps_to_non_goals() {
        let body = "## Out of Scope\n\n- Windows support";
        let r = parse_markdown(body);
        assert_eq!(r.non_goals, vec!["Windows support"]);
    }

    #[test]
    fn stakeholders_em_dash_pairs() {
        let body = "## Stakeholders\n\n- Carlos — Maintainer\n- Dani — Designer";
        let r = parse_markdown(body);
        assert_eq!(
            r.stakeholders,
            vec![
                ("Carlos".into(), "Maintainer".into()),
                ("Dani".into(), "Designer".into()),
            ]
        );
    }

    #[test]
    fn stakeholders_paren_role() {
        let body = "## Team\n\n- Alice (Lead)\n- Bob (Reviewer)";
        let r = parse_markdown(body);
        assert_eq!(
            r.stakeholders,
            vec![
                ("Alice".into(), "Lead".into()),
                ("Bob".into(), "Reviewer".into())
            ]
        );
    }

    #[test]
    fn stakeholders_table_format() {
        let body = "## Owners\n\n| Name | Role |\n|------|------|\n| Carlos | Maintainer |\n| Dani | Designer |";
        let r = parse_markdown(body);
        assert_eq!(
            r.stakeholders,
            vec![
                ("Carlos".into(), "Maintainer".into()),
                ("Dani".into(), "Designer".into()),
            ]
        );
    }

    #[test]
    fn ignores_unrelated_headings() {
        let body = "## Vision\n\nA pitch.\n\n## Tech Stack\n\n- Rust\n- ratatui\n\n## Architecture\n\nDiagram here.";
        let r = parse_markdown(body);
        assert_eq!(r.vision.as_deref(), Some("A pitch."));
        // "Tech Stack" / "Architecture" must NOT be parsed as goals.
        assert!(r.goals.is_empty());
    }

    #[test]
    fn issue_one_real_world_extraction() {
        // Cut-down of the real maestro PRD (issue #1) to exercise the
        // contract on the canonical input.
        let body = "# Maestro: Multi-Session Claude Code Orchestrator\n\n\
                    ## Vision\n\nMaestro is a CLI tool that orchestrates multiple Claude Code sessions.\n\n\
                    ---\n\n\
                    ## Architecture\n\nDiagram.\n\n\
                    ## Success Criteria\n\n\
                    1. `maestro run --prompt` spawns a session\n\
                    2. `maestro run --issue 1,2,3` shows split panels\n\
                    3. `maestro run --milestone v1` queues issues";
        let r = parse_markdown(body);
        assert!(r.vision.as_deref().unwrap_or("").contains("orchestrates"));
        assert_eq!(r.goals.len(), 3);
        assert!(r.goals[0].contains("maestro run --prompt"));
        assert!(r.non_goals.is_empty());
        assert!(r.stakeholders.is_empty());
    }

    #[test]
    fn h1_heading_is_recognized() {
        let body = "# Vision\n\nTop-level vision.";
        let r = parse_markdown(body);
        assert_eq!(r.vision.as_deref(), Some("Top-level vision."));
    }

    #[test]
    fn first_paragraph_stops_at_horizontal_rule() {
        let body = "## Vision\n\nFirst line.\nSecond line.\n---\nNot in vision.";
        let r = parse_markdown(body);
        assert_eq!(r.vision.as_deref(), Some("First line. Second line."));
    }

    #[test]
    fn first_paragraph_stops_at_code_fence() {
        let body = "## Vision\n\nThe pitch.\n```\nnot vision\n```";
        let r = parse_markdown(body);
        assert_eq!(r.vision.as_deref(), Some("The pitch."));
    }

    #[test]
    fn list_marker_with_emphasis_strips_stars() {
        let body = "## Goals\n\n- **First** important goal\n- *Second* goal";
        let r = parse_markdown(body);
        assert!(r.goals[0].contains("First"));
        assert!(r.goals[1].contains("Second"));
    }

    #[test]
    fn gfm_checkbox_in_acceptance_criteria_is_stripped() {
        // The bug: `## Acceptance Criteria` items render in the PRD UI
        // as `[ ] [ ] First crit` (double checkbox) because the GFM
        // checkbox text wasn't stripped from the goal label.
        let body = "## Success Criteria\n\n- [ ] First crit\n- [x] Second crit\n- [X] Third crit";
        let r = parse_markdown(body);
        assert_eq!(r.goals, vec!["First crit", "Second crit", "Third crit"]);
    }

    #[test]
    fn summary_reports_per_section_counts() {
        let body = "## Vision\n\nX.\n\n## Goals\n\n- a\n- b\n\n## Non-Goals\n\n- c\n\n## Stakeholders\n\n- A — B";
        let r = parse_markdown(body);
        assert_eq!(r.summary(), "vision=yes goals=2 non-goals=1 stakeholders=1");
    }
}
