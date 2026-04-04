use std::process::Command;

use crate::config::Config;
use crate::provider::types::ProviderKind;

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
    let mut checks = vec![
        check_gh_installed(),
        check_gh_authenticated(),
        check_git_installed(),
        check_git_user_config(),
        check_git_remote(),
        check_config_exists(),
    ];

    if let Some(cfg) = config
        && cfg.provider.kind == ProviderKind::AzureDevops
    {
        checks.push(check_az_cli());
    }

    checks.push(check_claude_cli());

    // Only check repo access if gh is authenticated
    if checks.iter().any(|c| c.name == "gh auth" && c.passed) {
        checks.push(check_gh_repo_accessible());
    }

    DoctorReport { checks }
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
    match Command::new("gh").args(["auth", "status"]).output() {
        Ok(out) if out.status.success() => {
            CheckResult::pass("gh auth", "authenticated", CheckSeverity::Required)
        }
        _ => CheckResult::fail(
            "gh auth",
            "not authenticated — run `gh auth login`",
            CheckSeverity::Required,
        ),
    }
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

fn check_claude_cli() -> CheckResult {
    match Command::new("claude").arg("--version").output() {
        Ok(out) if out.status.success() => {
            let version = sanitize(String::from_utf8_lossy(&out.stdout).trim());
            CheckResult::pass("claude cli", version, CheckSeverity::Optional)
        }
        _ => CheckResult::fail(
            "claude cli",
            "not installed — sessions won't launch",
            CheckSeverity::Optional,
        ),
    }
}

fn check_az_cli() -> CheckResult {
    match Command::new("az").arg("--version").output() {
        Ok(out) if out.status.success() => {
            CheckResult::pass("az cli", "installed", CheckSeverity::Optional)
        }
        _ => CheckResult::fail(
            "az cli",
            "not installed — required for Azure DevOps provider",
            CheckSeverity::Optional,
        ),
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
}
