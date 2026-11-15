use super::MINIMAL_TOML;
use crate::config::Config;

#[test]
fn parses_team_max_parallel() {
    let toml = format!("{MINIMAL_TOML}\n[concurrency]\nteam_max_parallel = 5\n");
    let cfg: Config = toml::from_str(&toml).unwrap();
    assert_eq!(cfg.concurrency.team_max_parallel, Some(5));
}

#[test]
fn parses_inline_teams_section() {
    let toml = format!(
        "{MINIMAL_TOML}\n[teams.cheap]\nextends = \"default-coder\"\nimplementer = \"ollama\"\n"
    );
    let cfg: Config = toml::from_str(&toml).unwrap();
    assert!(cfg.teams.contains_key("cheap"));
}
