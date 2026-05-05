use super::prd_source::PrdSource;

/// Configuration for the `maestro adapt` command.
#[derive(Debug, Clone, PartialEq)]
pub struct AdaptConfig {
    pub path: std::path::PathBuf,
    pub dry_run: bool,
    pub no_issues: bool,
    pub scan_only: bool,
    pub model: Option<String>,
    pub prd_source: PrdSource,
}

impl Default for AdaptConfig {
    fn default() -> Self {
        Self {
            path: std::path::PathBuf::from("."),
            dry_run: false,
            no_issues: false,
            scan_only: false,
            model: None,
            prd_source: PrdSource::default(),
        }
    }
}

/// Configuration for the `maestro prd` standalone command.
#[derive(Debug, Clone, PartialEq)]
pub struct PrdConfig {
    pub path: std::path::PathBuf,
    pub model: Option<String>,
    pub force: bool,
    pub source: PrdSource,
}
