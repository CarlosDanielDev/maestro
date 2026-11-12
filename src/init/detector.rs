//! Project-stack detection. The trait makes the production probe
//! injectable so unit tests can substitute a deterministic fake.

use std::path::Path;

/// A technology stack identified by a marker file under the project root.
/// Variants are ordered for stable polyglot output (Rust, Node, Python, Go).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DetectedStack {
    Rust,
    Node,
    Python,
    Go,
}

impl DetectedStack {
    /// The serialized identifier used in `project.language` /
    /// `project.languages`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Node => "node",
            Self::Python => "python",
            Self::Go => "go",
        }
    }

    /// Marker filenames (relative to the project root) that imply this
    /// stack. Multiple markers (Python) all map to the same variant.
    pub fn markers(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["Cargo.toml"],
            Self::Node => &["package.json"],
            Self::Python => &["pyproject.toml", "requirements.txt", "setup.py"],
            Self::Go => &["go.mod"],
        }
    }
}

/// Detects which technology stacks are present at a project root.
/// Implementations MUST be deterministic for a given root.
pub trait ProjectDetector {
    /// Returns the detected stacks in canonical order. An empty `Vec`
    /// means no markers were found and the caller should write the
    /// generic template.
    fn detect(&self, root: &Path) -> Vec<DetectedStack>;
}

/// Production detector that probes the real filesystem.
#[derive(Debug, Default)]
pub struct FsProjectDetector;

impl FsProjectDetector {
    pub fn new() -> Self {
        Self
    }
}

impl ProjectDetector for FsProjectDetector {
    fn detect(&self, root: &Path) -> Vec<DetectedStack> {
        let mut found = Vec::new();
        for stack in [
            DetectedStack::Rust,
            DetectedStack::Node,
            DetectedStack::Python,
            DetectedStack::Go,
        ] {
            if stack.markers().iter().any(|m| root.join(m).is_file()) {
                found.push(stack);
            }
        }
        found
    }
}

/// True when the tree shows pytest configuration: a `pytest.ini`, a
/// `conftest.py` at the root or under `tests/`, or a `[tool.pytest…]`
/// table inside `pyproject.toml`. Marker files only — init deliberately
/// does not probe the interpreter, so detection stays deterministic for
/// a given tree.
pub fn pytest_configured(root: &Path) -> bool {
    if root.join("pytest.ini").is_file() {
        return true;
    }
    if root.join("conftest.py").is_file() || root.join("tests").join("conftest.py").is_file() {
        return true;
    }
    let pyproject = root.join("pyproject.toml");
    pyproject.is_file()
        && std::fs::read_to_string(&pyproject)
            .map(|content| content.contains("[tool.pytest"))
            .unwrap_or(false)
}

/// True when the conventional Python entry point `main.py` exists at the
/// project root — the file the default `run_command` references.
pub fn python_entry_point(root: &Path) -> bool {
    root.join("main.py").is_file()
}

/// Test-only detector that returns a canned list of stacks.
#[cfg(test)]
pub struct FakeProjectDetector {
    stacks: Vec<DetectedStack>,
}

#[cfg(test)]
impl FakeProjectDetector {
    pub fn new(stacks: Vec<DetectedStack>) -> Self {
        Self { stacks }
    }
}

#[cfg(test)]
impl ProjectDetector for FakeProjectDetector {
    fn detect(&self, _root: &Path) -> Vec<DetectedStack> {
        self.stacks.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write(dir: &Path, name: &str) {
        fs::write(dir.join(name), "").expect("write marker");
    }

    #[test]
    fn fs_detector_finds_rust_marker() {
        let dir = tempdir().unwrap();
        write(dir.path(), "Cargo.toml");
        let result = FsProjectDetector::new().detect(dir.path());
        assert_eq!(result, vec![DetectedStack::Rust]);
        assert_eq!(result[0].id(), "rust");
    }

    #[test]
    fn fs_detector_finds_node_marker() {
        let dir = tempdir().unwrap();
        write(dir.path(), "package.json");
        let result = FsProjectDetector::new().detect(dir.path());
        assert_eq!(result, vec![DetectedStack::Node]);
        assert_eq!(result[0].id(), "node");
    }

    #[test]
    fn fs_detector_finds_go_marker() {
        let dir = tempdir().unwrap();
        write(dir.path(), "go.mod");
        let result = FsProjectDetector::new().detect(dir.path());
        assert_eq!(result, vec![DetectedStack::Go]);
        assert_eq!(result[0].id(), "go");
    }

    #[test]
    fn fs_detector_finds_python_pyproject() {
        let dir = tempdir().unwrap();
        write(dir.path(), "pyproject.toml");
        let result = FsProjectDetector::new().detect(dir.path());
        assert_eq!(result, vec![DetectedStack::Python]);
        assert_eq!(result[0].id(), "python");
    }

    #[test]
    fn fs_detector_finds_python_requirements() {
        let dir = tempdir().unwrap();
        write(dir.path(), "requirements.txt");
        let result = FsProjectDetector::new().detect(dir.path());
        assert_eq!(result, vec![DetectedStack::Python]);
    }

    #[test]
    fn fs_detector_finds_python_setup_py() {
        let dir = tempdir().unwrap();
        write(dir.path(), "setup.py");
        let result = FsProjectDetector::new().detect(dir.path());
        assert_eq!(result, vec![DetectedStack::Python]);
    }

    #[test]
    fn fs_detector_deduplicates_multiple_python_markers() {
        let dir = tempdir().unwrap();
        write(dir.path(), "pyproject.toml");
        write(dir.path(), "requirements.txt");
        let result = FsProjectDetector::new().detect(dir.path());
        let py_count = result
            .iter()
            .filter(|s| **s == DetectedStack::Python)
            .count();
        assert_eq!(py_count, 1);
    }

    #[test]
    fn fs_detector_finds_polyglot_rust_node() {
        let dir = tempdir().unwrap();
        write(dir.path(), "Cargo.toml");
        write(dir.path(), "package.json");
        let result = FsProjectDetector::new().detect(dir.path());
        assert_eq!(result.len(), 2);
        assert!(result.contains(&DetectedStack::Rust));
        assert!(result.contains(&DetectedStack::Node));
    }

    #[test]
    fn fs_detector_returns_empty_for_no_markers() {
        let dir = tempdir().unwrap();
        let result = FsProjectDetector::new().detect(dir.path());
        assert!(result.is_empty());
    }

    #[test]
    fn detected_stack_markers_rust() {
        assert_eq!(DetectedStack::Rust.markers(), &["Cargo.toml"]);
    }

    #[test]
    fn detected_stack_markers_node() {
        assert_eq!(DetectedStack::Node.markers(), &["package.json"]);
    }

    #[test]
    fn detected_stack_markers_python() {
        assert_eq!(
            DetectedStack::Python.markers(),
            &["pyproject.toml", "requirements.txt", "setup.py"]
        );
    }

    #[test]
    fn detected_stack_markers_go() {
        assert_eq!(DetectedStack::Go.markers(), &["go.mod"]);
    }

    // pytest_configured

    #[test]
    fn pytest_configured_false_for_bare_python_repo() {
        let dir = tempdir().unwrap();
        write(dir.path(), "pyproject.toml");
        assert!(!pytest_configured(dir.path()));
    }

    #[test]
    fn pytest_configured_true_for_pyproject_tool_table() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"x\"\n\n[tool.pytest.ini_options]\nminversion = \"7.0\"\n",
        )
        .unwrap();
        assert!(pytest_configured(dir.path()));
    }

    #[test]
    fn pytest_configured_true_for_pytest_ini() {
        let dir = tempdir().unwrap();
        write(dir.path(), "pytest.ini");
        assert!(pytest_configured(dir.path()));
    }

    #[test]
    fn pytest_configured_true_for_root_conftest() {
        let dir = tempdir().unwrap();
        write(dir.path(), "conftest.py");
        assert!(pytest_configured(dir.path()));
    }

    #[test]
    fn pytest_configured_true_for_tests_conftest() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("tests")).unwrap();
        write(&dir.path().join("tests"), "conftest.py");
        assert!(pytest_configured(dir.path()));
    }

    #[test]
    fn pytest_configured_false_for_pyproject_without_tool_pytest() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"x\"\n\n[tool.black]\n",
        )
        .unwrap();
        assert!(!pytest_configured(dir.path()));
    }

    // python_entry_point

    #[test]
    fn python_entry_point_true_when_main_py_exists() {
        let dir = tempdir().unwrap();
        write(dir.path(), "main.py");
        assert!(python_entry_point(dir.path()));
    }

    #[test]
    fn python_entry_point_false_when_main_py_absent() {
        let dir = tempdir().unwrap();
        assert!(!python_entry_point(dir.path()));
    }
}
