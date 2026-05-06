use crate::agent_provider::{
    AgentProvider, ClaudeProvider, CodexProvider, MinimaxProvider, OllamaProvider,
    OpenCodeProvider, QwenProvider,
};
use crate::config::{AgentKind, Config, ProviderConfig, ResolvedAgentConfig};
use crate::provider::types::ProviderKind;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Severity of a preflight check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckSeverity {
    Required,
    Optional,
}

/// Result of a single preflight check.
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    pub message: String,
    pub severity: CheckSeverity,
}

impl CheckResult {
    pub fn pass(
        name: impl Into<String>,
        message: impl Into<String>,
        severity: CheckSeverity,
    ) -> Self {
        Self {
            name: name.into(),
            passed: true,
            message: message.into(),
            severity,
        }
    }

    pub fn fail(
        name: impl Into<String>,
        message: impl Into<String>,
        severity: CheckSeverity,
    ) -> Self {
        Self {
            name: name.into(),
            passed: false,
            message: message.into(),
            severity,
        }
    }

    #[allow(dead_code)]
    pub fn symbol(&self) -> &'static str {
        match (self.passed, self.severity) {
            (true, _) => "OK",
            (false, CheckSeverity::Required) => "FAIL",
            (false, CheckSeverity::Optional) => "WARN",
        }
    }
}

/// Summary of all preflight checks.
#[derive(Debug, Clone)]
pub struct DoctorReport {
    pub checks: Vec<CheckResult>,
}

impl DoctorReport {
    pub fn has_failures(&self) -> bool {
        self.checks
            .iter()
            .any(|c| !c.passed && c.severity == CheckSeverity::Required)
    }

    pub fn has_warnings(&self) -> bool {
        self.checks.iter().any(|c| !c.passed)
    }

    pub fn failed_checks(&self) -> Vec<&CheckResult> {
        self.checks.iter().filter(|c| !c.passed).collect()
    }
}

/// Run all preflight checks and return a report.
pub fn run_all_checks(config: Option<&Config>) -> DoctorReport {
    run_all_checks_for_agent(config, None)
}

/// Print a formatted report to stdout.
pub fn print_report(report: &DoctorReport) {
    println!();
    let header = format!("  {:<16} {:<8} {}", "Check", "Status", "Details");
    println!("{header}");
    let separator = format!("  {}", "-".repeat(60));
    println!("{separator}");

    for check in &report.checks {
        let status = match (check.passed, check.severity) {
            (true, _) => "\x1b[32mOK\x1b[0m    ",
            (false, CheckSeverity::Required) => "\x1b[31mFAIL\x1b[0m  ",
            (false, CheckSeverity::Optional) => "\x1b[33mWARN\x1b[0m  ",
        };
        println!("  {:<16} {} {}", check.name, status, check.message);
    }

    if let Some((configured, healthy)) = agent_health_counts(report) {
        println!();
        println!("  Summary: {healthy}/{configured} agents healthy ({configured} configured)");
    }

    print_agent_config_upgrade_hint();

    println!();
    if report.has_failures() {
        println!("  \x1b[31mSome required checks failed. Maestro may not work correctly.\x1b[0m");
    } else if report.has_warnings() {
        println!(
            "  \x1b[33mAll required checks passed, but some optional tools are missing.\x1b[0m"
        );
    } else {
        println!("  \x1b[32mAll checks passed! Maestro is ready to use.\x1b[0m");
    }
    println!();
}

/// Strip control characters from subprocess output for safe terminal display.
fn sanitize(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

fn agent_health_counts(report: &DoctorReport) -> Option<(usize, usize)> {
    let configured = report
        .checks
        .iter()
        .filter(|check| check.name.starts_with("agent "))
        .count();
    if configured == 0 {
        return None;
    }
    let healthy = report
        .checks
        .iter()
        .filter(|check| check.name.starts_with("agent ") && check.passed)
        .count();
    Some((configured, healthy))
}

/// Validate preflight checks and return an error if required checks fail.
pub fn validate_preflight(report: &DoctorReport) -> anyhow::Result<()> {
    let failed: Vec<String> = report
        .checks
        .iter()
        .filter(|c| !c.passed && c.severity == CheckSeverity::Required)
        .map(|c| format!("{}: {}", c.name, c.message))
        .collect();
    if !failed.is_empty() {
        anyhow::bail!("Preflight failed: {}", failed.join("; "));
    }
    Ok(())
}

/// Validate provider-aware setup checks and return an error if required checks fail.
pub fn validate_provider_setup_for_agent(
    config: &Config,
    agent_id: Option<&str>,
) -> anyhow::Result<()> {
    let report = run_all_checks_for_agent(Some(config), agent_id);
    validate_preflight(&report)
}

pub fn run_all_checks_for_agent(config: Option<&Config>, agent_id: Option<&str>) -> DoctorReport {
    let provider_kind = config.map(|cfg| cfg.provider.kind).unwrap_or_default();
    let mut checks = vec![
        check_git_installed(),
        check_git_user_config(),
        check_git_remote(),
    ];

    match provider_kind {
        ProviderKind::Github => {
            checks.push(check_gh_installed());
            checks.push(check_gh_authenticated());
        }
        ProviderKind::AzureDevops => {
            checks.push(check_az_cli(CheckSeverity::Required));
            checks.push(check_az_identity(CheckSeverity::Required));
            if let Some(cfg) = config {
                checks.push(check_azdo_config(&cfg.provider));
            }
            checks.push(check_azdo_remote());
        }
    }

    checks.push(check_config_exists());

    if let Some(cfg) = config {
        checks.push(check_provider_matches_remote(cfg.provider.kind));
    }

    checks.push(check_agent_config_schema());

    if let (Some(cfg), None) = (config, agent_id) {
        checks.extend(check_configured_agent_runtimes(cfg));
    } else {
        checks.push(check_agent_runtime_for_agent(config, agent_id));
    }

    if provider_kind == ProviderKind::Github
        && checks.iter().any(|c| c.name == "gh auth" && c.passed)
    {
        checks.push(check_gh_repo_accessible());
    }

    DoctorReport { checks }
}

/// Pure, testable core of the claude cli check.
pub(crate) fn build_claude_cli_result(available: bool, version: &str) -> CheckResult {
    if available {
        CheckResult::pass("claude cli", version, CheckSeverity::Required)
    } else {
        CheckResult::fail(
            "claude cli",
            "not installed — sessions won't launch",
            CheckSeverity::Required,
        )
    }
}

/// Pure, testable core of the gh auth check.
/// Accepts the outputs of the external process calls as plain values.
pub(crate) fn build_gh_auth_result(
    auth_ok: bool,
    username: Option<&str>,
    scopes: Option<&str>,
) -> CheckResult {
    if !auth_ok {
        return CheckResult::fail(
            "gh auth",
            "not authenticated — run `gh auth login`",
            CheckSeverity::Required,
        );
    }
    let mut parts: Vec<String> = Vec::new();
    match username {
        Some(u) => parts.push(format!("authenticated as @{}", u)),
        None => parts.push("authenticated".to_string()),
    }
    if let Some(s) = scopes
        && !s.is_empty()
    {
        parts.push(format!("scopes: {}", s));
    }
    CheckResult::pass("gh auth", parts.join(", "), CheckSeverity::Required)
}

pub(crate) fn build_az_cli_result(available: bool, severity: CheckSeverity) -> CheckResult {
    if available {
        CheckResult::pass("az cli", "installed", severity)
    } else {
        CheckResult::fail(
            "az cli",
            "not installed — required for Azure DevOps provider",
            severity,
        )
    }
}

pub(crate) fn build_az_identity_result(
    authenticated: bool,
    username: Option<&str>,
    severity: CheckSeverity,
) -> CheckResult {
    if authenticated
        && let Some(username) = username
        && !username.trim().is_empty()
    {
        return CheckResult::pass(
            "az identity",
            format!("logged in as {}", username.trim()),
            severity,
        );
    }

    CheckResult::fail("az identity", "could not fetch identity", severity)
}

pub(crate) fn build_azdo_config_result(provider: &ProviderConfig) -> CheckResult {
    let has_org = provider
        .organization
        .as_deref()
        .map(str::trim)
        .is_some_and(|s| !s.is_empty());
    let has_project = provider
        .az_project
        .as_deref()
        .map(str::trim)
        .is_some_and(|s| !s.is_empty());

    match (has_org, has_project) {
        (true, true) => CheckResult::pass(
            "azdo config",
            "provider.organization and provider.az_project set",
            CheckSeverity::Required,
        ),
        (false, false) => CheckResult::fail(
            "azdo config",
            "provider.organization and provider.az_project are required for Azure DevOps",
            CheckSeverity::Required,
        ),
        (false, true) => CheckResult::fail(
            "azdo config",
            "provider.organization is required for Azure DevOps",
            CheckSeverity::Required,
        ),
        (true, false) => CheckResult::fail(
            "azdo config",
            "provider.az_project is required for Azure DevOps",
            CheckSeverity::Required,
        ),
    }
}

pub(crate) fn build_azdo_remote_result(remote_output: Option<&str>) -> CheckResult {
    let Some(remote_output) = remote_output.map(str::trim).filter(|s| !s.is_empty()) else {
        return CheckResult::fail(
            "azdo remote",
            "no git remote found — Azure DevOps provider requires a dev.azure.com remote",
            CheckSeverity::Required,
        );
    };

    let detected = crate::provider::detect_provider_from_remote(remote_output);
    if detected == ProviderKind::AzureDevops {
        CheckResult::pass(
            "azdo remote",
            "git remote points to Azure DevOps",
            CheckSeverity::Required,
        )
    } else {
        CheckResult::fail(
            "azdo remote",
            format!("git remote looks like {detected:?}, expected AzureDevops"),
            CheckSeverity::Required,
        )
    }
}

// --- Individual check functions ---

fn check_gh_installed() -> CheckResult {
    match Command::new("gh").arg("--version").output() {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout);
            let ver_line = sanitize(version.lines().next().unwrap_or("unknown"));
            CheckResult::pass("gh cli", ver_line, CheckSeverity::Required)
        }
        _ => CheckResult::fail(
            "gh cli",
            "not installed — https://cli.github.com",
            CheckSeverity::Required,
        ),
    }
}

fn check_gh_authenticated() -> CheckResult {
    let auth_output = Command::new("gh").args(["auth", "status"]).output();
    let auth_ok = auth_output
        .as_ref()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let username = if auth_ok {
        Command::new("gh")
            .args(["api", "user", "-q", ".login"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| sanitize(String::from_utf8_lossy(&o.stdout).trim()))
            .filter(|s| !s.is_empty())
    } else {
        None
    };

    let scopes = if auth_ok {
        auth_output
            .ok()
            .map(|o| {
                let stderr = String::from_utf8_lossy(&o.stderr);
                stderr
                    .lines()
                    .find(|l| l.contains("Token scopes:") || l.contains("scopes:"))
                    .map(|l| sanitize(l.trim()))
                    .unwrap_or_default()
            })
            .filter(|s| !s.is_empty())
    } else {
        None
    };

    build_gh_auth_result(auth_ok, username.as_deref(), scopes.as_deref())
}

fn check_git_installed() -> CheckResult {
    match Command::new("git").arg("--version").output() {
        Ok(out) if out.status.success() => {
            let version = sanitize(String::from_utf8_lossy(&out.stdout).trim());
            CheckResult::pass("git", version, CheckSeverity::Required)
        }
        _ => CheckResult::fail("git", "not installed", CheckSeverity::Required),
    }
}

fn check_git_user_config() -> CheckResult {
    let name_ok = Command::new("git")
        .args(["config", "user.name"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let email_ok = Command::new("git")
        .args(["config", "user.email"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    match (name_ok, email_ok) {
        (true, true) => CheckResult::pass(
            "git config",
            "user.name and user.email set",
            CheckSeverity::Required,
        ),
        (false, _) => CheckResult::fail(
            "git config",
            "user.name not set — run `git config --global user.name \"Your Name\"`",
            CheckSeverity::Required,
        ),
        (_, false) => CheckResult::fail(
            "git config",
            "user.email not set — run `git config --global user.email \"you@example.com\"`",
            CheckSeverity::Required,
        ),
    }
}

fn check_git_remote() -> CheckResult {
    match Command::new("git").args(["remote", "-v"]).output() {
        Ok(out) if out.status.success() && !out.stdout.is_empty() => {
            CheckResult::pass("git remote", "remote configured", CheckSeverity::Required)
        }
        _ => CheckResult::fail(
            "git remote",
            "no git remote found — are you in a git repo?",
            CheckSeverity::Required,
        ),
    }
}

fn check_provider_matches_remote(configured: ProviderKind) -> CheckResult {
    match Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
    {
        Ok(out) if out.status.success() => {
            let url = String::from_utf8_lossy(&out.stdout);
            let detected = crate::provider::detect_provider_from_remote(url.trim());
            if detected == configured {
                CheckResult::pass(
                    "provider",
                    format!("configured provider matches origin ({detected:?})"),
                    CheckSeverity::Optional,
                )
            } else {
                CheckResult::fail(
                    "provider",
                    format!(
                        "configured provider is {configured:?}, but origin looks like {detected:?}"
                    ),
                    CheckSeverity::Optional,
                )
            }
        }
        _ => CheckResult::fail(
            "provider",
            "could not detect provider from origin remote",
            CheckSeverity::Optional,
        ),
    }
}

fn check_config_exists() -> CheckResult {
    if std::path::Path::new("maestro.toml").exists() {
        match Config::find_and_load() {
            Ok(_) => CheckResult::pass("maestro.toml", "found and valid", CheckSeverity::Required),
            Err(e) => CheckResult::fail(
                "maestro.toml",
                format!("found but invalid — {}", e),
                CheckSeverity::Required,
            ),
        }
    } else {
        CheckResult::fail(
            "maestro.toml",
            "not found — run `maestro init`",
            CheckSeverity::Required,
        )
    }
}

fn check_agent_config_schema() -> CheckResult {
    let Some(path) = find_config_path(Path::new(".")) else {
        return CheckResult::fail(
            "agent config",
            "no config file found; run `maestro init` first",
            CheckSeverity::Optional,
        );
    };

    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) => {
            return CheckResult::fail(
                "agent config",
                format!("could not read {}: {err}", path.display()),
                CheckSeverity::Optional,
            );
        }
    };

    match crate::config::plan_agent_config_upgrade(&content) {
        Ok(plan) if plan.needs_update => CheckResult::fail(
            "agent config",
            format!(
                "{} schema; add {} missing key(s). Use Settings > Project > Normalize Agent Config or paste the suggested block below.",
                plan.version.label(),
                plan.keys_added.len()
            ),
            CheckSeverity::Optional,
        ),
        Ok(plan) => CheckResult::pass(
            "agent config",
            format!("{} schema", plan.version.label()),
            CheckSeverity::Optional,
        ),
        Err(err) => CheckResult::fail(
            "agent config",
            format!("could not inspect [agents]: {err}"),
            CheckSeverity::Optional,
        ),
    }
}

fn find_config_path(base: &Path) -> Option<PathBuf> {
    ["maestro.toml", ".maestro/config.toml"]
        .into_iter()
        .map(|candidate| base.join(candidate))
        .find(|path| path.exists())
}

fn print_agent_config_upgrade_hint() {
    let Some(path) = find_config_path(Path::new(".")) else {
        return;
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(plan) = crate::config::plan_agent_config_upgrade(&content) else {
        return;
    };
    if !plan.needs_update {
        return;
    }

    println!();
    println!(
        "  Suggested [agents] normalization for {} ({})",
        path.display(),
        plan.version.label()
    );
    println!("  Apply from Settings > Project > Normalize Agent Config, or insert:");
    println!();
    for line in plan.snippet.trim_end().lines() {
        println!("    {line}");
    }
}

fn check_claude_cli() -> CheckResult {
    let health = ClaudeProvider::default().health_check_blocking();
    let version = health.version.unwrap_or(health.message);
    build_claude_cli_result(health.available, &sanitize(&version))
}

fn check_agent_runtime_for_agent(config: Option<&Config>, agent_id: Option<&str>) -> CheckResult {
    let Some(config) = config else {
        return check_claude_cli();
    };

    match config.resolve_agent(agent_id) {
        Ok(resolved) => check_resolved_agent_runtime(resolved, CheckSeverity::Required),
        Err(err) => CheckResult::fail("agent config", err.to_string(), CheckSeverity::Required),
    }
}

fn check_configured_agent_runtimes(config: &Config) -> Vec<CheckResult> {
    if config.agents.entries.is_empty() {
        return vec![check_agent_runtime_for_agent(Some(config), None)];
    }

    config
        .agents
        .entries
        .iter()
        .filter(|(_, agent)| agent.enabled)
        .map(|(id, _)| match config.resolve_agent(Some(id)) {
            Ok(resolved) => {
                let severity = if resolved.id == config.agents.default {
                    CheckSeverity::Required
                } else {
                    CheckSeverity::Optional
                };
                check_resolved_agent_runtime(resolved, severity)
            }
            Err(err) => CheckResult::fail(
                format!("agent {id}"),
                err.to_string(),
                if id == &config.agents.default {
                    CheckSeverity::Required
                } else {
                    CheckSeverity::Optional
                },
            ),
        })
        .collect()
}

fn check_resolved_agent_runtime(
    resolved: ResolvedAgentConfig,
    severity: CheckSeverity,
) -> CheckResult {
    match resolved.config.kind {
        AgentKind::Claude => check_agent_health(
            format!("agent {}", resolved.id),
            ClaudeProvider::new(resolved.config.command.as_deref().unwrap_or("claude"))
                .health_check_blocking(),
            severity,
        ),
        AgentKind::Codex => check_subprocess_agent(
            &resolved.id,
            CodexProvider::with_config(
                resolved.config.command.as_deref().unwrap_or("codex"),
                resolved.config.sandbox.clone(),
                resolved.config.ephemeral,
                resolved.config.profile.clone(),
                resolved.config.config_overrides.clone(),
                resolved.config.extra_args.clone(),
                resolved.config.env.clone(),
                resolved.config.json,
            ),
            severity,
        ),
        AgentKind::Qwen => check_subprocess_agent(
            &resolved.id,
            QwenProvider::with_config(
                resolved.config.command.as_deref().unwrap_or("qwen"),
                resolved.config.extra_args.clone(),
                resolved.config.env.clone(),
            ),
            severity,
        ),
        AgentKind::Opencode => check_subprocess_agent(
            &resolved.id,
            OpenCodeProvider::with_config(
                resolved.config.command.as_deref().unwrap_or("opencode"),
                resolved.config.extra_args.clone(),
                resolved.config.env.clone(),
            ),
            severity,
        ),
        AgentKind::Ollama => check_ollama_agent(resolved, severity),
        AgentKind::Minimax => check_minimax_agent(resolved, severity),
    }
}

fn check_subprocess_agent<P>(id: &str, provider: P, severity: CheckSeverity) -> CheckResult
where
    P: AgentProvider + Send + 'static,
{
    run_agent_health_check(format!("agent {id}"), provider, severity)
}

fn check_agent_health(
    name: impl Into<String>,
    health: crate::agent_provider::AgentHealthCheck,
    severity: CheckSeverity,
) -> CheckResult {
    if health.available {
        let message = if health.message.trim().is_empty() {
            health.version.unwrap_or_else(|| "ready".to_string())
        } else {
            health.message
        };
        CheckResult::pass(name, sanitize(&message), severity)
    } else {
        CheckResult::fail(name, sanitize(&health.message), severity)
    }
}

fn check_ollama_agent(resolved: ResolvedAgentConfig, severity: CheckSeverity) -> CheckResult {
    let model = match resolved
        .config
        .model
        .clone()
        .filter(|model| !model.trim().is_empty())
    {
        Some(model) => model,
        None => {
            return CheckResult::fail(
                format!("agent {}", resolved.id),
                format!("agents.{}.model is required for ollama", resolved.id),
                severity,
            );
        }
    };

    let id = resolved.id.clone();
    let provider = match OllamaProvider::new(
        id.clone(),
        resolved
            .config
            .base_url
            .unwrap_or_else(|| "http://localhost:11434".to_string()),
        model,
        2,
        resolved.config.api_key_env,
    ) {
        Ok(provider) => provider,
        Err(err) => {
            return CheckResult::fail(format!("agent {}", resolved.id), err.to_string(), severity);
        }
    };

    run_agent_health_check(format!("agent {id}"), provider, severity)
}

fn check_minimax_agent(resolved: ResolvedAgentConfig, severity: CheckSeverity) -> CheckResult {
    let model = resolved
        .config
        .model
        .clone()
        .filter(|model| !model.trim().is_empty())
        .unwrap_or_else(|| "MiniMax-M2.7".to_string());

    let id = resolved.id.clone();
    let provider = match MinimaxProvider::new(
        id.clone(),
        resolved
            .config
            .base_url
            .unwrap_or_else(|| "https://api.minimax.io/v1".to_string()),
        model,
        2,
        resolved
            .config
            .api_key_env
            .or_else(|| Some("MINIMAX_API_KEY".to_string())),
    ) {
        Ok(provider) => provider,
        Err(err) => {
            return CheckResult::fail(format!("agent {}", resolved.id), err.to_string(), severity);
        }
    };

    run_agent_health_check(format!("agent {id}"), provider, severity)
}

fn run_agent_health_check<P>(
    name: impl Into<String>,
    provider: P,
    severity: CheckSeverity,
) -> CheckResult
where
    P: AgentProvider + Send + 'static,
{
    let name = name.into();
    let panic_name = name.clone();
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(err) => {
                return CheckResult::fail(name, err.to_string(), severity);
            }
        };

        match runtime.block_on(provider.health_check()) {
            Ok(health) => check_agent_health(name, health, severity),
            Err(err) => CheckResult::fail(name, err.to_string(), severity),
        }
    })
    .join()
    .unwrap_or_else(|_| CheckResult::fail(panic_name, "health check panicked", severity))
}

fn check_az_cli(severity: CheckSeverity) -> CheckResult {
    match Command::new("az").arg("--version").output() {
        Ok(out) if out.status.success() => build_az_cli_result(true, severity),
        _ => build_az_cli_result(false, severity),
    }
}

fn check_az_identity(severity: CheckSeverity) -> CheckResult {
    match Command::new("az")
        .args(["account", "show", "-o", "tsv", "--query", "user.name"])
        .output()
    {
        Ok(out) if out.status.success() => {
            let username = sanitize(String::from_utf8_lossy(&out.stdout).trim());
            build_az_identity_result(true, Some(&username), severity)
        }
        _ => build_az_identity_result(false, None, severity),
    }
}

fn check_azdo_config(provider: &ProviderConfig) -> CheckResult {
    build_azdo_config_result(provider)
}

fn check_azdo_remote() -> CheckResult {
    match Command::new("git").args(["remote", "-v"]).output() {
        Ok(out) if out.status.success() => {
            let remote_output = String::from_utf8_lossy(&out.stdout);
            build_azdo_remote_result(Some(&remote_output))
        }
        _ => build_azdo_remote_result(None),
    }
}

fn check_gh_repo_accessible() -> CheckResult {
    match Command::new("gh")
        .args(["repo", "view", "--json", "name", "-q", ".name"])
        .output()
    {
        Ok(out) if out.status.success() => {
            let name = sanitize(String::from_utf8_lossy(&out.stdout).trim());
            CheckResult::pass(
                "gh repo",
                format!("accessible ({})", name),
                CheckSeverity::Required,
            )
        }
        _ => CheckResult::fail(
            "gh repo",
            "cannot access repo — check permissions",
            CheckSeverity::Required,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // CheckResult constructors

    #[test]
    fn check_result_pass_sets_fields() {
        let r = CheckResult::pass("test", "all good", CheckSeverity::Required);
        assert!(r.passed);
        assert_eq!(r.name, "test");
        assert_eq!(r.message, "all good");
        assert_eq!(r.severity, CheckSeverity::Required);
    }

    #[test]
    fn check_result_fail_sets_fields() {
        let r = CheckResult::fail("test", "broken", CheckSeverity::Optional);
        assert!(!r.passed);
        assert_eq!(r.name, "test");
        assert_eq!(r.message, "broken");
        assert_eq!(r.severity, CheckSeverity::Optional);
    }

    // symbol()

    #[test]
    fn symbol_ok_when_passed() {
        let r = CheckResult::pass("x", "y", CheckSeverity::Required);
        assert_eq!(r.symbol(), "OK");
    }

    #[test]
    fn symbol_ok_when_passed_optional() {
        let r = CheckResult::pass("x", "y", CheckSeverity::Optional);
        assert_eq!(r.symbol(), "OK");
    }

    #[test]
    fn symbol_fail_when_required_fails() {
        let r = CheckResult::fail("x", "y", CheckSeverity::Required);
        assert_eq!(r.symbol(), "FAIL");
    }

    #[test]
    fn symbol_warn_when_optional_fails() {
        let r = CheckResult::fail("x", "y", CheckSeverity::Optional);
        assert_eq!(r.symbol(), "WARN");
    }

    // DoctorReport::has_failures

    #[test]
    fn has_failures_true_when_required_check_fails() {
        let report = DoctorReport {
            checks: vec![
                CheckResult::pass("a", "ok", CheckSeverity::Required),
                CheckResult::fail("b", "bad", CheckSeverity::Required),
            ],
        };
        assert!(report.has_failures());
    }

    #[test]
    fn has_failures_false_when_only_optional_fails() {
        let report = DoctorReport {
            checks: vec![
                CheckResult::pass("a", "ok", CheckSeverity::Required),
                CheckResult::fail("b", "missing", CheckSeverity::Optional),
            ],
        };
        assert!(!report.has_failures());
    }

    #[test]
    fn has_failures_false_when_all_pass() {
        let report = DoctorReport {
            checks: vec![
                CheckResult::pass("a", "ok", CheckSeverity::Required),
                CheckResult::pass("b", "ok", CheckSeverity::Optional),
            ],
        };
        assert!(!report.has_failures());
    }

    // DoctorReport::has_warnings

    #[test]
    fn has_warnings_true_when_any_check_fails() {
        let report = DoctorReport {
            checks: vec![
                CheckResult::pass("a", "ok", CheckSeverity::Required),
                CheckResult::fail("b", "missing", CheckSeverity::Optional),
            ],
        };
        assert!(report.has_warnings());
    }

    #[test]
    fn has_warnings_true_when_required_fails() {
        let report = DoctorReport {
            checks: vec![CheckResult::fail("a", "bad", CheckSeverity::Required)],
        };
        assert!(report.has_warnings());
    }

    #[test]
    fn has_warnings_false_when_all_pass() {
        let report = DoctorReport {
            checks: vec![
                CheckResult::pass("a", "ok", CheckSeverity::Required),
                CheckResult::pass("b", "ok", CheckSeverity::Optional),
            ],
        };
        assert!(!report.has_warnings());
    }

    // DoctorReport::failed_checks

    #[test]
    fn failed_checks_returns_only_failed() {
        let report = DoctorReport {
            checks: vec![
                CheckResult::pass("a", "ok", CheckSeverity::Required),
                CheckResult::fail("b", "bad", CheckSeverity::Required),
                CheckResult::fail("c", "missing", CheckSeverity::Optional),
            ],
        };
        let failed = report.failed_checks();
        assert_eq!(failed.len(), 2);
        assert_eq!(failed[0].name, "b");
        assert_eq!(failed[1].name, "c");
    }

    #[test]
    fn failed_checks_empty_when_all_pass() {
        let report = DoctorReport {
            checks: vec![CheckResult::pass("a", "ok", CheckSeverity::Required)],
        };
        assert!(report.failed_checks().is_empty());
    }

    // --- Tests for build_gh_auth_result() (Issue #34) ---

    #[test]
    fn gh_auth_check_pass_with_username_includes_username_in_message() {
        let result = build_gh_auth_result(true, Some("carlos"), None);
        assert!(result.passed);
        assert!(result.message.contains("carlos"));
    }

    #[test]
    fn gh_auth_check_pass_with_username_prefixes_at_sign() {
        let result = build_gh_auth_result(true, Some("carlos"), None);
        assert!(result.message.contains("@carlos"));
    }

    #[test]
    fn gh_auth_check_pass_without_username_falls_back_to_authenticated() {
        let result = build_gh_auth_result(true, None, None);
        assert!(result.passed);
        assert!(result.message.contains("authenticated"));
    }

    #[test]
    fn gh_auth_check_fail_returns_not_authenticated() {
        let result = build_gh_auth_result(false, None, None);
        assert!(!result.passed);
        assert_eq!(result.severity, CheckSeverity::Required);
        assert!(result.message.contains("gh auth login"));
    }

    #[test]
    fn gh_auth_check_pass_with_scopes_includes_scopes_in_message() {
        let result = build_gh_auth_result(true, Some("carlos"), Some("repo,read:org"));
        assert!(result.passed);
        assert!(result.message.contains("repo,read:org"));
    }

    #[test]
    fn gh_auth_check_pass_without_scopes_does_not_panic() {
        let result = build_gh_auth_result(true, Some("carlos"), None);
        assert!(result.passed);
        assert!(!result.message.is_empty());
    }

    #[test]
    fn report_with_username_check_has_no_failures_when_passed() {
        let report = DoctorReport {
            checks: vec![CheckResult::pass(
                "gh auth",
                "authenticated as @carlos",
                CheckSeverity::Required,
            )],
        };
        assert!(!report.has_failures());
    }

    #[test]
    fn report_with_failed_auth_check_has_failures() {
        let report = DoctorReport {
            checks: vec![CheckResult::fail(
                "gh auth",
                "not authenticated — run `gh auth login`",
                CheckSeverity::Required,
            )],
        };
        assert!(report.has_failures());
    }

    // --- Tests for build_claude_cli_result() and validate_preflight() (Issue #52) ---

    #[test]
    fn build_claude_cli_result_pass_returns_required_severity() {
        let result = build_claude_cli_result(true, "claude/1.2.3");
        assert!(result.passed);
        assert_eq!(result.severity, CheckSeverity::Required);
    }

    #[test]
    fn build_claude_cli_result_fail_returns_required_severity() {
        let result = build_claude_cli_result(false, "");
        assert!(!result.passed);
        assert_eq!(result.severity, CheckSeverity::Required);
    }

    #[test]
    fn build_claude_cli_result_fail_message_contains_not_installed() {
        let result = build_claude_cli_result(false, "");
        assert!(result.message.contains("not installed"));
    }

    #[test]
    fn build_claude_cli_result_pass_includes_version_in_message() {
        let result = build_claude_cli_result(true, "claude/1.2.3");
        assert!(result.message.contains("claude/1.2.3"));
    }

    #[test]
    fn validate_preflight_returns_ok_when_no_failures() {
        let report = DoctorReport {
            checks: vec![
                CheckResult::pass("gh cli", "ok", CheckSeverity::Required),
                CheckResult::pass("claude cli", "ok", CheckSeverity::Required),
            ],
        };
        assert!(validate_preflight(&report).is_ok());
    }

    #[test]
    fn validate_preflight_returns_error_when_required_check_fails() {
        let report = DoctorReport {
            checks: vec![
                CheckResult::pass("gh cli", "ok", CheckSeverity::Required),
                CheckResult::fail("claude cli", "not installed", CheckSeverity::Required),
            ],
        };
        assert!(validate_preflight(&report).is_err());
    }

    #[test]
    fn validate_preflight_error_message_names_the_failing_check() {
        let report = DoctorReport {
            checks: vec![CheckResult::fail(
                "claude cli",
                "not installed — sessions won't launch",
                CheckSeverity::Required,
            )],
        };
        let err = validate_preflight(&report).unwrap_err();
        assert!(err.to_string().contains("claude cli"));
    }

    #[test]
    fn validate_preflight_returns_ok_when_only_optional_fails() {
        let report = DoctorReport {
            checks: vec![
                CheckResult::pass("gh cli", "ok", CheckSeverity::Required),
                CheckResult::fail("az cli", "not installed", CheckSeverity::Optional),
            ],
        };
        assert!(validate_preflight(&report).is_ok());
    }

    #[test]
    fn validate_preflight_returns_ok_on_empty_report() {
        let report = DoctorReport { checks: vec![] };
        assert!(validate_preflight(&report).is_ok());
    }

    #[test]
    fn agent_health_counts_reports_configured_and_healthy_agents() {
        let report = DoctorReport {
            checks: vec![
                CheckResult::pass("git", "ok", CheckSeverity::Required),
                CheckResult::pass("agent claude", "ready", CheckSeverity::Required),
                CheckResult::fail("agent qwen", "not installed", CheckSeverity::Optional),
            ],
        };

        assert_eq!(agent_health_counts(&report), Some((2, 1)));
    }

    #[test]
    fn optional_agent_failure_does_not_fail_report() {
        let report = DoctorReport {
            checks: vec![
                CheckResult::pass("agent claude", "ready", CheckSeverity::Required),
                CheckResult::fail("agent qwen", "not installed", CheckSeverity::Optional),
            ],
        };

        assert!(!report.has_failures());
        assert!(report.has_warnings());
    }

    #[test]
    fn default_agent_failure_fails_report() {
        let report = DoctorReport {
            checks: vec![
                CheckResult::fail("agent claude", "not installed", CheckSeverity::Required),
                CheckResult::pass("agent qwen", "ready", CheckSeverity::Optional),
            ],
        };

        assert!(report.has_failures());
    }

    #[test]
    fn failed_agent_health_preserves_actionable_message_when_version_exists() {
        let result = check_agent_health(
            "agent opencode",
            crate::agent_provider::AgentHealthCheck {
                provider_id: crate::agent_provider::AgentProviderId::new("opencode"),
                available: false,
                version: Some("opencode 1.0.0".to_string()),
                message: "run `opencode /connect` to authenticate with a provider".to_string(),
            },
            CheckSeverity::Optional,
        );

        assert!(!result.passed);
        assert!(result.message.contains("opencode /connect"));
        assert!(!result.message.contains("1.0.0"));
    }

    #[cfg(unix)]
    #[test]
    fn configured_agent_runtime_checks_iterate_enabled_agents_only() {
        let temp = tempfile::tempdir().expect("tempdir");
        let claude = fake_binary(temp.path(), "fake-claude", "claude 1.0.0");
        let qwen = fake_binary(temp.path(), "fake-qwen", "qwen 0.9.0");
        let config_path = temp.path().join("maestro.toml");
        std::fs::write(
            &config_path,
            format!(
                r#"[project]
repo = "owner/repo"
[sessions]
default_model = "sonnet"
permission_mode = "acceptEdits"
[budget]
[provider]
kind = "github"
[notifications]
[agents]
default = "claude"
[agents.claude]
kind = "claude"
command = "{}"
[agents.qwen]
kind = "qwen"
command = "{}"
[agents.opencode]
kind = "opencode"
enabled = false
command = "missing-opencode"
"#,
                claude.display(),
                qwen.display()
            ),
        )
        .expect("write config");
        let config = Config::load(&config_path).expect("load config");

        let checks = check_configured_agent_runtimes(&config);

        assert_eq!(checks.len(), 2);
        assert!(checks.iter().any(|check| check.name == "agent claude"
            && check.passed
            && check.severity == CheckSeverity::Required));
        assert!(checks.iter().any(|check| check.name == "agent qwen"
            && check.passed
            && check.severity == CheckSeverity::Optional));
        assert!(!checks.iter().any(|check| check.name == "agent opencode"));
    }

    #[cfg(unix)]
    fn fake_binary(dir: &std::path::Path, name: &str, version: &str) -> std::path::PathBuf {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).expect("create fake binary");
        writeln!(file, "#!/bin/sh").expect("write shebang");
        writeln!(file, "if [ \"$1\" = \"--version\" ]; then").expect("write branch");
        writeln!(file, "  echo '{version}'").expect("write version");
        writeln!(file, "  exit 0").expect("write exit");
        writeln!(file, "fi").expect("write fi");
        writeln!(file, "exit 0").expect("write default");
        let mut perms = std::fs::metadata(&path).expect("metadata").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod");
        path
    }

    #[test]
    fn validate_preflight_error_lists_all_failing_required_checks() {
        let report = DoctorReport {
            checks: vec![
                CheckResult::fail("gh cli", "not installed", CheckSeverity::Required),
                CheckResult::fail("claude cli", "not installed", CheckSeverity::Required),
            ],
        };
        let err = validate_preflight(&report).unwrap_err().to_string();
        assert!(err.contains("gh cli"), "expected 'gh cli' in: {err}");
        assert!(
            err.contains("claude cli"),
            "expected 'claude cli' in: {err}"
        );
    }

    #[test]
    fn azdo_required_checks_happy_path_passes() {
        let provider = ProviderConfig {
            kind: ProviderKind::AzureDevops,
            organization: Some("https://dev.azure.com/MyOrg".into()),
            az_project: Some("MyProject".into()),
            ..ProviderConfig::default()
        };
        let checks = [
            build_az_cli_result(true, CheckSeverity::Required),
            build_az_identity_result(true, Some("user@example.com"), CheckSeverity::Required),
            build_azdo_config_result(&provider),
            build_azdo_remote_result(Some(
                "origin\thttps://MyOrg@dev.azure.com/MyOrg/MyProject/_git/MyRepo (fetch)",
            )),
        ];

        for name in ["az cli", "az identity", "azdo config", "azdo remote"] {
            let check = checks
                .iter()
                .find(|check| check.name == name)
                .unwrap_or_else(|| panic!("missing required check {name}"));
            assert!(check.passed, "{name} should pass: {}", check.message);
            assert_eq!(check.severity, CheckSeverity::Required);
        }
    }

    #[test]
    fn azdo_config_fails_when_az_project_is_missing() {
        let provider = ProviderConfig {
            kind: ProviderKind::AzureDevops,
            organization: Some("https://dev.azure.com/MyOrg".into()),
            az_project: None,
            ..ProviderConfig::default()
        };

        let result = build_azdo_config_result(&provider);

        assert!(!result.passed);
        assert_eq!(result.severity, CheckSeverity::Required);
        assert!(result.message.contains("provider.az_project"));
    }

    #[test]
    fn azdo_remote_fails_when_remote_is_missing() {
        let result = build_azdo_remote_result(None);

        assert!(!result.passed);
        assert_eq!(result.severity, CheckSeverity::Required);
        assert!(result.message.contains("no git remote"));
    }

    #[test]
    fn azdo_remote_fails_when_remote_is_not_azure_devops() {
        let result = build_azdo_remote_result(Some(
            "origin\tgit@github.com:owner/repo.git (fetch)\norigin\tgit@github.com:owner/repo.git (push)",
        ));

        assert!(!result.passed);
        assert_eq!(result.severity, CheckSeverity::Required);
        assert!(result.message.contains("expected AzureDevops"));
    }

    #[test]
    fn github_required_checks_remain_required_and_azdo_builders_are_not_needed() {
        let gh_cli = CheckResult::pass("gh cli", "gh version 2.0.0", CheckSeverity::Required);
        let gh_auth = build_gh_auth_result(true, Some("carlos"), None);
        let gh_repo = CheckResult::pass("gh repo", "accessible (maestro)", CheckSeverity::Required);

        for check in [&gh_cli, &gh_auth, &gh_repo] {
            assert_eq!(check.severity, CheckSeverity::Required);
            assert!(!check.name.starts_with("az"));
        }
    }
}
