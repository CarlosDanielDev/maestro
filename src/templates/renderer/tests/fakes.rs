//! Renderer-specific test fakes (recursion + termination scenarios).
//!
//! `FakeRules` and the shared `unsupported()` helper live in
//! `crate::templates::test_fakes` — reused by integration tests.

use std::path::Path;

use crate::templates::TemplateError;
use crate::templates::provider_rules::TemplateProviderRules;
pub(super) use crate::templates::test_fakes::FakeRules;
use crate::templates::test_fakes::unsupported;

pub(super) struct RecursiveIncludeRules;
impl TemplateProviderRules for RecursiveIncludeRules {
    fn target_dir(&self) -> Option<&'static Path> {
        None
    }
    fn invoke_subagent(&self, _: &str, _: &str) -> Result<String, TemplateError> {
        Err(unsupported("INVOKE_SUBAGENT"))
    }
    fn hook_gate(&self, _: &str, _: &str) -> Result<String, TemplateError> {
        Err(unsupported("HOOK_GATE"))
    }
    fn include(&self, path: &Path) -> Result<String, TemplateError> {
        Ok(format!("{{{{INCLUDE path=\"{}\"}}}}", path.display()))
    }
    fn subagent_list(&self) -> Result<String, TemplateError> {
        Err(unsupported("SUBAGENT_LIST"))
    }
    fn skill_link(&self, _: &str) -> Result<String, TemplateError> {
        Err(unsupported("SKILL"))
    }
}

pub(super) struct TerminatingIncludeRules {
    pub(super) cap: usize,
    pub(super) counter: std::sync::atomic::AtomicUsize,
}
impl TemplateProviderRules for TerminatingIncludeRules {
    fn target_dir(&self) -> Option<&'static Path> {
        None
    }
    fn invoke_subagent(&self, _: &str, _: &str) -> Result<String, TemplateError> {
        Ok(String::new())
    }
    fn hook_gate(&self, _: &str, _: &str) -> Result<String, TemplateError> {
        Ok(String::new())
    }
    fn include(&self, path: &Path) -> Result<String, TemplateError> {
        let n = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if n + 1 >= self.cap {
            Ok("TERMINAL".to_string())
        } else {
            Ok(format!("{{{{INCLUDE path=\"{}\"}}}}", path.display()))
        }
    }
    fn subagent_list(&self) -> Result<String, TemplateError> {
        Ok(String::new())
    }
    fn skill_link(&self, _: &str) -> Result<String, TemplateError> {
        Ok(String::new())
    }
}
