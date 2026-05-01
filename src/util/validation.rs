//! Input validation for user-supplied strings (branch names, slugs, env vars, CLI args).

use anyhow::{Result, bail};
use regex::Regex;
use std::sync::LazyLock;

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

/// Trim leading/trailing whitespace and collapse internal whitespace runs
/// into single ASCII spaces. Preserves case.
pub fn normalize_title(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Validate a user-supplied title (milestone or issue). Returns the
/// normalized form on success; callers MUST use the returned value.
pub fn validate_title(raw: &str, field: &str) -> Result<String> {
    if raw.contains('\0') {
        bail!("{} must not contain null bytes", field);
    }
    let normalized = normalize_title(raw);
    if normalized.is_empty() {
        bail!("{} must not be empty", field);
    }
    if normalized.len() > 256 {
        bail!("{} too long: {} bytes (max 256)", field, normalized.len());
    }
    if normalized.starts_with('-') {
        bail!("{} must not start with '-' (got {:?})", field, normalized);
    }
    Ok(normalized)
}

/// Canonical "same title" check. Normalizes both sides and compares
/// case-insensitively in ASCII. The single source of truth for
/// duplicate-title detection across the project.
pub fn titles_equivalent(a: &str, b: &str) -> bool {
    normalize_title(a).eq_ignore_ascii_case(&normalize_title(b))
}

/// Cap on PR/issue body + milestone description (one byte below GitHub's
/// 65 536-byte limit so caller-side suffixes still fit).
pub const GH_BODY_MAX_BYTES: usize = 65_535;

/// Reject null bytes (gh argv encoding can't carry them) and payloads
/// over [`GH_BODY_MAX_BYTES`].
pub fn validate_body(raw: &str, field: &str) -> Result<()> {
    if raw.contains('\0') {
        bail!("{} must not contain null bytes", field);
    }
    if raw.len() > GH_BODY_MAX_BYTES {
        bail!(
            "{} too long: {} bytes (max {})",
            field,
            raw.len(),
            GH_BODY_MAX_BYTES
        );
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

    // --- normalize_title ---

    #[test]
    fn normalize_title_trims_and_collapses_whitespace() {
        assert_eq!(normalize_title("  Foo   Bar  "), "Foo Bar");
        assert_eq!(normalize_title("A\t\tB"), "A B");
        assert_eq!(normalize_title("A\n\nB"), "A B");
        assert_eq!(normalize_title("Foo"), "Foo");
    }

    #[test]
    fn normalize_title_preserves_case() {
        assert_eq!(normalize_title("Foo Bar"), "Foo Bar");
        assert_eq!(normalize_title("  M0: Foundation  "), "M0: Foundation");
    }

    #[test]
    fn normalize_title_handles_empty_and_all_whitespace() {
        assert_eq!(normalize_title(""), "");
        assert_eq!(normalize_title("   "), "");
        assert_eq!(normalize_title("\t\n"), "");
    }

    // --- validate_title ---

    #[test]
    fn validate_title_accepts_normal_title() {
        let got = validate_title("  M0: Foundation  ", "milestone title").unwrap();
        assert_eq!(got, "M0: Foundation");
    }

    #[test]
    fn validate_title_rejects_empty() {
        assert!(validate_title("", "t").is_err());
    }

    #[test]
    fn validate_title_rejects_whitespace_only() {
        assert!(validate_title("   ", "t").is_err());
        assert!(validate_title("\t\n", "t").is_err());
    }

    #[test]
    fn validate_title_rejects_leading_dash() {
        assert!(validate_title("-foo", "t").is_err());
        assert!(validate_title("  -foo", "t").is_err());
    }

    #[test]
    fn validate_title_rejects_null_byte() {
        assert!(validate_title("a\0b", "t").is_err());
    }

    #[test]
    fn validate_title_rejects_over_256_bytes() {
        let too_long = "x".repeat(257);
        assert!(validate_title(&too_long, "t").is_err());
    }

    #[test]
    fn validate_title_accepts_exactly_256_bytes() {
        let exact = "x".repeat(256);
        assert!(validate_title(&exact, "t").is_ok());
    }

    // --- titles_equivalent ---

    #[test]
    fn titles_equivalent_is_normalized_and_case_insensitive() {
        assert!(titles_equivalent("  Foo  ", "foo"));
        assert!(titles_equivalent("a  b", "a b"));
        assert!(titles_equivalent("M0: Core", " m0: core "));
        assert!(!titles_equivalent("foo", "bar"));
    }

    #[test]
    fn titles_equivalent_distinguishes_different_titles() {
        assert!(!titles_equivalent("foo", "foo bar"));
        assert!(!titles_equivalent("M0: Core", "M1: Core"));
    }

    // --- validate_body ---

    #[test]
    fn validate_body_accepts_short() {
        assert!(validate_body("A normal PR description.", "PR body").is_ok());
    }

    #[test]
    fn validate_body_rejects_null_byte() {
        let err = validate_body("body\0content", "PR body").unwrap_err();
        assert!(err.to_string().contains("null byte"), "got: {}", err);
    }

    #[test]
    fn validate_body_rejects_over_cap() {
        let too_long = "x".repeat(GH_BODY_MAX_BYTES + 1);
        let err = validate_body(&too_long, "PR body").unwrap_err();
        assert!(err.to_string().contains("PR body"), "got: {}", err);
    }

    #[test]
    fn validate_body_accepts_at_cap() {
        let exact = "x".repeat(GH_BODY_MAX_BYTES);
        assert!(validate_body(&exact, "PR body").is_ok());
    }
}
