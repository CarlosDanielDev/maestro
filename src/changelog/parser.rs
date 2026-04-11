use regex::Regex;
use std::sync::LazyLock;

use super::{ChangeCategory, ChangeItem, ChangeSection, ChangelogData, VersionEntry};

static ISSUE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\(#(\d+)\)").unwrap());

pub fn parse(raw: &str) -> ChangelogData {
    let mut entries: Vec<VersionEntry> = Vec::new();
    let mut current_version: Option<VersionEntry> = None;
    let mut current_section: Option<ChangeSection> = None;

    for line in raw.lines() {
        let trimmed = line.trim();

        if let Some(ver) = parse_version_heading(trimmed) {
            flush_section(&mut current_version, &mut current_section);
            if let Some(v) = current_version.take() {
                entries.push(v);
            }
            current_version = Some(ver);
            continue;
        }

        if let Some(cat) = parse_section_heading(trimmed) {
            flush_section(&mut current_version, &mut current_section);
            current_section = Some(ChangeSection {
                category: cat,
                items: Vec::new(),
            });
            continue;
        }

        if let Some(item) = parse_item(trimmed)
            && let Some(section) = current_section.as_mut()
        {
            section.items.push(item);
        }
    }

    flush_section(&mut current_version, &mut current_section);
    if let Some(v) = current_version.take() {
        entries.push(v);
    }

    ChangelogData { entries }
}

fn flush_section(version: &mut Option<VersionEntry>, section: &mut Option<ChangeSection>) {
    if let (Some(v), Some(s)) = (version.as_mut(), section.take())
        && !s.items.is_empty()
    {
        v.sections.push(s);
    }
}

fn parse_version_heading(line: &str) -> Option<VersionEntry> {
    if !line.starts_with("## ") {
        return None;
    }
    let rest = &line[3..];

    if rest.trim_start().starts_with("[Unreleased]") {
        return Some(VersionEntry {
            version: "Unreleased".to_string(),
            date: None,
            sections: Vec::new(),
        });
    }

    let version_start = rest.find('[')?;
    let version_end = rest.find(']')?;
    let version = rest[version_start + 1..version_end].to_string();

    let date = rest.find("- ").map(|i| rest[i + 2..].trim().to_string());

    Some(VersionEntry {
        version,
        date,
        sections: Vec::new(),
    })
}

fn parse_section_heading(line: &str) -> Option<ChangeCategory> {
    if !line.starts_with("### ") {
        return None;
    }
    let heading = line[4..].trim();
    ChangeCategory::from_heading(heading)
}

fn parse_item(line: &str) -> Option<ChangeItem> {
    if !line.starts_with("- ") {
        return None;
    }
    let text = line[2..].to_string();
    let issue_numbers: Vec<u64> = ISSUE_RE
        .captures_iter(&text)
        .filter_map(|cap| cap.get(1)?.as_str().parse().ok())
        .collect();

    Some(ChangeItem {
        text,
        issue_numbers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_two_versions() -> &'static str {
        "\
## [1.1.0] - 2024-02-01
### Added
- Support dark mode (#101)
- Add keyboard shortcuts
### Fixed
- Fix crash on empty input (#102) and layout glitch (#103)
## [1.0.0] - 2024-01-01
### Changed
- Refactor rendering pipeline
"
    }

    fn fixture_unreleased() -> &'static str {
        "\
## [Unreleased]
### Added
- Experimental feature
"
    }

    #[test]
    fn parse_two_versions_returns_two_entries() {
        let data = ChangelogData::parse(fixture_two_versions());
        assert_eq!(data.entries.len(), 2);
    }

    #[test]
    fn parse_extracts_version_strings_and_dates() {
        let data = ChangelogData::parse(fixture_two_versions());
        assert_eq!(data.entries[0].version, "1.1.0");
        assert_eq!(data.entries[0].date, Some("2024-02-01".to_string()));
        assert_eq!(data.entries[1].version, "1.0.0");
        assert_eq!(data.entries[1].date, Some("2024-01-01".to_string()));
    }

    #[test]
    fn parse_extracts_sections_and_items() {
        let data = ChangelogData::parse(fixture_two_versions());
        let entry = &data.entries[0];
        assert_eq!(entry.sections.len(), 2);

        let added = entry
            .sections
            .iter()
            .find(|s| matches!(s.category, ChangeCategory::Added))
            .expect("Expected Added section");
        assert_eq!(added.items.len(), 2);
        assert_eq!(added.items[0].text, "Support dark mode (#101)");

        let fixed = entry
            .sections
            .iter()
            .find(|s| matches!(s.category, ChangeCategory::Fixed))
            .expect("Expected Fixed section");
        assert_eq!(fixed.items.len(), 1);
    }

    #[test]
    fn parse_issue_number_single() {
        let input = "## [1.0.0] - 2024-01-01\n### Added\n- Fix crash (#123)\n";
        let data = ChangelogData::parse(input);
        let item = &data.entries[0].sections[0].items[0];
        assert_eq!(item.issue_numbers, vec![123]);
    }

    #[test]
    fn parse_issue_numbers_multiple_separate_parens() {
        let input =
            "## [1.0.0] - 2024-01-01\n### Fixed\n- Fix crash (#123) and regression (#456)\n";
        let data = ChangelogData::parse(input);
        let item = &data.entries[0].sections[0].items[0];
        assert_eq!(item.issue_numbers, vec![123, 456]);
    }

    #[test]
    fn parse_no_issue_numbers_returns_empty_vec() {
        let input = "## [1.0.0] - 2024-01-01\n### Added\n- Add dark mode\n";
        let data = ChangelogData::parse(input);
        let item = &data.entries[0].sections[0].items[0];
        assert!(item.issue_numbers.is_empty());
    }

    #[test]
    fn parse_unreleased_version_and_no_date() {
        let data = ChangelogData::parse(fixture_unreleased());
        assert_eq!(data.entries[0].version, "Unreleased");
        assert!(data.entries[0].date.is_none());
    }

    #[test]
    fn parse_empty_string_returns_empty_entries() {
        let data = ChangelogData::parse("");
        assert!(data.entries.is_empty());
    }

    #[test]
    fn parse_version_with_no_sections_returns_empty_sections() {
        let input = "## [0.1.0] - 2020-01-01\n\nSome prose paragraph.\n";
        let data = ChangelogData::parse(input);
        assert_eq!(data.entries.len(), 1);
        assert!(data.entries[0].sections.is_empty());
    }

    #[test]
    fn parse_empty_section_heading_is_omitted() {
        let input = "## [1.0.0] - 2024-01-01\n### Added\n\n### Fixed\n- Fix crash\n";
        let data = ChangelogData::parse(input);
        let sections = &data.entries[0].sections;
        assert_eq!(sections.len(), 1);
        assert!(matches!(sections[0].category, ChangeCategory::Fixed));
    }

    #[test]
    fn parse_unknown_section_heading_is_skipped() {
        let input = "## [1.0.0] - 2024-01-01\n### Foobar\n- Some item\n### Added\n- Real item\n";
        let data = ChangelogData::parse(input);
        let sections = &data.entries[0].sections;
        assert_eq!(sections.len(), 1);
        assert!(matches!(sections[0].category, ChangeCategory::Added));
    }

    #[test]
    fn change_category_from_heading_all_known_variants() {
        assert!(matches!(
            ChangeCategory::from_heading("Added"),
            Some(ChangeCategory::Added)
        ));
        assert!(matches!(
            ChangeCategory::from_heading("Fixed"),
            Some(ChangeCategory::Fixed)
        ));
        assert!(matches!(
            ChangeCategory::from_heading("Changed"),
            Some(ChangeCategory::Changed)
        ));
        assert!(matches!(
            ChangeCategory::from_heading("Deprecated"),
            Some(ChangeCategory::Deprecated)
        ));
        assert!(matches!(
            ChangeCategory::from_heading("Removed"),
            Some(ChangeCategory::Removed)
        ));
        assert!(matches!(
            ChangeCategory::from_heading("Security"),
            Some(ChangeCategory::Security)
        ));
        assert!(matches!(
            ChangeCategory::from_heading("Performance"),
            Some(ChangeCategory::Performance)
        ));
        assert!(matches!(
            ChangeCategory::from_heading("Documentation"),
            Some(ChangeCategory::Documentation)
        ));
        assert!(matches!(
            ChangeCategory::from_heading("Testing"),
            Some(ChangeCategory::Testing)
        ));
    }

    #[test]
    fn change_category_from_heading_unknown_returns_none() {
        assert!(ChangeCategory::from_heading("Foobar").is_none());
        assert!(ChangeCategory::from_heading("").is_none());
    }

    #[test]
    fn change_category_priority_added_less_than_fixed_less_than_changed() {
        let added = ChangeCategory::Added.priority();
        let fixed = ChangeCategory::Fixed.priority();
        let changed = ChangeCategory::Changed.priority();
        assert!(added < fixed);
        assert!(fixed < changed);
    }
}
