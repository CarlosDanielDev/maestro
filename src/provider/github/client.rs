//! Public GitHub client facade.
//!
//! The GitHub transport and concrete `gh` CLI implementation live in
//! `transport.rs`; this module keeps the long-standing import path stable for
//! callers that depend on `provider::github::client::*`.

#[allow(unused_imports)]
pub(crate) use super::transport::parse_pr_number_from_create_output;
#[allow(unused_imports)]
pub use super::transport::{
    CreateOutcome, GhCliClient, GitHubClient, is_auth_error, is_gh_auth_error, parse_issues_json,
    parse_milestones_json, parse_prs_json, redact_secrets,
};

#[cfg(test)]
pub mod mock {
    pub use crate::provider::github::transport::mock::*;
}
