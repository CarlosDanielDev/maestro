//! Binary-embedded built-in team presets.
//! See spec §4 "Built-in seed list (v1, ship 5)".

#![allow(dead_code)]

use crate::orchestration::loader::RawTeam;
use crate::orchestration::team::{SourceTier, TeamConfig};

const DEFAULT_CODER: &str = include_str!("default-coder.toml");
const DEFAULT_RESEARCHER: &str = include_str!("default-researcher.toml");
const DEFAULT_TRIAGER: &str = include_str!("default-triager.toml");
const DEFAULT_REVIEWER: &str = include_str!("default-reviewer.toml");
const DEFAULT_DOCS: &str = include_str!("default-docs.toml");

const ALL: &[(&str, &str)] = &[
    ("default-coder", DEFAULT_CODER),
    ("default-researcher", DEFAULT_RESEARCHER),
    ("default-triager", DEFAULT_TRIAGER),
    ("default-reviewer", DEFAULT_REVIEWER),
    ("default-docs", DEFAULT_DOCS),
];

pub(crate) fn load_all() -> Vec<RawTeam> {
    ALL.iter()
        .map(|(name, src)| {
            let config: TeamConfig = toml::from_str(src)
                .unwrap_or_else(|e| panic!("built-in {name} fails to parse: {e}"));
            RawTeam {
                name: name.to_string(),
                config,
                source_tier: SourceTier::BuiltIn,
                source_path: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builtins_parse() {
        let raws = load_all();
        assert_eq!(raws.len(), 5);
        for r in &raws {
            assert_eq!(r.source_tier, SourceTier::BuiltIn);
            assert!(
                r.config
                    .min_agents
                    .as_ref()
                    .unwrap()
                    .contains(&"claude".to_string())
            );
        }
    }
}
