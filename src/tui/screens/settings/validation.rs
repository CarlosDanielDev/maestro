use crate::config::Config;

/// Severity of a validation result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationSeverity {
    Valid,
    #[allow(dead_code)] // Reason: validation severity — to be used in settings validation
    Warning,
    Error,
}

/// Validation feedback for a single field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationFeedback {
    pub severity: ValidationSeverity,
    pub message: String,
}

impl ValidationFeedback {
    pub fn valid() -> Self {
        Self {
            severity: ValidationSeverity::Valid,
            message: String::new(),
        }
    }

    #[allow(dead_code)] // Reason: validation feedback constructor — to be used in settings validation
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: ValidationSeverity::Warning,
            message: message.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: ValidationSeverity::Error,
            message: message.into(),
        }
    }

    pub fn is_error(&self) -> bool {
        self.severity == ValidationSeverity::Error
    }
}

/// A validation rule: pure function from config to feedback.
pub type ValidatorFn = fn(&Config) -> ValidationFeedback;

/// Key identifying a field: (tab_index, field_index).
pub type FieldKey = (usize, usize);

// --- Validation rules ---

pub fn validate_repo(config: &Config) -> ValidationFeedback {
    let repo = &config.project.repo;
    if repo.is_empty() {
        return ValidationFeedback::error("repo is required");
    }
    match crate::provider::github::types::parse_owner_repo(repo) {
        Ok(_) => ValidationFeedback::valid(),
        Err(_) => ValidationFeedback::error("must match owner/repo format"),
    }
}

pub fn validate_base_branch(config: &Config) -> ValidationFeedback {
    if config.project.base_branch.is_empty() {
        ValidationFeedback::error("base_branch cannot be empty")
    } else {
        ValidationFeedback::valid()
    }
}

pub fn validate_per_session_usd(config: &Config) -> ValidationFeedback {
    if config.budget.per_session_usd <= 0.0 {
        ValidationFeedback::error("must be > 0")
    } else {
        ValidationFeedback::valid()
    }
}

pub fn validate_total_usd(config: &Config) -> ValidationFeedback {
    if config.budget.total_usd <= 0.0 {
        ValidationFeedback::error("must be > 0")
    } else {
        ValidationFeedback::valid()
    }
}

pub fn validate_alert_threshold_pct(config: &Config) -> ValidationFeedback {
    if !(1..=100).contains(&config.budget.alert_threshold_pct) {
        ValidationFeedback::error("must be 1-100")
    } else {
        ValidationFeedback::valid()
    }
}

pub fn validate_overflow_threshold_pct(config: &Config) -> ValidationFeedback {
    if !(1..=100).contains(&config.sessions.context_overflow.overflow_threshold_pct) {
        ValidationFeedback::error("must be 1-100")
    } else {
        ValidationFeedback::valid()
    }
}

pub fn validate_slack_webhook_url(config: &Config) -> ValidationFeedback {
    match &config.notifications.slack_webhook_url {
        None => ValidationFeedback::valid(),
        Some(url) if url.is_empty() => ValidationFeedback::valid(),
        Some(url) => {
            if url.starts_with("https://") || url.starts_with("http://") {
                ValidationFeedback::valid()
            } else {
                ValidationFeedback::error("must be a valid URL (https://...)")
            }
        }
    }
}

pub fn validate_max_concurrent(config: &Config) -> ValidationFeedback {
    if config.sessions.max_concurrent < 1 {
        ValidationFeedback::error("must be >= 1")
    } else {
        ValidationFeedback::valid()
    }
}

pub fn validate_stall_timeout_secs(config: &Config) -> ValidationFeedback {
    if config.sessions.stall_timeout_secs < 30 {
        ValidationFeedback::error("must be >= 30")
    } else {
        ValidationFeedback::valid()
    }
}

pub fn validate_ci_max_wait_vs_poll(config: &Config) -> ValidationFeedback {
    let g = &config.gates;
    if g.ci_max_wait_secs <= g.ci_poll_interval_secs {
        ValidationFeedback::error("must be > ci_poll_interval_secs")
    } else {
        ValidationFeedback::valid()
    }
}

/// Build the validator map: (tab_index, field_index) -> validator function.
pub fn build_validator_map() -> std::collections::HashMap<FieldKey, ValidatorFn> {
    let mut m = std::collections::HashMap::new();
    // Project tab (0)
    m.insert((0, 0), validate_repo as ValidatorFn);
    m.insert((0, 1), validate_base_branch as ValidatorFn);
    // Sessions tab (1)
    m.insert((1, 0), validate_max_concurrent as ValidatorFn);
    m.insert((1, 1), validate_stall_timeout_secs as ValidatorFn);
    m.insert((1, 7), validate_overflow_threshold_pct as ValidatorFn);
    // Budget tab (2)
    m.insert((2, 0), validate_per_session_usd as ValidatorFn);
    m.insert((2, 1), validate_total_usd as ValidatorFn);
    m.insert((2, 2), validate_alert_threshold_pct as ValidatorFn);
    // Notifications tab (4)
    m.insert((4, 2), validate_slack_webhook_url as ValidatorFn);
    // Gates tab (5)
    m.insert((5, 3), validate_ci_max_wait_vs_poll as ValidatorFn);
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn test_config() -> Config {
        let toml_str = r#"
[project]
repo = "owner/repo"
[sessions]
[budget]
per_session_usd = 5.0
total_usd = 50.0
alert_threshold_pct = 80
[github]
[notifications]
"#;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        use std::io::Write;
        write!(f, "{}", toml_str).unwrap();
        Config::load(f.path()).unwrap()
    }

    // --- validate_repo ---

    #[test]
    fn repo_valid() {
        let c = test_config();
        assert_eq!(validate_repo(&c).severity, ValidationSeverity::Valid);
    }

    #[test]
    fn repo_empty_is_error() {
        let mut c = test_config();
        c.project.repo = String::new();
        assert!(validate_repo(&c).is_error());
    }

    #[test]
    fn repo_missing_slash() {
        let mut c = test_config();
        c.project.repo = "noslash".into();
        assert!(validate_repo(&c).is_error());
    }

    #[test]
    fn repo_slash_but_empty_owner() {
        let mut c = test_config();
        c.project.repo = "/repo".into();
        assert!(validate_repo(&c).is_error());
    }

    #[test]
    fn repo_slash_but_empty_name() {
        let mut c = test_config();
        c.project.repo = "owner/".into();
        assert!(validate_repo(&c).is_error());
    }

    // --- validate_base_branch ---

    #[test]
    fn base_branch_valid() {
        let c = test_config();
        assert_eq!(validate_base_branch(&c).severity, ValidationSeverity::Valid);
    }

    #[test]
    fn base_branch_empty_is_error() {
        let mut c = test_config();
        c.project.base_branch = String::new();
        assert!(validate_base_branch(&c).is_error());
    }

    // --- validate_per_session_usd ---

    #[test]
    fn per_session_usd_valid() {
        let c = test_config();
        assert_eq!(
            validate_per_session_usd(&c).severity,
            ValidationSeverity::Valid
        );
    }

    #[test]
    fn per_session_usd_zero_is_error() {
        let mut c = test_config();
        c.budget.per_session_usd = 0.0;
        assert!(validate_per_session_usd(&c).is_error());
    }

    #[test]
    fn per_session_usd_negative_is_error() {
        let mut c = test_config();
        c.budget.per_session_usd = -1.0;
        assert!(validate_per_session_usd(&c).is_error());
    }

    // --- validate_total_usd ---

    #[test]
    fn total_usd_valid() {
        let c = test_config();
        assert_eq!(validate_total_usd(&c).severity, ValidationSeverity::Valid);
    }

    #[test]
    fn total_usd_zero_is_error() {
        let mut c = test_config();
        c.budget.total_usd = 0.0;
        assert!(validate_total_usd(&c).is_error());
    }

    // --- validate_alert_threshold_pct ---

    #[test]
    fn alert_threshold_valid() {
        let c = test_config();
        assert_eq!(
            validate_alert_threshold_pct(&c).severity,
            ValidationSeverity::Valid
        );
    }

    #[test]
    fn alert_threshold_zero_is_error() {
        let mut c = test_config();
        c.budget.alert_threshold_pct = 0;
        assert!(validate_alert_threshold_pct(&c).is_error());
    }

    #[test]
    fn alert_threshold_101_is_error() {
        let mut c = test_config();
        c.budget.alert_threshold_pct = 101;
        assert!(validate_alert_threshold_pct(&c).is_error());
    }

    #[test]
    fn alert_threshold_boundary_1_valid() {
        let mut c = test_config();
        c.budget.alert_threshold_pct = 1;
        assert_eq!(
            validate_alert_threshold_pct(&c).severity,
            ValidationSeverity::Valid
        );
    }

    #[test]
    fn alert_threshold_boundary_100_valid() {
        let mut c = test_config();
        c.budget.alert_threshold_pct = 100;
        assert_eq!(
            validate_alert_threshold_pct(&c).severity,
            ValidationSeverity::Valid
        );
    }

    // --- validate_overflow_threshold_pct ---

    #[test]
    fn overflow_threshold_valid() {
        let c = test_config();
        assert_eq!(
            validate_overflow_threshold_pct(&c).severity,
            ValidationSeverity::Valid
        );
    }

    #[test]
    fn overflow_threshold_zero_is_error() {
        let mut c = test_config();
        c.sessions.context_overflow.overflow_threshold_pct = 0;
        assert!(validate_overflow_threshold_pct(&c).is_error());
    }

    // --- validate_slack_webhook_url ---

    #[test]
    fn slack_url_none_is_valid() {
        let mut c = test_config();
        c.notifications.slack_webhook_url = None;
        assert_eq!(
            validate_slack_webhook_url(&c).severity,
            ValidationSeverity::Valid
        );
    }

    #[test]
    fn slack_url_empty_is_valid() {
        let mut c = test_config();
        c.notifications.slack_webhook_url = Some(String::new());
        assert_eq!(
            validate_slack_webhook_url(&c).severity,
            ValidationSeverity::Valid
        );
    }

    #[test]
    fn slack_url_valid_https() {
        let mut c = test_config();
        c.notifications.slack_webhook_url =
            Some("https://hooks.slack.com/services/T00/B00/xxx".into());
        assert_eq!(
            validate_slack_webhook_url(&c).severity,
            ValidationSeverity::Valid
        );
    }

    #[test]
    fn slack_url_invalid() {
        let mut c = test_config();
        c.notifications.slack_webhook_url = Some("not-a-url".into());
        assert!(validate_slack_webhook_url(&c).is_error());
    }

    // --- validate_max_concurrent ---

    #[test]
    fn max_concurrent_valid() {
        let c = test_config();
        assert_eq!(
            validate_max_concurrent(&c).severity,
            ValidationSeverity::Valid
        );
    }

    // --- validate_stall_timeout_secs ---

    #[test]
    fn stall_timeout_valid() {
        let c = test_config();
        assert_eq!(
            validate_stall_timeout_secs(&c).severity,
            ValidationSeverity::Valid
        );
    }

    #[test]
    fn stall_timeout_29_is_error() {
        let mut c = test_config();
        c.sessions.stall_timeout_secs = 29;
        assert!(validate_stall_timeout_secs(&c).is_error());
    }

    #[test]
    fn stall_timeout_30_is_valid() {
        let mut c = test_config();
        c.sessions.stall_timeout_secs = 30;
        assert_eq!(
            validate_stall_timeout_secs(&c).severity,
            ValidationSeverity::Valid
        );
    }

    // --- validate_ci_max_wait_vs_poll ---

    #[test]
    fn ci_max_wait_greater_than_poll_is_valid() {
        let mut c = test_config();
        c.gates.ci_poll_interval_secs = 30;
        c.gates.ci_max_wait_secs = 60;
        assert_eq!(
            validate_ci_max_wait_vs_poll(&c).severity,
            ValidationSeverity::Valid
        );
    }

    #[test]
    fn ci_max_wait_equal_to_poll_is_error() {
        let mut c = test_config();
        c.gates.ci_poll_interval_secs = 60;
        c.gates.ci_max_wait_secs = 60;
        assert!(validate_ci_max_wait_vs_poll(&c).is_error());
    }

    #[test]
    fn ci_max_wait_less_than_poll_is_error() {
        let mut c = test_config();
        c.gates.ci_poll_interval_secs = 60;
        c.gates.ci_max_wait_secs = 30;
        assert!(validate_ci_max_wait_vs_poll(&c).is_error());
    }

    // --- build_validator_map ---

    #[test]
    fn validator_map_has_all_entries() {
        let map = build_validator_map();
        assert_eq!(map.len(), 10);
        assert!(map.contains_key(&(0, 0))); // repo
        assert!(map.contains_key(&(0, 1))); // base_branch
        assert!(map.contains_key(&(1, 0))); // max_concurrent
        assert!(map.contains_key(&(1, 1))); // stall_timeout_secs
        assert!(map.contains_key(&(1, 7))); // overflow_threshold_pct
        assert!(map.contains_key(&(2, 0))); // per_session_usd
        assert!(map.contains_key(&(2, 1))); // total_usd
        assert!(map.contains_key(&(2, 2))); // alert_threshold_pct
        assert!(map.contains_key(&(4, 2))); // slack_webhook_url
        assert!(map.contains_key(&(5, 3))); // ci_max_wait_secs
    }

    #[test]
    fn all_validators_pass_for_valid_config() {
        let map = build_validator_map();
        let config = test_config();
        for (key, validator) in &map {
            let result = validator(&config);
            assert_eq!(
                result.severity,
                ValidationSeverity::Valid,
                "Validator at {:?} should pass for valid config",
                key
            );
        }
    }
}
