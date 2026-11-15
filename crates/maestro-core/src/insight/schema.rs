//! Versioned artifact schema for `maestro insight scan`.
//!
//! Single source of truth for the `docs/insight/scan.json` shape — the contract
//! between the maestro extractor and the portfolio renderer. Bump
//! [`Scan::schema_version`] on any breaking change to the on-disk shape; the
//! renderer tolerates unknown fields so additive changes stay compatible.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Scan {
    pub schema_version: u32,
    pub generated_at: String,
    pub commit_sha: String,
    pub repo_stats: RepoStats,
    pub features: Vec<Feature>,
    pub modules: Vec<Module>,
    pub design_system: DesignSystem,
    pub architecture: Architecture,
    pub coverage: Coverage,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceType {
    Cli,
    TuiMode,
    SlashCommand,
    Subagent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Feature {
    pub id: String,
    pub surface_type: SurfaceType,
    pub name: String,
    pub entry_points: Vec<String>,
    pub modules: Vec<String>,
    pub summary_static: String,
    /// Filled by the AI narration phase (P6); serialized as `null` until then.
    #[serde(default)]
    pub behavior_narrative: Option<String>,
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub related: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Module {
    pub path: String,
    pub loc: u64,
    pub public_api: Vec<String>,
    pub doc_comment: Option<String>,
    pub depends_on: Vec<String>,
    pub feature_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RepoStats {
    pub loc_by_lang: BTreeMap<String, u64>,
    pub file_count: u64,
    pub commits: u64,
    pub contributors: u64,
    pub first_commit_date: Option<String>,
    pub releases: Vec<Release>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Release {
    pub version: String,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DesignSystem {
    pub palette: Vec<NamedValue>,
    pub styles: Vec<NamedValue>,
    pub icons: Vec<NamedValue>,
    pub mascot: Option<Mascot>,
    pub layout_conventions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NamedValue {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mascot {
    pub name: String,
    pub frame_count: u32,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Architecture {
    pub layers: Vec<String>,
    pub module_graph_edges: Vec<Edge>,
    pub entry_binaries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Edge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Coverage {
    pub surfaces_total: u64,
    pub surfaces_documented: u64,
    pub modules_orphaned: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_serializes_with_schema_version_and_null_narrative() {
        let scan = Scan {
            schema_version: 1,
            generated_at: "2026-06-14T00:00:00Z".into(),
            commit_sha: "abc123".into(),
            repo_stats: RepoStats::default(),
            features: vec![Feature {
                id: "cli-run".into(),
                surface_type: SurfaceType::Cli,
                name: "run".into(),
                entry_points: vec!["src/cli.rs:Commands::Run".into()],
                modules: vec!["src/session".into()],
                summary_static: "Runs a session.".into(),
                behavior_narrative: None,
                since: None,
                related: vec![],
            }],
            modules: vec![],
            design_system: DesignSystem::default(),
            architecture: Architecture::default(),
            coverage: Coverage::default(),
        };
        let json = serde_json::to_string(&scan).unwrap();
        assert!(json.contains("\"schema_version\":1"));
        assert!(json.contains("\"surface_type\":\"cli\""));
        assert!(json.contains("\"behavior_narrative\":null"));
    }
}
