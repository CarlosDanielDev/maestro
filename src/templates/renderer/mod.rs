//! Placeholder-expansion engine for canonical command templates.
//!
//! Hand-written tokenizer (not regex). Five hard-coded placeholder kinds:
//! `INVOKE_SUBAGENT`, `HOOK_GATE`, `INCLUDE`, `SUBAGENT_LIST`, `SKILL`.
//! Unknown placeholders fail closed with `TemplateError::UnknownPlaceholder`.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

mod tokenize;

use std::collections::BTreeMap;
use std::path::Path;

use crate::templates::TemplateError;
use crate::templates::provider_rules::TemplateProviderRules;

pub(crate) const MAX_INCLUDE_DEPTH: usize = 8;
pub(crate) const SOURCE_LABEL: &str = "<template>";

pub(crate) use tokenize::tokenize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Token {
    Text(String),
    Placeholder {
        name: String,
        args: BTreeMap<String, String>,
        offset: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlaceholderKind {
    InvokeSubagent,
    HookGate,
    Include,
    SubagentList,
    Skill,
}

impl PlaceholderKind {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "INVOKE_SUBAGENT" => Some(Self::InvokeSubagent),
            "HOOK_GATE" => Some(Self::HookGate),
            "INCLUDE" => Some(Self::Include),
            "SUBAGENT_LIST" => Some(Self::SubagentList),
            "SKILL" => Some(Self::Skill),
            _ => None,
        }
    }

    pub(crate) fn required_args(self) -> &'static [&'static str] {
        match self {
            Self::InvokeSubagent => &["name", "prompt"],
            Self::HookGate => &["script", "args"],
            Self::Include => &["path"],
            Self::SubagentList => &[],
            Self::Skill => &["name"],
        }
    }
}

pub(crate) fn render_with_source(
    input: &str,
    rules: &dyn TemplateProviderRules,
    source_path: &str,
    depth: usize,
) -> Result<String, TemplateError> {
    if depth >= MAX_INCLUDE_DEPTH {
        return Err(TemplateError::IncludeCycle {
            path: source_path.to_string(),
            depth,
        });
    }
    let tokens = tokenize(input, source_path)?;
    let mut out = String::new();
    for tok in tokens {
        match tok {
            Token::Text(t) => out.push_str(&t),
            Token::Placeholder { name, args, offset } => {
                let kind = PlaceholderKind::from_name(&name).ok_or_else(|| {
                    TemplateError::UnknownPlaceholder {
                        name: name.clone(),
                        offset,
                        source_path: source_path.to_string(),
                    }
                })?;
                for required in kind.required_args() {
                    if !args.contains_key(*required) {
                        return Err(TemplateError::InvalidPlaceholder {
                            name,
                            offset,
                            source_path: source_path.to_string(),
                            reason: format!("missing required argument `{required}`"),
                        });
                    }
                }
                let expanded = expand(kind, &name, &args, offset, source_path, rules, depth)?;
                out.push_str(&expanded);
            }
        }
    }
    Ok(out)
}

fn expand(
    kind: PlaceholderKind,
    name: &str,
    args: &BTreeMap<String, String>,
    offset: usize,
    source_path: &str,
    rules: &dyn TemplateProviderRules,
    depth: usize,
) -> Result<String, TemplateError> {
    let lookup = |key: &str| -> Result<&str, TemplateError> {
        args.get(key)
            .map(String::as_str)
            .ok_or_else(|| TemplateError::InvalidPlaceholder {
                name: name.to_string(),
                offset,
                source_path: source_path.to_string(),
                reason: format!("missing `{key}`"),
            })
    };
    match kind {
        PlaceholderKind::InvokeSubagent => {
            let n = lookup("name")?;
            let p = lookup("prompt")?;
            rules.invoke_subagent(n, p)
        }
        PlaceholderKind::HookGate => {
            let s = lookup("script")?;
            let a = lookup("args")?;
            rules.hook_gate(s, a)
        }
        PlaceholderKind::Include => {
            let p = lookup("path")?;
            let raw = rules.include(Path::new(p))?;
            render_with_source(&raw, rules, p, depth + 1)
        }
        PlaceholderKind::SubagentList => rules.subagent_list(),
        PlaceholderKind::Skill => {
            let n = lookup("name")?;
            rules.skill_link(n)
        }
    }
}

pub(crate) fn render(
    input: &str,
    rules: &dyn TemplateProviderRules,
) -> Result<String, TemplateError> {
    render_with_source(input, rules, SOURCE_LABEL, 0)
}

#[cfg(test)]
mod tests;
