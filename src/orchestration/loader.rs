//! Three-tier team preset loader. See spec §4 Tier resolution.

#![allow(dead_code)]

use crate::orchestration::team::{ResolvedTeam, RoleBinding, RoleOverride, SourceTier, TeamConfig};
use crate::orchestration::types::{Primitive, TeamRole};
use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Raw (pre-`extends`-merge) entry as loaded from disk or embedded TOML.
#[derive(Debug, Clone)]
pub struct RawTeam {
    pub name: String,
    pub config: TeamConfig,
    pub source_tier: SourceTier,
    pub source_path: Option<PathBuf>,
}

pub struct Loader {
    builtins: Vec<RawTeam>,
    user_dir: Option<PathBuf>,
    project_dir: Option<PathBuf>,
    project_inline: Vec<RawTeam>,
}

impl Loader {
    /// Build a loader with the given user and project directories.
    /// Pass `None` for the user directory to skip user-tier loading
    /// (used in tests; production loaders use `directories` crate).
    pub fn new(user_dir: Option<PathBuf>, project_dir: Option<PathBuf>) -> Self {
        Self {
            builtins: Self::load_builtins(),
            user_dir,
            project_dir,
            project_inline: Vec::new(),
        }
    }

    /// Attach inline project teams parsed from `[teams.*]` in `maestro.toml`.
    pub fn with_project_inline(mut self, inline: &HashMap<String, TeamConfig>) -> Self {
        for (name, config) in inline {
            self.project_inline.push(RawTeam {
                name: name.clone(),
                config: config.clone(),
                source_tier: SourceTier::Project,
                source_path: None,
            });
        }
        self
    }

    fn load_builtins() -> Vec<RawTeam> {
        crate::orchestration::builtins::load_all()
    }

    /// Load all three tiers and apply name-collision (project > user > built-in).
    /// Returns the post-collision raw map; `extends` merge happens separately
    /// in `Loader::resolve`.
    pub fn load_raw(&self) -> Result<HashMap<String, RawTeam>> {
        let mut map = HashMap::new();
        // built-ins first — lowest priority
        for t in &self.builtins {
            map.insert(t.name.clone(), t.clone());
        }
        // user tier
        if let Some(dir) = &self.user_dir {
            for t in load_dir(dir, SourceTier::User)? {
                map.insert(t.name.clone(), t);
            }
        }
        // project tier (highest priority — overwrites)
        if let Some(dir) = &self.project_dir {
            for t in load_dir(dir, SourceTier::Project)? {
                map.insert(t.name.clone(), t);
            }
        }
        for t in &self.project_inline {
            map.insert(t.name.clone(), t.clone());
        }
        Ok(map)
    }

    pub fn resolve(&self) -> Result<HashMap<String, ResolvedTeam>> {
        let raw = self.load_raw()?;
        let mut resolved: HashMap<String, ResolvedTeam> = HashMap::new();

        for name in raw.keys() {
            // Walk the extends chain, detecting cycles via visited set.
            let mut visited: Vec<String> = vec![name.clone()];
            let mut chain: Vec<&RawTeam> = Vec::new();
            let mut cur_name = name.clone();
            loop {
                let cur = raw
                    .get(&cur_name)
                    .ok_or_else(|| anyhow!("team {name:?} extends missing parent {cur_name:?}"))?;
                chain.push(cur);
                if cur.config.extends.is_empty() {
                    break;
                }
                if visited.contains(&cur.config.extends) {
                    let mut path = visited.clone();
                    path.push(cur.config.extends.clone());
                    return Err(anyhow!("extends cycle detected: {}", path.join(" → ")));
                }
                visited.push(cur.config.extends.clone());
                cur_name = cur.config.extends.clone();
            }

            // Merge from root toward leaf.
            let mut primitive: Option<Primitive> = None;
            let mut min_agents: Vec<String> = Vec::new();
            let mut bindings_str: HashMap<String, String> = HashMap::new();
            let mut overrides: HashMap<String, RoleOverride> = HashMap::new();
            for cur in chain.iter().rev() {
                if let Some(p) = cur.config.primitive {
                    primitive = Some(p);
                }
                if let Some(m) = &cur.config.min_agents {
                    min_agents = m.clone();
                }
                for (k, v) in &cur.config.bindings {
                    if let Some(s) = v.as_str() {
                        bindings_str.insert(k.clone(), s.to_string());
                    }
                }
                for (k, v) in &cur.config.role_overrides {
                    overrides.insert(k.clone(), v.clone());
                }
            }

            let primitive = primitive.ok_or_else(|| {
                anyhow!("team {name:?}: primitive not set anywhere in extends chain")
            })?;

            let mut bindings: HashMap<TeamRole, RoleBinding> = HashMap::new();
            for (k, agent) in &bindings_str {
                let role = match k.as_str() {
                    "implementer" => TeamRole::Implementer,
                    "reviewer" => TeamRole::Reviewer,
                    "docs" => TeamRole::Docs,
                    "devops" => TeamRole::Devops,
                    "orchestrator" => TeamRole::Orchestrator,
                    "triager" => TeamRole::Triager,
                    "researcher" => TeamRole::Researcher,
                    other => return Err(anyhow!("team {name:?}: unknown role binding {other:?}")),
                };
                let ovr = overrides.get(k);
                bindings.insert(
                    role,
                    RoleBinding {
                        agent: ovr
                            .and_then(|o| o.agent.clone())
                            .unwrap_or_else(|| agent.clone()),
                        mode: ovr.and_then(|o| o.mode.clone()),
                        model_override: ovr.and_then(|o| o.model_override.clone()),
                        prompt_addendum: ovr.and_then(|o| o.prompt_addendum.clone()),
                        fallback_agent: ovr.and_then(|o| o.fallback_agent.clone()),
                    },
                );
            }

            resolved.insert(
                name.clone(),
                ResolvedTeam {
                    name: name.clone(),
                    primitive,
                    min_agents,
                    bindings,
                    source_tier: raw[name].source_tier,
                },
            );
        }

        Ok(resolved)
    }

    /// Resolve the user-tier teams directory using the `directories` crate.
    /// Returns `None` if the platform's config dir cannot be determined.
    pub fn user_tier_default() -> Option<PathBuf> {
        directories::ProjectDirs::from("io", "maestro", "maestro")
            .map(|p| p.config_dir().join("teams"))
    }

    /// Resolve the project-tier teams directory: `<repo_root>/.maestro/teams`.
    pub fn project_tier_default(repo_root: &Path) -> PathBuf {
        repo_root.join(".maestro/teams")
    }
}

fn load_dir(dir: &Path, tier: SourceTier) -> Result<Vec<RawTeam>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("reading team dir {dir:?}"))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("invalid team filename: {path:?}"))?
            .to_string();
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("reading team file {path:?}"))?;
        let config: TeamConfig =
            toml::from_str(&content).with_context(|| format!("parsing team file {path:?}"))?;
        out.push(RawTeam {
            name,
            config,
            source_tier: tier,
            source_path: Some(path),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn empty_dirs_load_cleanly() {
        let loader = Loader::new(None, None);
        let raw = loader.load_raw().unwrap();
        assert_eq!(raw.len(), 5);
    }

    #[test]
    fn project_overwrites_user() {
        let user = tempdir().unwrap();
        let project = tempdir().unwrap();

        fs::write(
            user.path().join("foo.toml"),
            r#"extends = ""
primitive = "single-pass"
implementer = "claude""#,
        )
        .unwrap();
        fs::write(
            project.path().join("foo.toml"),
            r#"extends = ""
primitive = "single-pass"
implementer = "ollama""#,
        )
        .unwrap();

        let loader = Loader::new(
            Some(user.path().to_path_buf()),
            Some(project.path().to_path_buf()),
        );
        let raw = loader.load_raw().unwrap();
        let foo = raw.get("foo").unwrap();
        assert_eq!(foo.source_tier, SourceTier::Project);
        assert_eq!(
            foo.config.bindings.get("implementer").unwrap().as_str(),
            Some("ollama")
        );
    }

    #[test]
    fn extends_chain_merges_role_overrides() {
        let user = tempdir().unwrap();
        fs::write(
            user.path().join("base.toml"),
            r#"extends = ""
primitive = "pipeline"
implementer = "claude"
reviewer = "claude"
docs = "claude""#,
        )
        .unwrap();
        fs::write(
            user.path().join("child.toml"),
            r#"extends = "base"
reviewer = "opencode""#,
        )
        .unwrap();

        let loader = Loader::new(Some(user.path().to_path_buf()), None);
        let resolved = loader.resolve().unwrap();
        let child = resolved.get("child").unwrap();
        assert_eq!(
            child.bindings.get(&TeamRole::Reviewer).unwrap().agent,
            "opencode"
        );
        assert_eq!(
            child.bindings.get(&TeamRole::Implementer).unwrap().agent,
            "claude"
        );
    }

    #[test]
    fn cycle_detected() {
        let user = tempdir().unwrap();
        fs::write(
            user.path().join("a.toml"),
            r#"extends = "b"
implementer = "claude""#,
        )
        .unwrap();
        fs::write(
            user.path().join("b.toml"),
            r#"extends = "a"
implementer = "claude""#,
        )
        .unwrap();

        let loader = Loader::new(Some(user.path().to_path_buf()), None);
        let err = loader.resolve().unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("cycle"), "expected cycle error, got: {msg}");
    }

    #[test]
    fn cycle_fixture_detected() {
        let fixtures = PathBuf::from("tests/fixtures/teams");
        let loader = Loader::new(Some(fixtures), None);
        let err = loader.resolve().unwrap_err();
        assert!(format!("{err:#}").contains("cycle"));
    }

    #[test]
    fn builtins_resolve_clean() {
        let loader = Loader::new(None, None);
        let resolved = loader.resolve().unwrap();
        assert_eq!(resolved.len(), 5);
        for name in [
            "default-coder",
            "default-researcher",
            "default-triager",
            "default-reviewer",
            "default-docs",
        ] {
            assert!(resolved.contains_key(name), "missing {name}");
        }
    }

    #[test]
    fn user_path_resolves_per_platform() {
        let path = Loader::user_tier_default().unwrap();
        let expected = directories::ProjectDirs::from("io", "maestro", "maestro")
            .unwrap()
            .config_dir()
            .join("teams");
        assert_eq!(path, expected);
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn project_path_resolves() {
        let root = PathBuf::from("/tmp/repo");
        let p = Loader::project_tier_default(&root);
        assert!(p.ends_with(".maestro/teams"));
    }
}
