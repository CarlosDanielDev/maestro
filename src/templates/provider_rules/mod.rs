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

use std::path::Path;

use crate::templates::TemplateError;

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
}
