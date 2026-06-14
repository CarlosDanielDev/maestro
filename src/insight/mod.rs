//! `maestro insight` — static maintenance-map extractor.
//!
//! Walks the repository and emits a versioned [`schema::Scan`] artifact
//! (`docs/insight/scan.json`). P1 populates `repo_stats` and `modules`; the
//! `features`, `design_system`, `architecture`, and `coverage` sections are
//! present but empty, filled by later phases.

pub mod coverage;
pub mod features;
pub mod modules;
pub mod repo_stats;
pub mod schema;

use anyhow::Result;
use schema::*;
use std::path::Path;
use std::process::Command;

/// Build the full static [`Scan`] for the repository at `root`.
pub fn scan(root: &Path) -> Scan {
    let mut modules = collect_modules(&root.join("src"));
    let mut features = collect_features(root);
    coverage::map_modules(&mut features, &mut modules);
    let coverage = coverage::compute_coverage(&features, &modules);

    let repo_stats = repo_stats::collect(root);
    let commit_sha = current_sha(root);
    Scan {
        schema_version: 1,
        generated_at: chrono::Utc::now().to_rfc3339(),
        commit_sha,
        repo_stats,
        features,
        modules,
        design_system: DesignSystem::default(),
        architecture: Architecture::default(),
        coverage,
    }
}

/// Collect every user-facing surface from the 4 entry points: the CLI
/// `Commands` enum, the TUI `TuiMode` enum, slash-command templates, and
/// subagent definitions. All reads are best-effort — a missing file or
/// unparseable source contributes nothing rather than aborting the scan.
fn collect_features(root: &Path) -> Vec<Feature> {
    let cli_src = std::fs::read_to_string(root.join("src/cli.rs")).unwrap_or_default();
    let tui_src = std::fs::read_to_string(root.join("src/tui/app/types.rs")).unwrap_or_default();

    let mut features = Vec::new();
    features.extend(features::surfaces_from_enum(
        &cli_src,
        "Commands",
        SurfaceType::Cli,
        "src/cli.rs:Commands",
    ));
    features.extend(features::surfaces_from_enum(
        &tui_src,
        "TuiMode",
        SurfaceType::TuiMode,
        "src/tui/app/types.rs:TuiMode",
    ));
    features.extend(features::surfaces_from_md_dir(
        &read_md_dir(&root.join(".maestro/templates/commands")),
        SurfaceType::SlashCommand,
        ".maestro/templates/commands",
    ));
    features.extend(features::surfaces_from_md_dir(
        &read_md_dir(&root.join(".claude/agents")),
        SurfaceType::Subagent,
        ".claude/agents",
    ));
    features
}

/// Read a directory of `.md` files into `(file_stem, contents)` pairs, sorted by
/// stem for stable output. Best-effort: unreadable dir or files are skipped.
fn read_md_dir(dir: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("md"))
        .filter_map(|p| {
            let stem = p.file_stem()?.to_string_lossy().to_string();
            let contents = std::fs::read_to_string(&p).ok()?;
            Some((stem, contents))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Walk each top-level `src/<name>` directory (or `src/<name>.rs`) into one
/// [`Module`], combining a directory's `.rs` files for analysis and LOC.
fn collect_modules(src: &Path) -> Vec<Module> {
    let Ok(entries) = std::fs::read_dir(src) else {
        return Vec::new();
    };

    // Group source files by module path so a module present as both `foo.rs`
    // and `foo/` (Rust module + submodules) is analyzed once, not twice.
    let mut by_module: std::collections::BTreeMap<String, Vec<std::path::PathBuf>> =
        std::collections::BTreeMap::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            let files = walkdir::WalkDir::new(&path)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("rs"))
                .map(|e| e.path().to_path_buf());
            by_module
                .entry(format!("src/{name}"))
                .or_default()
                .extend(files);
        } else if path.extension().and_then(|x| x.to_str()) == Some("rs") {
            by_module
                .entry(format!("src/{}", name.trim_end_matches(".rs")))
                .or_default()
                .push(path);
        }
    }

    let mut out = Vec::new();
    for (mod_path, files) in by_module {
        let mut combined = String::new();
        let mut loc = 0u64;
        for file_path in &files {
            if let Ok(contents) = std::fs::read_to_string(file_path) {
                loc += contents.lines().count() as u64;
                combined.push_str(&contents);
                combined.push('\n');
            }
        }
        let mut m = modules::analyze_source(&mod_path, &combined);
        m.loc = loc;
        // Self-edges from combining files within the module are meaningless.
        m.depends_on.retain(|d| d != &mod_path);
        out.push(m);
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Current `HEAD` sha, or an empty string when git is unavailable.
fn current_sha(root: &Path) -> String {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// CLI entry: scan `root` and write pretty `docs/insight/scan.json`.
pub fn run_cli(root: &Path) -> Result<()> {
    let scan = scan(root);
    let out_dir = root.join("docs/insight");
    std::fs::create_dir_all(&out_dir)?;
    let path = out_dir.join("scan.json");
    let json = serde_json::to_string_pretty(&scan)?;
    std::fs::write(&path, json + "\n")?;
    println!("wrote {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_walks_src_and_fills_module_loc() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("session")).unwrap();
        std::fs::write(
            src.join("session").join("mod.rs"),
            "//! Session.\npub fn run() {}\n",
        )
        .unwrap();
        let scan = scan(dir.path());
        assert_eq!(scan.schema_version, 1);
        let session = scan
            .modules
            .iter()
            .find(|m| m.path == "src/session")
            .expect("module");
        assert!(session.loc > 0);
        assert!(session.public_api.contains(&"run".to_string()));
    }

    #[test]
    fn collect_modules_merges_file_and_dir_of_same_name() {
        // A module that exists as both `foo.rs` and `foo/` (Rust module +
        // submodules) must yield ONE `src/foo`, not a duplicate.
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("budget")).unwrap();
        std::fs::write(src.join("budget.rs"), "//! Budget.\npub fn cap() {}\n").unwrap();
        std::fs::write(src.join("budget").join("ledger.rs"), "pub fn spend() {}\n").unwrap();

        let modules = collect_modules(&src);

        let budget: Vec<_> = modules.iter().filter(|m| m.path == "src/budget").collect();
        assert_eq!(budget.len(), 1, "foo.rs + foo/ must merge into one module");
        let m = budget[0];
        assert!(m.public_api.contains(&"cap".to_string()), "from budget.rs");
        assert!(
            m.public_api.contains(&"spend".to_string()),
            "from budget/ledger.rs"
        );
        assert!(m.loc >= 3, "loc must sum both files, got {}", m.loc);
    }

    #[test]
    fn scan_with_features_returns_non_trivial_output() {
        // Integration smoke test: run the full pipeline against the real repo.
        // These are lower bounds, not exact counts — lower them cautiously.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let s = scan(root);

        assert!(
            s.features.len() > 40,
            "expected > 40 features from 4 surfaces, got {}",
            s.features.len()
        );
        assert!(
            s.coverage.surfaces_total > 0,
            "coverage.surfaces_total must be non-zero after extraction"
        );
        assert!(
            s.coverage.surfaces_documented <= s.coverage.surfaces_total,
            "documented ({}) cannot exceed total ({})",
            s.coverage.surfaces_documented,
            s.coverage.surfaces_total
        );
    }

    #[test]
    fn run_cli_writes_pretty_json_to_docs_insight() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("lib.rs"), "pub fn hello() {}\n").unwrap();

        run_cli(dir.path()).unwrap();

        let out = dir.path().join("docs/insight/scan.json");
        assert!(out.exists());
        let content = std::fs::read_to_string(&out).unwrap();
        // Pretty-printed, not compact.
        assert!(content.starts_with("{\n"));
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(value["schema_version"], 1);
    }
}
