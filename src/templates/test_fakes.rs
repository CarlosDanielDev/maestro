//! Shared trait-based fakes for template tests (unit + integration).

#![cfg(test)]
#![allow(dead_code)]

use std::path::Path;

use crate::templates::TemplateError;
use crate::templates::provider_rules::TemplateProviderRules;

pub(crate) struct FakeRules;

impl TemplateProviderRules for FakeRules {
    fn target_dir(&self) -> Option<&'static Path> {
        None
    }
    fn invoke_subagent(&self, name: &str, prompt: &str) -> Result<String, TemplateError> {
        Ok(format!("[INVOKE name={name} prompt={prompt}]"))
    }
    fn hook_gate(&self, script: &str, args: &str) -> Result<String, TemplateError> {
        Ok(format!("[HOOK script={script} args={args}]"))
    }
    fn include(&self, path: &Path) -> Result<String, TemplateError> {
        Ok(format!("[INCLUDE path={}]", path.display()))
    }
    fn subagent_list(&self) -> Result<String, TemplateError> {
        Ok("[SUBAGENT_LIST]".to_string())
    }
    fn skill_link(&self, name: &str) -> Result<String, TemplateError> {
        Ok(format!("[SKILL name={name}]"))
    }
}

pub(crate) fn unsupported(name: &str) -> TemplateError {
    TemplateError::UnsupportedByProvider {
        name: name.to_string(),
        reason: "unused in test".to_string(),
    }
}
