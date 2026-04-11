use anyhow::{Result, bail};
use regex::Regex;
use std::sync::LazyLock;

/// Find the largest byte offset <= max_bytes that is a valid char boundary.
pub fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> usize {
    if s.len() <= max_bytes {
        return s.len();
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    end
}

/// Truncate a string at a char boundary and append "..." if it was truncated.
pub fn truncate_with_ellipsis(s: &str, max_bytes: usize) -> String {
    let end = truncate_at_char_boundary(s, max_bytes);
    if end < s.len() {
        format!("{}...", &s[..end])
    } else {
        s.to_string()
    }
}

// --- Input validation ---

static ENV_VAR_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Z][A-Z0-9_]*$").unwrap());

static BRANCH_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9/_.\-]+$").unwrap());

static SLUG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_\-]+$").unwrap());

/// Validate that an environment variable name uses the MAESTRO_ prefix
/// and matches ^[A-Z][A-Z0-9_]*$.
pub fn validate_env_var_name(name: &str) -> Result<()> {
    if !ENV_VAR_NAME_RE.is_match(name) {
        bail!(
            "Invalid env var name: {:?} (must match ^[A-Z][A-Z0-9_]*$)",
            name
        );
    }
    if !name.starts_with("MAESTRO_") {
        bail!("Plugin env var {:?} must use MAESTRO_ prefix", name);
    }
    Ok(())
}

/// Validate a branch name against safe characters only.
pub fn validate_branch_name(branch: &str) -> Result<()> {
    if branch.is_empty() {
        bail!("Branch name must not be empty");
    }
    if branch.len() > 255 {
        bail!("Branch name too long: {} bytes (max 255)", branch.len());
    }
    if !BRANCH_NAME_RE.is_match(branch) {
        bail!(
            "Invalid branch name: {:?} (must match ^[a-zA-Z0-9/_.-]+$)",
            branch
        );
    }
    if branch.contains("..") {
        bail!("Branch name must not contain '..'");
    }
    if branch.starts_with('-') {
        bail!("Branch name must not start with '-'");
    }
    Ok(())
}

/// Validate a PR number string as numeric only.
#[allow(dead_code)] // Reason: PR number validation — to be used in review dispatcher for string PR inputs
pub fn validate_pr_number_str(s: &str) -> Result<u64> {
    s.parse::<u64>()
        .map_err(|_| anyhow::anyhow!("PR number must be numeric, got {:?}", s))
}

/// Validate a slug for safe filesystem use (no path separators, no dots).
pub fn validate_slug(slug: &str) -> Result<()> {
    if slug.is_empty() {
        bail!("Slug must not be empty");
    }
    if slug.len() > 128 {
        bail!("Slug too long: {} bytes (max 128)", slug.len());
    }
    if !SLUG_RE.is_match(slug) {
        bail!("Invalid slug: {:?} (must match ^[a-zA-Z0-9_-]+$)", slug);
    }
    Ok(())
}

/// Validate user-provided strings before passing to `gh` CLI.
/// Prevents argument injection (values starting with `-`).
pub fn validate_gh_arg(value: &str, field_name: &str) -> Result<()> {
    if value.starts_with('-') {
        bail!("{} must not start with '-' (got {:?})", field_name, value);
    }
    if value.contains('\0') {
        bail!("{} must not contain null bytes", field_name);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Env var validation ---

    #[test]
    fn env_var_valid_maestro_prefix() {
        assert!(validate_env_var_name("MAESTRO_SESSION_ID").is_ok());
        assert!(validate_env_var_name("MAESTRO_HOOK").is_ok());
        assert!(validate_env_var_name("MAESTRO_X").is_ok());
    }

    #[test]
    fn env_var_rejects_path() {
        assert!(validate_env_var_name("PATH").is_err());
    }

    #[test]
    fn env_var_rejects_ld_preload() {
        assert!(validate_env_var_name("LD_PRELOAD").is_err());
    }

    #[test]
    fn env_var_rejects_lowercase() {
        assert!(validate_env_var_name("lowercase").is_err());
    }

    #[test]
    fn env_var_rejects_empty() {
        assert!(validate_env_var_name("").is_err());
    }

    #[test]
    fn env_var_rejects_non_maestro_uppercase() {
        assert!(validate_env_var_name("HOME").is_err());
        assert!(validate_env_var_name("DYLD_INSERT_LIBRARIES").is_err());
    }

    // --- Branch name validation ---

    #[test]
    fn branch_valid_names() {
        assert!(validate_branch_name("main").is_ok());
        assert!(validate_branch_name("feat/my-feature").is_ok());
        assert!(validate_branch_name("maestro/issue-42").is_ok());
        assert!(validate_branch_name("release/1.0.0").is_ok());
    }

    #[test]
    fn branch_rejects_spaces() {
        assert!(validate_branch_name("feat branch").is_err());
    }

    #[test]
    fn branch_rejects_semicolons() {
        assert!(validate_branch_name("main;rm -rf /").is_err());
    }

    #[test]
    fn branch_rejects_double_dots() {
        assert!(validate_branch_name("foo/../bar").is_err());
    }

    #[test]
    fn branch_rejects_empty() {
        assert!(validate_branch_name("").is_err());
    }

    #[test]
    fn branch_rejects_dash_prefix_args() {
        assert!(validate_branch_name("--exec=evil").is_err());
    }

    #[test]
    fn branch_rejects_leading_dash() {
        assert!(validate_branch_name("-foo").is_err());
    }

    // --- PR number validation ---

    #[test]
    fn pr_number_valid() {
        assert_eq!(validate_pr_number_str("123").unwrap(), 123);
        assert_eq!(validate_pr_number_str("1").unwrap(), 1);
    }

    #[test]
    fn pr_number_rejects_non_numeric() {
        assert!(validate_pr_number_str("123abc").is_err());
        assert!(validate_pr_number_str("-1").is_err());
        assert!(validate_pr_number_str("").is_err());
    }

    // --- Slug validation ---

    #[test]
    fn slug_valid() {
        assert!(validate_slug("my-feature").is_ok());
        assert!(validate_slug("issue_42").is_ok());
        assert!(validate_slug("abc123").is_ok());
    }

    #[test]
    fn slug_rejects_path_traversal() {
        assert!(validate_slug("../../../etc").is_err());
    }

    #[test]
    fn slug_rejects_slashes() {
        assert!(validate_slug("foo/bar").is_err());
    }

    #[test]
    fn slug_rejects_empty() {
        assert!(validate_slug("").is_err());
    }

    #[test]
    fn slug_rejects_dots() {
        assert!(validate_slug("foo.bar").is_err());
    }

    #[test]
    fn slug_rejects_null_bytes() {
        assert!(validate_slug("slug\0evil").is_err());
    }

    // --- gh arg validation ---

    #[test]
    fn gh_arg_valid() {
        assert!(validate_gh_arg("owner/repo", "repo").is_ok());
        assert!(validate_gh_arg("123", "number").is_ok());
    }

    #[test]
    fn gh_arg_rejects_dash_prefix() {
        assert!(validate_gh_arg("-evil", "arg").is_err());
    }

    #[test]
    fn gh_arg_rejects_null_bytes() {
        assert!(validate_gh_arg("val\0ue", "arg").is_err());
    }
}
