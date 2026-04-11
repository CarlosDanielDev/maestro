use std::sync::LazyLock;

mod parser;

const CHANGELOG_RAW: &str = include_str!("../../CHANGELOG.md");

static CHANGELOG: LazyLock<ChangelogData> = LazyLock::new(|| ChangelogData::parse(CHANGELOG_RAW));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangeCategory {
    Added,
    Fixed,
    Changed,
    Deprecated,
    Removed,
    Security,
    Performance,
    Documentation,
    Testing,
}

impl ChangeCategory {
    pub fn from_heading(s: &str) -> Option<Self> {
        match s {
            "Added" => Some(Self::Added),
            "Fixed" => Some(Self::Fixed),
            "Changed" => Some(Self::Changed),
            "Deprecated" => Some(Self::Deprecated),
            "Removed" => Some(Self::Removed),
            "Security" => Some(Self::Security),
            "Performance" => Some(Self::Performance),
            "Documentation" => Some(Self::Documentation),
            "Testing" => Some(Self::Testing),
            _ => None,
        }
    }

    pub fn priority(self) -> u8 {
        match self {
            Self::Added => 0,
            Self::Fixed => 1,
            Self::Changed => 2,
            Self::Security => 3,
            Self::Performance => 4,
            Self::Deprecated => 5,
            Self::Removed => 6,
            Self::Documentation => 7,
            Self::Testing => 8,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Added => "Added",
            Self::Fixed => "Fixed",
            Self::Changed => "Changed",
            Self::Deprecated => "Deprecated",
            Self::Removed => "Removed",
            Self::Security => "Security",
            Self::Performance => "Performance",
            Self::Documentation => "Documentation",
            Self::Testing => "Testing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeItem {
    pub text: String,
    pub issue_numbers: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSection {
    pub category: ChangeCategory,
    pub items: Vec<ChangeItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionEntry {
    pub version: String,
    pub date: Option<String>,
    pub sections: Vec<ChangeSection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangelogData {
    pub entries: Vec<VersionEntry>,
}

impl ChangelogData {
    pub fn parse(raw: &str) -> Self {
        parser::parse(raw)
    }

    pub fn highlights(&self, version: &str, max: usize) -> Vec<&ChangeItem> {
        let Some(entry) = self.entries.iter().find(|e| e.version == version) else {
            return vec![];
        };

        let mut items: Vec<(u8, &ChangeItem)> = entry
            .sections
            .iter()
            .flat_map(|s| {
                let prio = s.category.priority();
                s.items.iter().map(move |item| (prio, item))
            })
            .collect();

        items.sort_by_key(|(prio, _)| *prio);
        items.into_iter().take(max).map(|(_, item)| item).collect()
    }

    pub fn highlights_with_category(
        &self,
        version: &str,
        max: usize,
    ) -> Vec<(ChangeCategory, &ChangeItem)> {
        let Some(entry) = self.entries.iter().find(|e| e.version == version) else {
            return vec![];
        };

        let mut items: Vec<(u8, ChangeCategory, &ChangeItem)> = entry
            .sections
            .iter()
            .flat_map(|s| {
                let prio = s.category.priority();
                let cat = s.category;
                s.items.iter().map(move |item| (prio, cat, item))
            })
            .collect();

        items.sort_by_key(|(prio, _, _)| *prio);
        items
            .into_iter()
            .take(max)
            .map(|(_, cat, item)| (cat, item))
            .collect()
    }
}

pub fn changelog() -> &'static ChangelogData {
    &CHANGELOG
}

pub fn current_version() -> Option<&'static VersionEntry> {
    let version = env!("CARGO_PKG_VERSION");
    changelog().entries.iter().find(|e| e.version == version)
}

pub fn highlights(version: &str, max: usize) -> Vec<&'static ChangeItem> {
    changelog().highlights(version, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_highlights() -> &'static str {
        "\
## [1.0.0] - 2024-01-01
### Added
- Item A1
- Item A2
- Item A3
### Fixed
- Item F1
- Item F2
### Changed
- Item C1
"
    }

    fn fixture_single_version() -> &'static str {
        "\
## [2.0.0] - 2024-06-01
### Added
- Feature X
"
    }

    #[test]
    fn highlights_returns_added_before_fixed_before_changed() {
        let data = ChangelogData::parse(fixture_highlights());
        let result = data.highlights("1.0.0", 10);
        assert_eq!(result.len(), 6);
        assert!(result[0].text.starts_with("Item A"));
        assert!(result[1].text.starts_with("Item A"));
        assert!(result[2].text.starts_with("Item A"));
        assert!(result[3].text.starts_with("Item F"));
        assert!(result[4].text.starts_with("Item F"));
        assert!(result[5].text.starts_with("Item C"));
    }

    #[test]
    fn highlights_max_limits_result_count() {
        let data = ChangelogData::parse(fixture_highlights());
        let result = data.highlights("1.0.0", 2);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn highlights_max_larger_than_available_returns_all() {
        let data = ChangelogData::parse(fixture_single_version());
        let result = data.highlights("2.0.0", 100);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn highlights_max_zero_returns_empty() {
        let data = ChangelogData::parse(fixture_highlights());
        let result = data.highlights("1.0.0", 0);
        assert!(result.is_empty());
    }

    #[test]
    fn highlights_unknown_version_returns_empty_vec() {
        let data = ChangelogData::parse(fixture_highlights());
        let result = data.highlights("9.9.9", 5);
        assert!(result.is_empty());
    }

    #[test]
    fn highlights_with_category_returns_category_and_item() {
        let data = ChangelogData::parse(fixture_highlights());
        let result = data.highlights_with_category("1.0.0", 10);
        assert_eq!(result.len(), 6);
        assert_eq!(result[0].0, ChangeCategory::Added);
        assert!(result[0].1.text.starts_with("Item A"));
        assert_eq!(result[3].0, ChangeCategory::Fixed);
        assert!(result[3].1.text.starts_with("Item F"));
        assert_eq!(result[5].0, ChangeCategory::Changed);
    }

    #[test]
    fn highlights_with_category_respects_max() {
        let data = ChangelogData::parse(fixture_highlights());
        let result = data.highlights_with_category("1.0.0", 2);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn current_version_does_not_panic() {
        let _ = current_version();
    }

    #[test]
    fn changelog_static_accessor_does_not_panic() {
        let data = changelog();
        let _ = data.entries.len();
    }
}
