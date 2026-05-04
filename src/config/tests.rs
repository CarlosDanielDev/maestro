//! Config tests are split by domain so each source file stays under the
//! repository's 400-line guardrail.

use super::sessions::merge_legacy_hollow;
use super::*;

const MINIMAL_TOML: &str = "[project]\nrepo = \"owner/repo\"\n[sessions]\n[budget]\nper_session_usd = 5.0\ntotal_usd = 50.0\nalert_threshold_pct = 80\n[github]\n[notifications]\n";

mod core;
mod notifications_views;
mod roundtrip;
mod sessions_layout;
mod turbo_adapt_paths;
