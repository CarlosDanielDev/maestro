//! Repository-level statistics: CHANGELOG release parsing, lines-of-code by
//! language, and git history facts. The git and filesystem reads are
//! best-effort — failures degrade to zero/empty rather than aborting the scan.

use crate::insight::schema::{Release, RepoStats};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

/// Parse `## [vX.Y.Z] - YYYY-MM-DD` headers from a CHANGELOG body. The version
/// is kept verbatim (the `v` prefix is preserved); the date is optional.
pub fn parse_releases(changelog: &str) -> Vec<Release> {
    let Ok(re) =
        regex::Regex::new(r"(?m)^##\s*\[?(v?\d+\.\d+\.\d+)\]?(?:\s*-\s*(\d{4}-\d{2}-\d{2}))?")
    else {
        return Vec::new();
    };
    re.captures_iter(changelog)
        .map(|c| Release {
            version: c[1].to_string(),
            date: c.get(2).map(|m| m.as_str().to_string()),
        })
        .collect()
}

/// Count source files and lines per language by extension under `root`,
/// skipping `target/` and `node_modules/`. Returns the per-language map and the
/// total counted-file count.
pub fn loc_by_lang(root: &Path) -> (BTreeMap<String, u64>, u64) {
    let mut map: BTreeMap<String, u64> = BTreeMap::new();
    let mut files = 0u64;
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path
            .components()
            .any(|c| c.as_os_str() == "target" || c.as_os_str() == "node_modules")
        {
            continue;
        }
        let lang = match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => "rust",
            Some("ts") | Some("tsx") => "typescript",
            Some("md") => "markdown",
            Some("toml") => "toml",
            _ => continue,
        };
        if let Ok(content) = std::fs::read_to_string(path) {
            *map.entry(lang.to_string()).or_default() += content.lines().count() as u64;
            files += 1;
        }
    }
    (map, files)
}

/// Git commit count, contributor count, and first-commit date. Best-effort:
/// returns `(0, 0, None)` on any git failure.
pub fn git_stats(repo: &Path) -> (u64, u64, Option<String>) {
    let git = |args: &[&str]| -> Option<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    let commits = git(&["rev-list", "--count", "HEAD"])
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let contributors = git(&["shortlog", "-sne", "HEAD"])
        .map(|s| s.lines().count() as u64)
        .unwrap_or(0);
    let first = git(&["log", "--reverse", "--format=%cs", "--max-parents=0"])
        .and_then(|s| s.lines().next().map(str::to_string));
    (commits, contributors, first)
}

/// Assemble [`RepoStats`] from the repository root.
pub fn collect(repo: &Path) -> RepoStats {
    let (loc_by_lang, file_count) = loc_by_lang(repo);
    let (commits, contributors, first_commit_date) = git_stats(repo);
    let releases = std::fs::read_to_string(repo.join("CHANGELOG.md"))
        .map(|c| parse_releases(&c))
        .unwrap_or_default();
    RepoStats {
        loc_by_lang,
        file_count,
        commits,
        contributors,
        first_commit_date,
        releases,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_releases_from_changelog_headers() {
        let changelog = "\
# Changelog

## [v0.31.0] - 2026-05-01
### Added
- thing

## [v0.30.0] - 2026-04-01
";
        let rels = parse_releases(changelog);
        assert_eq!(rels.len(), 2);
        assert_eq!(rels[0].version, "v0.31.0");
        assert_eq!(rels[0].date.as_deref(), Some("2026-05-01"));
        assert_eq!(rels[1].version, "v0.30.0");
    }

    #[test]
    fn loc_by_lang_excludes_target_and_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target/debug")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/foo")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\nfn b() {}\n").unwrap();
        std::fs::write(
            root.join("target/debug/foo.rs"),
            "// generated\n".repeat(500),
        )
        .unwrap();
        std::fs::write(
            root.join("node_modules/foo/index.js"),
            "// npm\n".repeat(500),
        )
        .unwrap();

        let (map, files) = loc_by_lang(root);
        // Only src/main.rs counted.
        assert_eq!(files, 1);
        assert_eq!(map.get("rust").copied(), Some(2));
        assert!(!map.contains_key("javascript"));
    }

    #[test]
    fn git_stats_returns_defaults_in_non_repo() {
        let dir = tempfile::tempdir().unwrap();
        let (commits, contributors, first) = git_stats(dir.path());
        assert_eq!(commits, 0);
        assert_eq!(contributors, 0);
        assert_eq!(first, None);
    }
}
