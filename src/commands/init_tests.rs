use super::*;
use crate::init::{DetectedStack, FakeProjectDetector};
use tempfile::TempDir;

#[test]
fn init_creates_templates_manifest_toml() {
    // Verifies cmd_init_inner wires the scaffold step in after the
    // maestro.toml write succeeds.
    let dir = TempDir::new().expect("create tempdir");
    let detector = FakeProjectDetector::new(vec![DetectedStack::Rust]);
    let code = cmd_init_inner(false, dir.path(), &detector).expect("cmd_init_inner");
    assert_eq!(code, 0);
    let manifest = dir.path().join(".maestro/templates/manifest.toml");
    assert!(
        manifest.exists(),
        ".maestro/templates/manifest.toml must be created by init"
    );
}

#[test]
fn init_manifest_toml_matches_canonical_bytes() {
    let dir = TempDir::new().expect("create tempdir");
    let detector = FakeProjectDetector::new(vec![DetectedStack::Rust]);
    let code = cmd_init_inner(false, dir.path(), &detector).expect("cmd_init_inner");
    assert_eq!(code, 0);

    let written = std::fs::read(dir.path().join(".maestro/templates/manifest.toml"))
        .expect("manifest.toml must be readable after init");
    let canonical = include_bytes!("../../template/.maestro/templates/manifest.toml");
    assert_eq!(
        written.as_slice(),
        canonical.as_slice(),
        "scaffolded manifest.toml must be byte-equal to embedded source"
    );
}

#[test]
fn init_creates_all_template_files() {
    let dir = TempDir::new().expect("create tempdir");
    let detector = FakeProjectDetector::new(vec![DetectedStack::Rust]);
    let code = cmd_init_inner(false, dir.path(), &detector).expect("cmd_init_inner");
    assert_eq!(code, 0);

    for rel in crate::init::scaffold::template_relative_paths() {
        assert!(
            dir.path().join(&rel).exists(),
            "expected scaffolded file: {}",
            rel.display()
        );
    }
}

fn git(dir: &TempDir, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir.path())
        .output()
        .expect("git must be available for repo tests");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn read_maestro_toml(dir: &TempDir) -> String {
    std::fs::read_to_string(dir.path().join("maestro.toml")).expect("maestro.toml written")
}

#[test]
fn init_python_without_pytest_config_uses_unittest_in_both_places() {
    // #1150: `tests/test_*.py` with no pytest markers must yield
    // `python3 -m unittest` in both `[project].test_command` and
    // `[gates].test_command` — a stdlib project gets a runnable gate.
    let dir = TempDir::new().expect("create tempdir");
    std::fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = \"x\"\n",
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("tests")).unwrap();
    std::fs::write(
        dir.path().join("tests/test_sample.py"),
        "def test_x():\n    pass\n",
    )
    .unwrap();
    let detector = FsProjectDetector::new();
    let code = cmd_init_inner(false, dir.path(), &detector).expect("cmd_init_inner");
    assert_eq!(code, 0);
    let toml = read_maestro_toml(&dir);
    assert_eq!(
        toml.matches("test_command = \"python3 -m unittest\"")
            .count(),
        2,
        "expected unittest in [project] and [gates]:\n{toml}"
    );
    let gates = &toml[toml.find("[gates]").expect("[gates] section")..];
    assert!(
        gates.contains("test_command = \"python3 -m unittest\""),
        "gates must carry the unittest default:\n{gates}"
    );
    assert!(
        !toml.contains("test_command = \"pytest\""),
        "no pytest default without pytest config:\n{toml}"
    );
}

#[test]
fn init_python_with_pyproject_pytest_table_keeps_pytest() {
    // #1150: a `[tool.pytest…]` table is a pytest marker; the default
    // stays `pytest` in both places.
    let dir = TempDir::new().expect("create tempdir");
    std::fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = \"x\"\n\n[tool.pytest.ini_options]\nminversion = \"7.0\"\n",
    )
    .unwrap();
    let detector = FsProjectDetector::new();
    let code = cmd_init_inner(false, dir.path(), &detector).expect("cmd_init_inner");
    assert_eq!(code, 0);
    let toml = read_maestro_toml(&dir);
    assert_eq!(
        toml.matches("test_command = \"pytest\"").count(),
        2,
        "pytest config detected — keep pytest in both places:\n{toml}"
    );
}

#[test]
fn init_python_without_main_py_comments_run_command() {
    // #1150: no `main.py` → the default `python main.py` is written as a
    // comment, never as a runnable-but-broken key.
    let dir = TempDir::new().expect("create tempdir");
    std::fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = \"x\"\n",
    )
    .unwrap();
    let detector = FsProjectDetector::new();
    let code = cmd_init_inner(false, dir.path(), &detector).expect("cmd_init_inner");
    assert_eq!(code, 0);
    let toml = read_maestro_toml(&dir);
    assert!(
        !toml.contains("\nrun_command = "),
        "run_command must be commented when no entry point exists:\n{toml}"
    );
    assert!(
        toml.contains("# run_command = \"python main.py\""),
        "commented default guides manual fill:\n{toml}"
    );
}

#[test]
fn init_python_with_main_py_keeps_uncommented_run_command() {
    let dir = TempDir::new().expect("create tempdir");
    std::fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = \"x\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("main.py"), "print(\"hi\")\n").unwrap();
    let detector = FsProjectDetector::new();
    let code = cmd_init_inner(false, dir.path(), &detector).expect("cmd_init_inner");
    assert_eq!(code, 0);
    let toml = read_maestro_toml(&dir);
    assert!(
        toml.contains("\nrun_command = \"python main.py\""),
        "entry point present — keep the runnable default:\n{toml}"
    );
}

#[test]
fn init_fills_repo_from_github_origin() {
    // #1150: `origin` parsing as a GitHub URL fills `project.repo` as
    // `owner/name` instead of leaving it empty.
    let dir = TempDir::new().expect("create tempdir");
    git(&dir, &["init", "-q"]);
    git(
        &dir,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/acme/widgets.git",
        ],
    );
    let detector = FakeProjectDetector::new(vec![DetectedStack::Rust]);
    let code = cmd_init_inner(false, dir.path(), &detector).expect("cmd_init_inner");
    assert_eq!(code, 0);
    let toml = read_maestro_toml(&dir);
    assert!(
        toml.contains("repo = \"acme/widgets\""),
        "repo must be filled from origin:\n{toml}"
    );
}

#[test]
fn init_writes_runtime_gitignore_entries_once() {
    // Done-when #1151: init writes the five runtime entries; a --reset
    // re-run must not duplicate them.
    let dir = TempDir::new().expect("create tempdir");
    let detector = FakeProjectDetector::new(vec![DetectedStack::Rust]);
    let code = cmd_init_inner(false, dir.path(), &detector).expect("cmd_init_inner");
    assert_eq!(code, 0);
    let ignore_path = dir.path().join(".gitignore");
    let first = std::fs::read_to_string(&ignore_path).expect(".gitignore written by init");
    for entry in crate::init::ignore::RUNTIME_ENTRIES {
        assert_eq!(
            first.lines().filter(|line| line == entry).count(),
            1,
            "runtime entry {entry} must appear exactly once:\n{first}"
        );
    }

    // Re-run through the real path: only --reset reaches the scaffold and
    // ignore step again (a plain re-run exits early: maestro.toml exists).
    let code = cmd_init_inner(true, dir.path(), &detector).expect("cmd_init_inner reset");
    assert_eq!(code, 0);
    let second = std::fs::read_to_string(&ignore_path).expect(".gitignore still present");
    assert_eq!(
        second, first,
        "--reset re-run must leave .gitignore unchanged"
    );
}

#[test]
fn init_keeps_existing_gitignore_and_adds_entries_missing_from_the_block() {
    // Done-when #1151: an existing .gitignore keeps its content and its
    // maestro block receives every entry it lacked. The user's own exact
    // line does not count as coverage (security follow-up: a foreign line
    // cannot vouch under gitignore's last-match-wins), so maestro-state.json
    // is added inside the block too, after the user's own line.
    let dir = TempDir::new().expect("create tempdir");
    std::fs::write(
        dir.path().join(".gitignore"),
        "__pycache__/\n*.pyc\nmaestro-state.json\n",
    )
    .expect("seed .gitignore");
    let detector = FakeProjectDetector::new(vec![DetectedStack::Rust]);
    let code = cmd_init_inner(false, dir.path(), &detector).expect("cmd_init_inner");
    assert_eq!(code, 0);
    let content = std::fs::read_to_string(dir.path().join(".gitignore")).expect("read back");
    assert!(
        content.starts_with("__pycache__/\n*.pyc\nmaestro-state.json\n"),
        "user content must survive verbatim:\n{content}"
    );
    for entry in [
        ".maestro/logs/",
        ".maestro/worktrees/",
        "maestro-state.json.lock",
        "maestro.log",
    ] {
        assert_eq!(
            content.lines().filter(|line| *line == entry).count(),
            1,
            "entry {entry} added exactly once (inside the block):\n{content}"
        );
    }
    assert_eq!(
        content
            .lines()
            .filter(|line| *line == "maestro-state.json")
            .count(),
        2,
        "user's own line plus the block entry:\n{content}"
    );
    assert!(
        content.ends_with(concat!(
            "# --- maestro ---\n",
            ".maestro/logs/\n",
            ".maestro/worktrees/\n",
            "maestro-state.json\n",
            "maestro-state.json.lock\n",
            "maestro.log\n",
            "# --- end maestro ---\n",
        )),
        "the appended block must carry all five entries, after the user's content:\n{content}"
    );
}

#[test]
fn first_origin_remote_url_prefers_fetch_url() {
    let remote = "upstream\thttps://example.com/upstream.git (fetch)\n\
                  origin\tgit@github.com:owner/repo.git (push)\n\
                  origin\thttps://github.com/owner/repo.git (fetch)\n";
    assert_eq!(
        first_origin_remote_url(remote).as_deref(),
        Some("https://github.com/owner/repo.git")
    );
}

#[test]
fn validate_azure_devops_org_accepts_supported_forms() {
    assert!(validate_azure_devops_organization_url(
        "https://dev.azure.com/MyOrg"
    ));
    assert!(validate_azure_devops_organization_url(
        "https://MyOrg.visualstudio.com"
    ));
}

#[test]
fn validate_azure_devops_org_rejects_invalid_forms() {
    for input in [
        "http://dev.azure.com/MyOrg",
        "https://dev.azure.com/",
        "https://dev.azure.com/MyOrg/Project",
        "https://MyOrg@dev.azure.com/MyOrg",
        "https://example.com/MyOrg",
        "dev.azure.com/MyOrg",
        "",
    ] {
        assert!(
            !validate_azure_devops_organization_url(input),
            "accepted {input:?}"
        );
    }
}
