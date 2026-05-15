//! Manifest TOML loader for `.maestro/templates/manifest.toml`.
//!
//! The manifest declares per-placeholder validation hints and per-provider
//! metadata. The renderer's *placeholder vocabulary* is hard-coded; the
//! manifest is informational metadata only (required args, target dirs).

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// Manifest fields/methods are populated for #706 (sync-templates CLI) and
// downstream consumers; load() exercises them via serde at parse time.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::templates::TemplateError;

pub const SUPPORTED_VERSION: u32 = 1;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub meta: ManifestMeta,
    #[serde(default)]
    pub placeholders: BTreeMap<String, ManifestPlaceholder>,
    #[serde(default)]
    pub providers: BTreeMap<String, ManifestProvider>,
    #[serde(default)]
    pub subagents: Vec<ManifestSubagent>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestMeta {
    pub version: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_templates_root")]
    pub templates_root: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestPlaceholder {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required_args: Vec<String>,
    #[serde(default)]
    pub max_depth: Option<u32>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestSubagent {
    pub slug: String,
    pub purpose: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestProvider {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub target_dir: String,
    #[serde(default)]
    pub inline_skills: bool,
}

fn default_templates_root() -> String {
    ".maestro/templates".to_string()
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self, TemplateError> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(TemplateError::ManifestMissing {
                    path: PathBuf::from(path),
                });
            }
            Err(e) => {
                return Err(TemplateError::Io {
                    path: PathBuf::from(path),
                    source: e,
                });
            }
        };
        let text = String::from_utf8(bytes).map_err(|e| TemplateError::Io {
            path: PathBuf::from(path),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        })?;
        let manifest: Manifest =
            toml::from_str(&text).map_err(|source| TemplateError::ManifestParse {
                path: PathBuf::from(path),
                source,
            })?;
        if manifest.meta.version != SUPPORTED_VERSION {
            return Err(TemplateError::UnsupportedManifestVersion {
                found: manifest.meta.version,
                expected: SUPPORTED_VERSION,
            });
        }
        Ok(manifest)
    }

    pub fn placeholder(&self, name: &str) -> Option<&ManifestPlaceholder> {
        self.placeholders.get(name)
    }

    pub fn provider(&self, id: &str) -> Option<&ManifestProvider> {
        self.providers.get(id)
    }

    pub fn subagents(&self) -> &[ManifestSubagent] {
        &self.subagents
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_tmp(contents: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".toml")
            .tempfile()
            .expect("tempfile create");
        f.write_all(contents.as_bytes()).expect("tempfile write");
        f
    }

    #[test]
    fn load_happy_path_returns_manifest() {
        let f = write_tmp(
            r#"
[meta]
version = 1
description = "test manifest"

[placeholders.INVOKE_SUBAGENT]
description = "invoke a subagent"
required_args = ["name", "prompt"]

[placeholders.SUBAGENT_LIST]
description = "list subagents"
required_args = []

[providers.claude]
display_name = "Claude Code"
target_dir = ".claude/commands"
"#,
        );
        let m = Manifest::load(f.path()).expect("load ok");
        assert_eq!(m.meta.version, 1);
        let p = m
            .placeholder("INVOKE_SUBAGENT")
            .expect("placeholder present");
        assert!(p.required_args.contains(&"name".to_string()));
        assert!(p.required_args.contains(&"prompt".to_string()));
        assert!(m.placeholder("NONEXISTENT").is_none());
        let cp = m.provider("claude").expect("provider present");
        assert_eq!(cp.display_name, "Claude Code");
    }

    #[test]
    fn load_missing_file_returns_manifest_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nope.toml");
        match Manifest::load(&path) {
            Err(TemplateError::ManifestMissing { path: p }) => {
                assert_eq!(p, path);
            }
            other => panic!("expected ManifestMissing, got {other:?}"),
        }
    }

    #[test]
    fn load_invalid_toml_returns_manifest_parse() {
        let f = write_tmp("not valid = [");
        match Manifest::load(f.path()) {
            Err(TemplateError::ManifestParse { .. }) => {}
            other => panic!("expected ManifestParse, got {other:?}"),
        }
    }

    #[test]
    fn load_unknown_field_returns_parse_error() {
        let f = write_tmp(
            r#"
[meta]
version = 1
extra_field = "oops"
"#,
        );
        match Manifest::load(f.path()) {
            Err(TemplateError::ManifestParse { .. }) => {}
            other => panic!("expected ManifestParse for unknown field, got {other:?}"),
        }
    }

    #[test]
    fn load_unsupported_version_returns_error() {
        let f = write_tmp(
            r#"
[meta]
version = 99
"#,
        );
        match Manifest::load(f.path()) {
            Err(TemplateError::UnsupportedManifestVersion {
                found: 99,
                expected: 1,
            }) => {}
            other => panic!("expected UnsupportedManifestVersion, got {other:?}"),
        }
    }

    #[test]
    fn placeholder_lookup_is_case_sensitive() {
        let f = write_tmp(
            r#"
[meta]
version = 1

[placeholders.INVOKE_SUBAGENT]
required_args = ["name", "prompt"]
"#,
        );
        let m = Manifest::load(f.path()).expect("load ok");
        assert!(m.placeholder("invoke_subagent").is_none());
        assert!(m.placeholder("INVOKE_SUBAGENT").is_some());
    }

    #[test]
    fn provider_lookup_unknown_returns_none() {
        let f = write_tmp(
            r#"
[meta]
version = 1
"#,
        );
        let m = Manifest::load(f.path()).expect("load ok");
        assert!(m.provider("cursor").is_none());
    }

    #[test]
    fn subagents_default_to_empty_when_absent() {
        let f = write_tmp(
            r#"
[meta]
version = 1
"#,
        );
        let m = Manifest::load(f.path()).expect("load ok");
        assert!(m.subagents().is_empty());
    }

    #[test]
    fn subagents_preserve_declaration_order() {
        let f = write_tmp(
            r#"
[meta]
version = 1

[[subagents]]
slug = "subagent-idea-triager"
purpose = "Triage"

[[subagents]]
slug = "subagent-gatekeeper"
purpose = "Gate"

[[subagents]]
slug = "subagent-qa"
purpose = "QA"
"#,
        );
        let m = Manifest::load(f.path()).expect("load ok");
        let slugs: Vec<&str> = m.subagents().iter().map(|s| s.slug.as_str()).collect();
        assert_eq!(
            slugs,
            [
                "subagent-idea-triager",
                "subagent-gatekeeper",
                "subagent-qa"
            ]
        );
        assert_eq!(m.subagents()[0].purpose, "Triage");
    }

    #[test]
    fn subagent_unknown_field_in_entry_returns_parse_error() {
        let f = write_tmp(
            r#"
[meta]
version = 1

[[subagents]]
slug = "subagent-gatekeeper"
purpose = "Gate"
category = "control"
"#,
        );
        match Manifest::load(f.path()) {
            Err(TemplateError::ManifestParse { .. }) => {}
            other => panic!("expected ManifestParse, got {other:?}"),
        }
    }
}
