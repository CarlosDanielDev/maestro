//! Read-only access to the OAuth token the official Claude Code CLI writes.
//!
//! maestro never writes, refreshes, or logs the credential file or the token.

use std::fmt;
use std::path::Path;

use serde::Deserialize;

/// Bearer token. `Debug` redacts the value. There is no `Display` and no `Serialize`.
#[derive(Clone)]
pub(super) struct AccessToken(String);

impl AccessToken {
    pub(super) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccessToken(<redacted>)")
    }
}

#[derive(Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<OAuthBlock>,
}

#[derive(Deserialize)]
struct OAuthBlock {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
}

/// Reads `<home>/.claude/.credentials.json`. Returns `None` when the file is
/// missing, malformed, or has no token. The serde error is dropped on purpose:
/// serde can echo the offending value, and that value may be the token.
pub(super) fn load_claude_access_token(home: &Path) -> Option<AccessToken> {
    let path = home.join(".claude").join(".credentials.json");
    let raw = std::fs::read_to_string(path).ok()?;
    let file: CredentialsFile = serde_json::from_str(&raw).ok()?;
    file.claude_ai_oauth?
        .access_token
        .filter(|token| !token.is_empty())
        .map(AccessToken)
}
