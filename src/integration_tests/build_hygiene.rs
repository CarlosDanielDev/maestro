//! Drift guard for the committed-lockfile build contract (#1039).
//!
//! `Cargo.lock` must be committed and every `cargo build` / `cargo test` /
//! `cargo clippy` in CI and release workflows must run with `--locked`, so
//! release binaries and the audited dependency graph are exactly the graph
//! resolved in git. Single test:
//! - `build_hygiene_no_drift`: checks `.gitignore`, the tracked lockfile,
//!   the workflow flags, the lockfile-dirty CI step, and the Dependabot
//!   config. Fails listing every violation at once.

use std::path::PathBuf;
use std::process::Command;

fn repo_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn read(rel: &str) -> String {
    let path = repo_path(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Workflow lines invoking a lockfile-sensitive cargo command that lack
/// `--locked`.
fn unlocked_cargo_lines(name: &str, contents: &str) -> Vec<String> {
    contents
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let line = line.trim_start();
            line.contains("cargo build")
                || line.contains("cargo test")
                || line.contains("cargo clippy")
        })
        .filter(|(_, line)| !line.contains("--locked"))
        .map(|(i, line)| format!("{name}:{}: `{}` runs without --locked", i + 1, line.trim()))
        .collect()
}

#[test]
fn build_hygiene_no_drift() {
    let mut violations: Vec<String> = Vec::new();

    // 1. `.gitignore` must not ignore the lockfile.
    let gitignore = read(".gitignore");
    for (i, line) in gitignore.lines().enumerate() {
        let t = line.trim();
        if t == "Cargo.lock" || t == "/Cargo.lock" {
            violations.push(format!(
                ".gitignore:{}: `{t}` still ignores the lockfile",
                i + 1
            ));
        }
    }

    // 2. The lockfile must exist on disk and be tracked by git.
    if !repo_path("Cargo.lock").is_file() {
        violations.push(
            "Cargo.lock missing at repo root — run `cargo generate-lockfile` and commit it"
                .to_string(),
        );
    }
    let tracked = Command::new("git")
        .args(["ls-files", "Cargo.lock"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap_or_else(|e| panic!("git must be on PATH: {e}"));
    if String::from_utf8_lossy(&tracked.stdout).trim().is_empty() {
        violations.push(
            "`git ls-files Cargo.lock` is empty — the committed lockfile is the shipped graph; \
             run `git add Cargo.lock`"
                .to_string(),
        );
    }

    // 3. Release builds resolve nothing beyond the committed lockfile.
    let release = read(".github/workflows/release.yml");
    violations.extend(unlocked_cargo_lines("release.yml", &release));

    // 4. CI test/clippy runs are locked, the lockfile is never regenerated
    //    on the runner, and a stale lockfile fails the job.
    let ci = read(".github/workflows/ci.yml");
    violations.extend(unlocked_cargo_lines("ci.yml", &ci));
    for (i, line) in ci.lines().enumerate() {
        if line.contains("cargo generate-lockfile") {
            violations.push(format!(
                "ci.yml:{}: `cargo generate-lockfile` reintroduced — it audits a graph nobody ships",
                i + 1
            ));
        }
    }
    if !ci.contains("git diff --exit-code Cargo.lock") {
        violations.push(
            "ci.yml: missing `git diff --exit-code Cargo.lock` step — a stale committed \
             lockfile must fail CI"
                .to_string(),
        );
    }

    // 5. Dependabot keeps the committed lockfile updated.
    let dependabot_path = repo_path(".github/dependabot.yml");
    match std::fs::read_to_string(&dependabot_path) {
        Ok(contents) => {
            if !contents.contains("package-ecosystem: cargo") {
                violations.push(
                    ".github/dependabot.yml: missing `package-ecosystem: cargo` entry".to_string(),
                );
            }
        }
        Err(_) => violations.push(
            ".github/dependabot.yml missing — the committed lockfile needs Dependabot \
             update PRs"
                .to_string(),
        ),
    }

    assert!(
        violations.is_empty(),
        "committed-lockfile build contract violated:\n  {}",
        violations.join("\n  ")
    );
}
