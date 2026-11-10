//! Remote bearer-credential policy for HTTP agents (#1030).
//!
//! A cloned repo's `maestro.toml` is attacker-controlled input: it can
//! point `base_url` at any host and — before this policy — name any env
//! var via `api_key_env`, so `maestro doctor` would happily POST the
//! victim's real secret to an attacker host in cleartext (SEC-1 / SUP-5).
//!
//! Two layers consult the same helpers here:
//! - `AgentConfig::validate` rejects unconfirmed hosts at config load.
//! - the MiniMax/Ollama clients re-check before every `bearer_auth`
//!   (defense in depth for paths that bypass config validation).
//!
//! Bearer credentials only travel to the provider default host, a
//! loopback host, or a host the user confirmed with `allow_remote_auth =
//! true`. The `api_key_env` allowlist is absolute: `allow_remote_auth`
//! never lifts it, because a confirmation key inside the repo-controlled
//! file proves nothing to the victim — a provider-prefixed or
//! `MAESTRO_`-prefixed variable is the only secret a repo may name.

use anyhow::Result;

use super::agents::AgentKind;

pub(crate) const OLLAMA_DEFAULT_BASE_URL: &str = "http://localhost:11434";
pub(crate) const MINIMAX_DEFAULT_BASE_URL: &str = "https://api.minimax.io/v1";

/// Hosts that may receive bearer credentials over plain http.
const LOOPBACK_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "::1"];

pub(crate) fn is_loopback_host(host: &str) -> bool {
    LOOPBACK_HOSTS.contains(&host)
}

/// Operator-side confirmation switch (judge-1105): remote-auth
/// confirmation must live OUTSIDE the repo's control, because a cloned
/// repo ships its own `maestro.toml` and could set `allow_remote_auth =
/// true` itself. The repo-local key is parsed but inert; the only
/// confirmation maestro honors is this process-env switch.
pub(crate) fn operator_allows_remote_auth() -> bool {
    operator_allows_remote_auth_with(|name| std::env::var(name).ok())
}

pub(crate) fn operator_allows_remote_auth_with(getenv: impl Fn(&str) -> Option<String>) -> bool {
    getenv("MAESTRO_ALLOW_REMOTE_AUTH").is_some_and(|value| {
        let value = value.trim();
        value == "1" || value.eq_ignore_ascii_case("true")
    })
}

/// Split an http(s) base_url into `(lowercase scheme, lowercase host)`.
/// Strips userinfo, port and IPv6 brackets; `None` for anything that is
/// not a plain http/https URL.
pub(crate) fn split_base_url(base_url: &str) -> Option<(String, String)> {
    let (scheme, rest) = base_url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(authority);
    let host = if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']')?;
        &rest[..end]
    } else {
        authority.split(':').next()?
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        None
    } else {
        Some((scheme, host))
    }
}

fn default_base_url(kind: AgentKind) -> &'static str {
    match kind {
        AgentKind::Ollama => OLLAMA_DEFAULT_BASE_URL,
        AgentKind::Minimax => MINIMAX_DEFAULT_BASE_URL,
        other => unreachable!("default_base_url is only called for HTTP agents, got {other:?}"),
    }
}

/// Request-layer trust check: may this `base_url` receive bearer
/// credentials? Shared by both HTTP clients.
pub(crate) fn credentials_allowed(
    base_url: &str,
    provider_default_base_url: &str,
    allow_remote_auth: bool,
) -> bool {
    if allow_remote_auth {
        return true;
    }
    if base_url.trim_end_matches('/') == provider_default_base_url {
        return true;
    }
    matches!(split_base_url(base_url), Some((_, host)) if is_loopback_host(&host))
}

/// Config-layer checks for one HTTP agent. Called from
/// `AgentConfig::validate` after the base_url non-empty check.
pub(crate) fn validate_remote_auth(
    id: &str,
    kind: AgentKind,
    base_url: &str,
    api_key_env: Option<&str>,
    allow_remote_auth: bool,
) -> Result<()> {
    let Some((scheme, host)) = split_base_url(base_url) else {
        anyhow::bail!("agents.{id}.base_url `{base_url}` is not a valid http(s) URL");
    };

    if scheme != "https" && !is_loopback_host(&host) && !allow_remote_auth {
        anyhow::bail!(
            "agents.{id}.base_url `{base_url}` uses `{scheme}://`, which sends the bearer credential in cleartext; use https:// or a loopback host (localhost, 127.0.0.1, ::1), or export MAESTRO_ALLOW_REMOTE_AUTH=1 to accept the risk (repo-local `allow_remote_auth` is ignored)"
        );
    }

    // The bearer gate only applies when a credential would actually be
    // attached: MiniMax always configures one, Ollama only when
    // api_key_env is set.
    let credentialed = api_key_env
        .map(|name| !name.trim().is_empty())
        .unwrap_or(kind == AgentKind::Minimax);
    let is_default = base_url.trim_end_matches('/') == default_base_url(kind);
    if credentialed && !allow_remote_auth && !is_loopback_host(&host) && !is_default {
        anyhow::bail!(
            "agents.{id}.base_url `{base_url}` points at remote host `{host}`; bearer credentials would be sent there. Export MAESTRO_ALLOW_REMOTE_AUTH=1 in your environment if you trust this host (repo-local `allow_remote_auth` is ignored)"
        );
    }

    // Absolute allowlist: `allow_remote_auth` never lifts it, or a
    // repo-controlled `true` would let a cloned repo exfiltrate any
    // secret env var (SEC-1 / SUP-5).
    if let Some(env_name) = api_key_env.map(str::trim).filter(|name| !name.is_empty()) {
        let prefix = format!("{}_", kind.as_str().to_ascii_uppercase());
        let allowlisted = env_name.starts_with(&prefix) || env_name.starts_with("MAESTRO_");
        if !allowlisted {
            anyhow::bail!(
                "agents.{id}.api_key_env `{env_name}` is outside the allowlist ({prefix}*, MAESTRO_*); a repo-supplied config must not name arbitrary secret env variables. Rename the variable (custom maestro agent secrets use the MAESTRO_ prefix)"
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_base_url_extracts_scheme_and_host() {
        assert_eq!(
            split_base_url("HTTPS://Api.Example.COM:8443/v1"),
            Some(("https".to_string(), "api.example.com".to_string()))
        );
        assert_eq!(
            split_base_url("http://user:pass@127.0.0.1:11434/"),
            Some(("http".to_string(), "127.0.0.1".to_string()))
        );
        assert_eq!(
            split_base_url("http://[::1]:11434"),
            Some(("http".to_string(), "::1".to_string()))
        );
        assert_eq!(split_base_url("ftp://collector.example"), None);
        assert_eq!(split_base_url("collector.example"), None);
        assert_eq!(split_base_url("http://"), None);
    }

    #[test]
    fn loopback_hosts_recognized() {
        for host in ["localhost", "127.0.0.1", "::1"] {
            assert!(is_loopback_host(host), "{host} is loopback");
        }
        for host in [
            "collector.example",
            "0.0.0.0",
            "[::1]",
            "localhost.evil.tld",
        ] {
            assert!(!is_loopback_host(host), "{host} is not loopback");
        }
    }

    #[test]
    fn operator_switch_parses_env_value() {
        let on = |v: &str| operator_allows_remote_auth_with(|_| Some(v.to_string()));
        assert!(on("1"));
        assert!(on("true"));
        assert!(!on("0"));
        assert!(!on("false"));
        assert!(!on("yes"));
        assert!(!on(""));
        assert!(!operator_allows_remote_auth_with(|_| None));
    }

    #[test]
    fn validate_accepts_remote_host_only_with_operator_switch() {
        // Confirmed path lives here rather than in the Config::load tests
        // because tests cannot (and must not) mutate the process env.
        let minimax = |allow: bool| {
            validate_remote_auth(
                "x",
                AgentKind::Minimax,
                "https://relay.example.com/v1",
                Some("MINIMAX_API_KEY"),
                allow,
            )
        };
        assert!(minimax(true).is_ok(), "operator switch must confirm");
        assert!(minimax(false).is_err(), "no switch must refuse");

        let cleartext = validate_remote_auth(
            "x",
            AgentKind::Ollama,
            "http://collector.example/v1",
            None,
            true,
        );
        assert!(
            cleartext.is_ok(),
            "switch must also lift the cleartext gate"
        );
    }
}
