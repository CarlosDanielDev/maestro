//! Per-provider placeholder-expansion rules.
//!
//! Each concrete `AgentProvider` returns a `&'static dyn TemplateProviderRules`
//! from its `template_rules()` method. The default impl on `AgentProvider`
//! returns [`NullRules`], which fails closed on every placeholder. Concrete
//! provider rule modules live in `claude.rs`, `codex.rs`, and
//! `http_generic.rs`.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

mod claude;
mod codex;
mod http_generic;

pub use claude::claude_rules;
pub use codex::codex_rules;
pub use http_generic::http_generic_rules;

use std::path::{Component, Path};

use crate::templates::TemplateError;

/// Read a file under `root`, rejecting any path that escapes the sandbox.
///
/// Sandbox rules:
/// - `path` must be relative.
/// - Every component of `path` must be `Component::Normal` (no `.`, `..`,
///   prefix, or root-dir markers).
/// - After canonicalization, the resolved file must remain a descendant of
///   `root` (`starts_with` on canonicalized paths).
///
/// Errors:
/// - `TemplateError::SandboxEscape` for absolute paths, non-Normal components,
///   or post-canonicalization escapes (symlinks pointing outside `root`).
/// - `TemplateError::FileMissing` if the resolved file does not exist.
/// - `TemplateError::Io` for any other I/O failure.
pub(super) fn read_sandboxed(root: &Path, path: &Path) -> Result<String, TemplateError> {
    let display_path = path.to_string_lossy().into_owned();
    let root_display = root.to_string_lossy().into_owned();
    let escape = || TemplateError::SandboxEscape {
        path: display_path.clone(),
        root: root_display.clone(),
    };
    if path.is_absolute() {
        return Err(escape());
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(escape());
    }
    let full = root.join(path);
    let canonical_root = std::fs::canonicalize(root).map_err(|source| TemplateError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    let canonical_full = std::fs::canonicalize(&full).map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => TemplateError::FileMissing { path: full.clone() },
        _ => TemplateError::Io {
            path: full.clone(),
            source,
        },
    })?;
    if !canonical_full.starts_with(&canonical_root) {
        return Err(escape());
    }
    std::fs::read_to_string(&canonical_full).map_err(|source| TemplateError::Io {
        path: canonical_full,
        source,
    })
}

/// Read `.claude/skills/<name>/SKILL.md` through the sandbox reader.
///
/// Shared by `CodexRules::skill_link` and `HttpGenericRules::skill_link`, both
/// of which inline the skill body verbatim into the rendered template.
pub(super) fn read_skill_body(name: &str) -> Result<String, TemplateError> {
    const SKILLS_ROOT: &str = ".claude/skills";
    let skill_path = format!("{name}/SKILL.md");
    read_sandboxed(Path::new(SKILLS_ROOT), Path::new(&skill_path))
}

/// Per-provider rendering rules for the five canonical placeholder kinds.
///
/// `Send + Sync` so renderer state can be held across threads. Every method
/// is fallible — concrete providers may return `TemplateError` if they cannot
/// satisfy a given placeholder (e.g., HTTP providers have no `target_dir`).
pub trait TemplateProviderRules: Send + Sync {
    fn target_dir(&self) -> Option<&'static Path>;

    fn invoke_subagent(&self, name: &str, prompt: &str) -> Result<String, TemplateError>;

    fn hook_gate(&self, script: &str, args: &str) -> Result<String, TemplateError>;

    fn include(&self, path: &Path) -> Result<String, TemplateError>;

    fn subagent_list(&self) -> Result<String, TemplateError>;

    fn skill_link(&self, name: &str) -> Result<String, TemplateError>;
}

/// Fail-closed stub returned by the default `AgentProvider::template_rules()`.
///
/// Every method returns `TemplateError::UnsupportedByProvider`. This is NOT
/// silent pass-through — concrete providers must override
/// `AgentProvider::template_rules()` to enable rendering.
#[derive(Debug, Default)]
pub struct NullRules;

impl TemplateProviderRules for NullRules {
    fn target_dir(&self) -> Option<&'static Path> {
        None
    }

    fn invoke_subagent(&self, _name: &str, _prompt: &str) -> Result<String, TemplateError> {
        Err(TemplateError::UnsupportedByProvider {
            name: "INVOKE_SUBAGENT".to_string(),
            reason: "no provider rules registered (NullRules)".to_string(),
        })
    }

    fn hook_gate(&self, _script: &str, _args: &str) -> Result<String, TemplateError> {
        Err(TemplateError::UnsupportedByProvider {
            name: "HOOK_GATE".to_string(),
            reason: "no provider rules registered (NullRules)".to_string(),
        })
    }

    fn include(&self, _path: &Path) -> Result<String, TemplateError> {
        Err(TemplateError::UnsupportedByProvider {
            name: "INCLUDE".to_string(),
            reason: "no provider rules registered (NullRules)".to_string(),
        })
    }

    fn subagent_list(&self) -> Result<String, TemplateError> {
        Err(TemplateError::UnsupportedByProvider {
            name: "SUBAGENT_LIST".to_string(),
            reason: "no provider rules registered (NullRules)".to_string(),
        })
    }

    fn skill_link(&self, _name: &str) -> Result<String, TemplateError> {
        Err(TemplateError::UnsupportedByProvider {
            name: "SKILL".to_string(),
            reason: "no provider rules registered (NullRules)".to_string(),
        })
    }
}

/// Shared `'static` reference to the [`NullRules`] singleton.
pub fn null_rules() -> &'static dyn TemplateProviderRules {
    static NULL: NullRules = NullRules;
    &NULL
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_err_unsupported(result: Result<String, TemplateError>, expected_name: &str) {
        match result {
            Err(TemplateError::UnsupportedByProvider { name, .. }) => {
                assert_eq!(name, expected_name);
            }
            other => panic!("expected UnsupportedByProvider, got {other:?}"),
        }
    }

    #[test]
    fn null_rules_invoke_subagent_fails_closed() {
        assert_err_unsupported(
            null_rules().invoke_subagent("foo", "do stuff"),
            "INVOKE_SUBAGENT",
        );
    }

    #[test]
    fn null_rules_hook_gate_fails_closed() {
        assert_err_unsupported(null_rules().hook_gate("script.sh", ""), "HOOK_GATE");
    }

    #[test]
    fn null_rules_include_fails_closed() {
        assert_err_unsupported(null_rules().include(Path::new("core/x.md")), "INCLUDE");
    }

    #[test]
    fn null_rules_subagent_list_fails_closed() {
        assert_err_unsupported(null_rules().subagent_list(), "SUBAGENT_LIST");
    }

    #[test]
    fn null_rules_skill_link_fails_closed() {
        assert_err_unsupported(null_rules().skill_link("project-patterns"), "SKILL");
    }

    #[test]
    fn null_rules_target_dir_is_none() {
        assert!(null_rules().target_dir().is_none());
    }

    #[test]
    fn null_rules_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NullRules>();
    }

    mod sandbox {
        use super::super::read_sandboxed;
        use crate::templates::TemplateError;
        use std::path::{Path, PathBuf};

        fn manifest_dir() -> PathBuf {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        }

        #[test]
        fn reads_existing_file_under_root() {
            let root = manifest_dir().join(".maestro/templates");
            let out = read_sandboxed(&root, Path::new("core/premises.md")).expect("ok");
            assert!(
                out.contains("YOU ARE THE ONLY AGENT THAT WRITES CODE"),
                "unexpected content: {out:.120}"
            );
        }

        #[test]
        fn rejects_absolute_path() {
            let root = manifest_dir().join(".maestro/templates");
            let err = read_sandboxed(&root, Path::new("/etc/passwd")).unwrap_err();
            assert!(
                matches!(err, TemplateError::SandboxEscape { .. }),
                "{err:?}"
            );
        }

        #[test]
        fn rejects_parent_dir_component() {
            let root = manifest_dir().join(".maestro/templates");
            let err = read_sandboxed(&root, Path::new("../Cargo.toml")).unwrap_err();
            assert!(
                matches!(err, TemplateError::SandboxEscape { .. }),
                "{err:?}"
            );
        }

        #[test]
        fn rejects_cur_dir_component() {
            let root = manifest_dir().join(".maestro/templates");
            let err = read_sandboxed(&root, Path::new("./core/premises.md")).unwrap_err();
            assert!(
                matches!(err, TemplateError::SandboxEscape { .. }),
                "{err:?}"
            );
        }

        #[test]
        fn missing_file_returns_file_missing() {
            let root = manifest_dir().join(".maestro/templates");
            let err = read_sandboxed(&root, Path::new("core/does-not-exist.md")).unwrap_err();
            assert!(matches!(err, TemplateError::FileMissing { .. }), "{err:?}");
        }

        #[test]
        #[cfg(unix)]
        fn symlink_escape_is_rejected_by_starts_with_check() {
            use std::os::unix::fs::symlink;

            let dir = tempfile::tempdir().expect("tempdir");
            let link = dir.path().join("escape.md");
            symlink("/etc/passwd", &link).expect("symlink");

            let err = read_sandboxed(dir.path(), Path::new("escape.md")).unwrap_err();
            assert!(
                matches!(err, TemplateError::SandboxEscape { .. }),
                "symlink escape must be caught by canonicalize+starts_with: {err:?}"
            );
        }
    }
}
