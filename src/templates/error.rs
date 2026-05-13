//! Typed error enum for the template render engine.
//!
//! Public seam between the renderer and its callers. Callers branch on the
//! variant to drive exit codes (sync-templates CI) and remediation messages.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("unknown placeholder `{name}` at offset {offset} in `{source_path}`")]
    UnknownPlaceholder {
        name: String,
        offset: usize,
        source_path: String,
    },

    #[error("invalid placeholder `{name}` at offset {offset} in `{source_path}`: {reason}")]
    InvalidPlaceholder {
        name: String,
        offset: usize,
        source_path: String,
        reason: String,
    },

    #[error("include path `{path}` escapes templates root `{root}`")]
    SandboxEscape { path: String, root: String },

    #[error("include cycle or depth limit exceeded at `{path}` (depth {depth})")]
    IncludeCycle { path: String, depth: usize },

    #[error("manifest not found at `{path}`")]
    ManifestMissing { path: PathBuf },

    #[error("malformed manifest at `{path}`: {source}")]
    ManifestParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("template file not found: `{path}`")]
    FileMissing { path: PathBuf },

    #[error("reading `{path}`")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("unterminated placeholder starting at offset {offset} in `{source_path}`")]
    UnterminatedPlaceholder { offset: usize, source_path: String },

    #[error("provider rules cannot render placeholder `{name}`: {reason}")]
    UnsupportedByProvider { name: String, reason: String },

    #[error("unsupported manifest version {found} (expected {expected})")]
    UnsupportedManifestVersion { found: u32, expected: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_placeholder_display_contains_fields() {
        let err = TemplateError::UnknownPlaceholder {
            name: "BOGUS".to_string(),
            offset: 42,
            source_path: "commands/foo.md".to_string(),
        };
        let s = format!("{err}");
        assert!(s.contains("BOGUS"), "{s}");
        assert!(s.contains("42"), "{s}");
        assert!(s.contains("commands/foo.md"), "{s}");
    }

    #[test]
    fn unterminated_placeholder_display_contains_fields() {
        let err = TemplateError::UnterminatedPlaceholder {
            offset: 0,
            source_path: "core/x.md".to_string(),
        };
        let s = format!("{err}");
        assert!(s.contains("core/x.md"), "{s}");
    }

    #[test]
    fn sandbox_escape_display_contains_path_and_root() {
        let err = TemplateError::SandboxEscape {
            path: "/etc/passwd".to_string(),
            root: "/tmp/tpl".to_string(),
        };
        let s = format!("{err}");
        assert!(s.contains("/etc/passwd"), "{s}");
        assert!(s.contains("/tmp/tpl"), "{s}");
    }

    #[test]
    fn include_cycle_display_contains_path_and_depth() {
        let err = TemplateError::IncludeCycle {
            path: "core/x.md".to_string(),
            depth: 10,
        };
        let s = format!("{err}");
        assert!(s.contains("core/x.md"), "{s}");
        assert!(s.contains("10"), "{s}");
    }

    #[test]
    fn manifest_parse_chains_source() {
        let bad = "not valid = [";
        let toml_err = toml::from_str::<toml::Value>(bad).err();
        let toml_err = match toml_err {
            Some(e) => e,
            None => panic!("fixture must produce a parse error"),
        };
        let err = TemplateError::ManifestParse {
            path: PathBuf::from("manifest.toml"),
            source: toml_err,
        };
        let source = std::error::Error::source(&err);
        assert!(
            source.is_some(),
            "ManifestParse must chain underlying source"
        );
    }

    #[test]
    fn unsupported_by_provider_display_contains_fields() {
        let err = TemplateError::UnsupportedByProvider {
            name: "INVOKE_SUBAGENT".to_string(),
            reason: "provider does not support subagents".to_string(),
        };
        let s = format!("{err}");
        assert!(s.contains("INVOKE_SUBAGENT"), "{s}");
        assert!(s.contains("does not support subagents"), "{s}");
    }

    #[test]
    fn unsupported_manifest_version_display_contains_versions() {
        let err = TemplateError::UnsupportedManifestVersion {
            found: 2,
            expected: 1,
        };
        let s = format!("{err}");
        assert!(s.contains('2'), "{s}");
        assert!(s.contains('1'), "{s}");
    }
}
