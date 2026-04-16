use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::types::*;

const IGNORED_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    "vendor",
    "__pycache__",
    ".venv",
    "venv",
    "dist",
    "build",
    ".next",
];

const MANIFEST_FILES: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "go.mod",
    "pyproject.toml",
    "requirements.txt",
    "pom.xml",
    "build.gradle",
    "Gemfile",
];

const ENTRY_POINTS: &[(&str, &[&str])] = &[
    ("rs", &["src/main.rs", "src/lib.rs"]),
    ("ts", &["src/index.ts", "src/main.ts", "index.ts"]),
    ("js", &["src/index.js", "index.js", "server.js", "app.js"]),
    ("py", &["main.py", "app.py", "src/main.py", "__main__.py"]),
    ("go", &["main.go", "cmd/main.go"]),
];

#[async_trait]
pub trait ProjectScanner: Send + Sync {
    async fn scan(&self, root: &Path) -> anyhow::Result<ProjectProfile>;
}

pub struct LocalProjectScanner;

impl LocalProjectScanner {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProjectScanner for LocalProjectScanner {
    async fn scan(&self, root: &Path) -> anyhow::Result<ProjectProfile> {
        let root = root.to_path_buf();
        tokio::task::spawn_blocking(move || scan_project(&root))
            .await
            .map_err(|e| anyhow::anyhow!("Scanner task failed: {}", e))?
    }
}

fn scan_project(root: &Path) -> anyhow::Result<ProjectProfile> {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());

    let language = detect_language(root);
    let manifests = find_manifests(root);
    let config_files = find_config_files(root);
    let entry_points = find_entry_points(root);
    let walk = walk_source_tree(root);
    let source_stats = walk.source_stats;
    let test_infra = detect_test_infra(root, language, walk.test_file_count);
    let ci = detect_ci(root);
    let git = gather_git_info(root);
    let dependencies = parse_dependencies(root, language);
    let directory_tree = build_directory_tree(root, 3);
    let has_maestro_config =
        root.join("maestro.toml").exists() || root.join(".claude/CLAUDE.md").exists();

    Ok(ProjectProfile {
        name,
        root: root.to_path_buf(),
        language,
        manifests,
        config_files,
        entry_points,
        source_stats,
        test_infra,
        ci,
        git,
        dependencies,
        directory_tree,
        has_maestro_config,
    })
}

fn detect_language(root: &Path) -> ProjectLanguage {
    if root.join("Cargo.toml").exists() {
        ProjectLanguage::Rust
    } else if root.join("package.json").exists() {
        ProjectLanguage::TypeScript
    } else if root.join("pyproject.toml").exists() || root.join("requirements.txt").exists() {
        ProjectLanguage::Python
    } else if root.join("go.mod").exists() {
        ProjectLanguage::Go
    } else if root.join("pom.xml").exists() || root.join("build.gradle").exists() {
        ProjectLanguage::Java
    } else if root.join("Gemfile").exists() {
        ProjectLanguage::Ruby
    } else {
        ProjectLanguage::Unknown
    }
}

fn is_ignored(entry: &walkdir::DirEntry) -> bool {
    entry.file_type().is_dir()
        && entry
            .file_name()
            .to_str()
            .map(|name| IGNORED_DIRS.contains(&name))
            .unwrap_or(false)
}

fn find_manifests(root: &Path) -> Vec<PathBuf> {
    MANIFEST_FILES
        .iter()
        .filter_map(|name| {
            let p = root.join(name);
            p.exists().then_some(PathBuf::from(name))
        })
        .collect()
}

fn find_config_files(root: &Path) -> Vec<PathBuf> {
    let candidates = [
        ".env",
        ".env.example",
        "docker-compose.yml",
        "docker-compose.yaml",
        "Dockerfile",
        ".eslintrc.json",
        ".prettierrc",
        "tsconfig.json",
        "jest.config.js",
        "jest.config.ts",
        "vitest.config.ts",
        "maestro.toml",
    ];
    candidates
        .iter()
        .filter_map(|name| {
            let p = root.join(name);
            p.exists().then_some(PathBuf::from(name))
        })
        .collect()
}

fn find_entry_points(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for (_ext, paths) in ENTRY_POINTS {
        for path in *paths {
            if root.join(path).exists() {
                found.push(PathBuf::from(path));
            }
        }
    }
    found
}

/// Combined result of a single directory walk (source stats + test file count).
struct WalkResult {
    source_stats: SourceStats,
    test_file_count: u32,
}

fn walk_source_tree(root: &Path) -> WalkResult {
    let mut by_ext: HashMap<String, (u32, u64)> = HashMap::new();
    let mut test_file_count: u32 = 0;

    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_ignored(e))
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }

        let file_name = entry.file_name().to_string_lossy();
        if file_name.contains("test")
            || file_name.contains("spec")
            || file_name.ends_with("_test.go")
        {
            test_file_count += 1;
        }

        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();

        if !matches!(
            ext.as_str(),
            "rs" | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "py"
                | "go"
                | "java"
                | "rb"
                | "c"
                | "cpp"
                | "h"
                | "hpp"
                | "cs"
                | "swift"
                | "kt"
        ) {
            continue;
        }

        let lines = count_lines(entry.path());
        let counter = by_ext.entry(ext).or_insert((0, 0));
        counter.0 += 1;
        counter.1 += lines;
    }

    let total_files: u32 = by_ext.values().map(|(f, _)| f).sum();
    let total_lines: u64 = by_ext.values().map(|(_, l)| l).sum();

    let mut by_extension: Vec<ExtensionStats> = by_ext
        .into_iter()
        .map(|(ext, (files, lines))| ExtensionStats {
            extension: ext,
            files,
            lines,
        })
        .collect();
    by_extension.sort_by_key(|b| std::cmp::Reverse(b.files));

    WalkResult {
        source_stats: SourceStats {
            total_files,
            total_lines,
            by_extension,
        },
        test_file_count,
    }
}

fn count_lines(path: &Path) -> u64 {
    use std::io::Read;
    let mut buf = [0u8; 8192];
    let mut count = 0u64;
    let Ok(mut file) = std::fs::File::open(path) else {
        return 0;
    };
    loop {
        let n = match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        count += buf[..n].iter().filter(|&&b| b == b'\n').count() as u64;
    }
    count
}

fn detect_test_infra(
    root: &Path,
    language: ProjectLanguage,
    test_file_count: u32,
) -> TestInfraInfo {
    let mut test_dirs = Vec::new();

    let test_dir_names = ["tests", "test", "__tests__", "spec"];
    for name in &test_dir_names {
        let p = root.join(name);
        if p.is_dir() {
            test_dirs.push(PathBuf::from(name));
        }
    }

    let framework = match language {
        ProjectLanguage::Rust => Some("cargo test".into()),
        ProjectLanguage::TypeScript | ProjectLanguage::Java => {
            if root.join("jest.config.js").exists()
                || root.join("jest.config.ts").exists()
                || root.join("jest.config.cjs").exists()
            {
                Some("jest".into())
            } else if root.join("vitest.config.ts").exists() {
                Some("vitest".into())
            } else {
                None
            }
        }
        ProjectLanguage::Python => {
            if root.join("pytest.ini").exists()
                || root.join("pyproject.toml").exists()
                || root.join("conftest.py").exists()
            {
                Some("pytest".into())
            } else {
                None
            }
        }
        ProjectLanguage::Go => Some("go test".into()),
        _ => None,
    };

    let has_tests = test_file_count > 0 || !test_dirs.is_empty();

    TestInfraInfo {
        has_tests,
        framework,
        test_directories: test_dirs,
        test_file_count,
    }
}

fn detect_ci(root: &Path) -> CiInfo {
    let ga_dir = root.join(".github/workflows");
    if ga_dir.is_dir() {
        let config_files: Vec<PathBuf> = std::fs::read_dir(&ga_dir)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .map(|ext| ext == "yml" || ext == "yaml")
                            .unwrap_or(false)
                    })
                    .map(|e| {
                        PathBuf::from(format!(
                            ".github/workflows/{}",
                            e.file_name().to_string_lossy()
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();

        if !config_files.is_empty() {
            return CiInfo {
                provider: Some("github_actions".into()),
                config_files,
            };
        }
    }

    if root.join(".gitlab-ci.yml").exists() {
        return CiInfo {
            provider: Some("gitlab_ci".into()),
            config_files: vec![PathBuf::from(".gitlab-ci.yml")],
        };
    }

    if root.join("Jenkinsfile").exists() {
        return CiInfo {
            provider: Some("jenkins".into()),
            config_files: vec![PathBuf::from("Jenkinsfile")],
        };
    }

    if root.join(".circleci/config.yml").exists() {
        return CiInfo {
            provider: Some("circleci".into()),
            config_files: vec![PathBuf::from(".circleci/config.yml")],
        };
    }

    CiInfo {
        provider: None,
        config_files: vec![],
    }
}

fn gather_git_info(root: &Path) -> GitInfo {
    if !root.join(".git").exists() {
        return GitInfo {
            is_git_repo: false,
            default_branch: None,
            remote_url: None,
            commit_count: 0,
            recent_contributors: vec![],
        };
    }

    let default_branch = run_git(root, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let remote_url = run_git(root, &["remote", "get-url", "origin"]);
    let commit_count = run_git(root, &["rev-list", "--count", "HEAD"])
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let recent_contributors = run_git(root, &["log", "--format=%aN", "-20", "--no-merges"])
        .map(|s| {
            let mut names: Vec<String> = s.lines().map(|l| l.to_string()).collect();
            names.sort();
            names.dedup();
            names
        })
        .unwrap_or_default();

    GitInfo {
        is_git_repo: true,
        default_branch,
        remote_url,
        commit_count,
        recent_contributors,
    }
}

fn run_git(root: &Path, args: &[&str]) -> Option<String> {
    std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn parse_dependencies(root: &Path, language: ProjectLanguage) -> DependencySummary {
    match language {
        ProjectLanguage::Rust => parse_cargo_deps(root),
        ProjectLanguage::TypeScript => parse_npm_deps(root),
        _ => DependencySummary {
            direct_count: 0,
            dev_count: 0,
            notable: vec![],
        },
    }
}

fn parse_cargo_deps(root: &Path) -> DependencySummary {
    let path = root.join("Cargo.toml");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            return DependencySummary::default();
        }
    };

    let mut direct = 0u32;
    let mut dev = 0u32;
    let mut notable = Vec::new();
    let mut in_deps = false;
    let mut in_dev_deps = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[dependencies]" {
            in_deps = true;
            in_dev_deps = false;
            continue;
        } else if trimmed == "[dev-dependencies]" {
            in_deps = false;
            in_dev_deps = true;
            continue;
        } else if trimmed.starts_with('[') {
            in_deps = false;
            in_dev_deps = false;
            continue;
        }

        if (in_deps || in_dev_deps) && trimmed.contains('=') && !trimmed.starts_with('#') {
            if in_deps {
                direct += 1;
            } else {
                dev += 1;
            }
            if let Some(name) = trimmed.split('=').next().map(|s| s.trim()) {
                let notable_crates = [
                    "tokio",
                    "serde",
                    "clap",
                    "actix-web",
                    "axum",
                    "rocket",
                    "diesel",
                    "sqlx",
                    "reqwest",
                    "tracing",
                    "ratatui",
                ];
                if notable_crates.contains(&name) {
                    notable.push(name.to_string());
                }
            }
        }
    }

    DependencySummary {
        direct_count: direct,
        dev_count: dev,
        notable,
    }
}

fn parse_npm_deps(root: &Path) -> DependencySummary {
    let path = root.join("package.json");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            return DependencySummary::default();
        }
    };

    let v: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => {
            return DependencySummary::default();
        }
    };

    let direct = v
        .get("dependencies")
        .and_then(|d| d.as_object())
        .map(|o| o.len() as u32)
        .unwrap_or(0);
    let dev = v
        .get("devDependencies")
        .and_then(|d| d.as_object())
        .map(|o| o.len() as u32)
        .unwrap_or(0);

    DependencySummary {
        direct_count: direct,
        dev_count: dev,
        notable: vec![],
    }
}

fn build_directory_tree(root: &Path, max_depth: usize) -> String {
    let mut lines = Vec::new();
    build_tree_recursive(root, 0, max_depth, &mut lines);
    lines.join("\n")
}

const MAX_ENTRIES_PER_DIR: usize = 50;

fn build_tree_recursive(current: &Path, depth: usize, max_depth: usize, lines: &mut Vec<String>) {
    if depth > max_depth {
        return;
    }

    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    let total = entries.len();
    for entry in entries.into_iter().take(MAX_ENTRIES_PER_DIR) {
        let name = entry.file_name().to_string_lossy().to_string();
        if IGNORED_DIRS.contains(&name.as_str()) || name.starts_with('.') {
            continue;
        }
        let indent = "  ".repeat(depth);
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            lines.push(format!("{}{}/", indent, name));
            build_tree_recursive(&entry.path(), depth + 1, max_depth, lines);
        } else {
            lines.push(format!("{}{}", indent, name));
        }
    }
    if total > MAX_ENTRIES_PER_DIR {
        let indent = "  ".repeat(depth);
        lines.push(format!(
            "{}... ({} more entries)",
            indent,
            total - MAX_ENTRIES_PER_DIR
        ));
    }
}

#[cfg(test)]
pub struct MockProjectScanner {
    result: Option<ProjectProfile>,
}

#[cfg(test)]
impl MockProjectScanner {
    pub fn with_profile(profile: ProjectProfile) -> Self {
        Self {
            result: Some(profile),
        }
    }
}

#[cfg(test)]
#[async_trait]
impl ProjectScanner for MockProjectScanner {
    async fn scan(&self, _path: &Path) -> anyhow::Result<ProjectProfile> {
        self.result
            .clone()
            .ok_or_else(|| anyhow::anyhow!("mock scanner: no profile configured"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_language_rust() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(detect_language(dir.path()), ProjectLanguage::Rust);
    }

    #[test]
    fn detect_language_typescript() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert_eq!(detect_language(dir.path()), ProjectLanguage::TypeScript);
    }

    #[test]
    fn detect_language_python() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pyproject.toml"), "").unwrap();
        assert_eq!(detect_language(dir.path()), ProjectLanguage::Python);
    }

    #[test]
    fn detect_language_go() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("go.mod"), "").unwrap();
        assert_eq!(detect_language(dir.path()), ProjectLanguage::Go);
    }

    #[test]
    fn detect_language_unknown() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(detect_language(dir.path()), ProjectLanguage::Unknown);
    }

    #[test]
    fn find_manifests_detects_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        let manifests = find_manifests(dir.path());
        assert_eq!(manifests, vec![PathBuf::from("Cargo.toml")]);
    }

    #[test]
    fn find_entry_points_detects_main_rs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        let eps = find_entry_points(dir.path());
        assert!(eps.contains(&PathBuf::from("src/main.rs")));
    }

    #[test]
    fn source_stats_counts_files_and_lines() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(
            dir.path().join("src/lib.rs"),
            "pub fn foo() {}\npub fn bar() {}\n",
        )
        .unwrap();
        let stats = walk_source_tree(dir.path()).source_stats;
        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.total_lines, 3);
    }

    #[test]
    fn source_stats_ignores_vendor_dirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules/dep")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(
            dir.path().join("node_modules/dep/index.js"),
            "module.exports = {};\n",
        )
        .unwrap();
        let stats = walk_source_tree(dir.path()).source_stats;
        assert_eq!(stats.total_files, 1, "node_modules files must be excluded");
    }

    #[test]
    fn detect_ci_github_actions() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".github/workflows")).unwrap();
        std::fs::write(dir.path().join(".github/workflows/ci.yml"), "on: push").unwrap();
        let ci = detect_ci(dir.path());
        assert_eq!(ci.provider, Some("github_actions".into()));
        assert!(!ci.config_files.is_empty());
    }

    #[test]
    fn detect_ci_none() {
        let dir = tempfile::tempdir().unwrap();
        let ci = detect_ci(dir.path());
        assert!(ci.provider.is_none());
    }

    #[test]
    fn detect_test_infra_rust() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("tests")).unwrap();
        std::fs::write(
            dir.path().join("tests/integration_test.rs"),
            "#[test]\nfn it_works() {}",
        )
        .unwrap();
        let walk = walk_source_tree(dir.path());
        let info = detect_test_infra(dir.path(), ProjectLanguage::Rust, walk.test_file_count);
        assert!(info.has_tests);
        assert_eq!(info.framework, Some("cargo test".into()));
        assert!(info.test_file_count >= 1);
    }

    #[test]
    fn git_info_non_git_dir() {
        let dir = tempfile::tempdir().unwrap();
        let info = gather_git_info(dir.path());
        assert!(!info.is_git_repo);
    }

    #[test]
    fn parse_cargo_deps_counts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            r#"[package]
name = "test"
version = "0.1.0"

[dependencies]
tokio = "1"
serde = "1"

[dev-dependencies]
tempfile = "3"
"#,
        )
        .unwrap();
        let deps = parse_cargo_deps(dir.path());
        assert_eq!(deps.direct_count, 2);
        assert_eq!(deps.dev_count, 1);
        assert!(deps.notable.contains(&"tokio".to_string()));
        assert!(deps.notable.contains(&"serde".to_string()));
    }

    #[test]
    fn directory_tree_excludes_ignored_dirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "").unwrap();
        let tree = build_directory_tree(dir.path(), 2);
        assert!(tree.contains("src/"));
        assert!(!tree.contains("node_modules"));
    }

    #[tokio::test]
    async fn local_scanner_scans_rust_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();

        let scanner = LocalProjectScanner::new();
        let profile = scanner.scan(dir.path()).await.unwrap();
        assert_eq!(profile.language, ProjectLanguage::Rust);
        assert_eq!(profile.source_stats.total_files, 1);
        assert!(profile.manifests.contains(&PathBuf::from("Cargo.toml")));
    }

    #[test]
    fn parse_npm_deps_counts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies":{"react":"18","next":"14"},"devDependencies":{"jest":"29"}}"#,
        )
        .unwrap();
        let deps = parse_npm_deps(dir.path());
        assert_eq!(deps.direct_count, 2);
        assert_eq!(deps.dev_count, 1);
    }
}
