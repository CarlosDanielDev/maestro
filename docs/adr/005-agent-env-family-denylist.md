# ADR 005 — Agent Env Gate: Family Prefixes and Suffixes, Not an Allowlist

- **Status:** Accepted
- **Date:** 2026-11-14
- **Tracking issue:** [#1135](https://github.com/CarlosDanielDev/maestro/issues/1135)
- **Extends:** [ADR 004](004-agent-env-denylist-extension.md) (its "Known limit" section)
- **Implementation:** `src/util/validation.rs` (`AGENT_ENV_DENYLIST_EXACT`, `AGENT_ENV_DENYLIST_PREFIXES`, `AGENT_ENV_DENYLIST_SUFFIXES`)

---

## Problem

The `[agents.<id>].env` gate denies keys by exact name. Every review pass
found more siblings the last list missed: #1029's AC list, the #1107
post-merge review, two judge passes on PR #1126, then a third pass
(`OPENSSL_CONF`, `OPENSSL_ENGINES`, `CURL_HOME`, `WGETRC`, `PIP_CONFIG_FILE`,
`CARGO_HTTP_CAINFO`, `PYTHONHTTPSVERIFY`, `PIP_INDEX_URL`,
`PIP_EXTRA_INDEX_URL`, `NPM_CONFIG_REGISTRY`). Each missed key stays
pass-through until its own PR lands. #1135 offered two shapes: invert to a
vendor-prefix allowlist, or keep the denylist and deny whole families.

---

## Decision

**Keep the denylist; deny whole families by prefix and suffix.**

| Table | Added in #1135 | Family it closes |
|-------|----------------|------------------|
| prefix | `NODE_TLS_` | Node TLS verification (`NODE_TLS_REJECT_UNAUTHORIZED`) |
| prefix | `OPENSSL_` | OpenSSL config and engine loading (`OPENSSL_CONF`, `OPENSSL_ENGINES`) |
| prefix | `PIP_` | pip registry, config file and CA (`PIP_INDEX_URL`, `PIP_CONFIG_FILE`, `PIP_CERT`) |
| prefix | `NPM_CONFIG_` | npm registry, prefix and CA (`NPM_CONFIG_REGISTRY`, `NPM_CONFIG_PREFIX`) |
| suffix | `_CA_BUNDLE` | CA bundle swap (`REQUESTS_CA_BUNDLE`, `CURL_CA_BUNDLE`, `AWS_CA_BUNDLE`) |
| suffix | `_CAINFO` | CA bundle swap (`CARGO_HTTP_CAINFO`) |
| exact | `CURL_HOME`, `WGETRC`, `PYTHONHTTPSVERIFY` | config files / TLS switches with no shared affix |

`SSLKEYLOGFILE` stays an exact row (it has no family). The exact rows that
a family now also covers (`PIP_CERT`, `NPM_CONFIG_PREFIX`,
`REQUESTS_CA_BUNDLE`, ...) stay too: the floor test only lets the tables
grow, and the rows document why the family exists.

**Why not the inversion.** An allowlist of vendor prefixes (`ANTHROPIC_`,
`OPENAI_`, `QWEN_`, `MINIMAX_`, `OLLAMA_`, `MAESTRO_`) breaks every
non-prefixed var users set today (`MY_TOOL_API_KEY`, `HTTPS_PROXY`,
`NO_PROXY`, `NODE_ENV`), and every new tool or provider becomes a config
migration. It also does not buy the safety it promises: the allowed prefixes
contain the most dangerous redirect of all (`ANTHROPIC_BASE_URL`,
`OPENAI_BASE_URL` send the agent's traffic and key to another host), so an
allowlist still needs a denylist inside it. Families cut the enumeration
pressure where the review passes actually found siblings (CA trust, TLS,
package registries) at no cost to the vars that pass today.

---

## Migration story

- **Who breaks:** a config that sets a key in a new family in
  `[agents.<id>].env` (any `PIP_*`, `NPM_CONFIG_*`, `OPENSSL_*`,
  `NODE_TLS_*`, `*_CA_BUNDLE`, `*_CAINFO`, or `CURL_HOME` / `WGETRC` /
  `PYTHONHTTPSVERIFY`). Config load fails and the error names the key and the
  agent, same as every denied key since #1029.
- **What to do:** set the var in the environment you start maestro from
  (`PIP_INDEX_URL=… maestro …`). Agent children inherit maestro's own
  environment (`apply_agent_env` only overlays toml keys), so the value still
  reaches the agent — only the repo-local toml override is blocked. Same path
  as ADR 004's `HOME` / `SHELL`.
- **Who does not break:** non-prefixed legit vars (custom API keys, proxy
  settings, `NODE_ENV`, `PYTHONUNBUFFERED`) and the near-miss names
  `NPM_TOKEN` and `PIPENV_*` (they do not start with `NPM_CONFIG_` / `PIP_`).
  A regression test pins these.

---

## Known limit

Families shrink the enumeration but do not end it: a dangerous key with no
shared affix still needs an exact row. If a later census keeps finding
affix-less siblings, revisit the inversion with an explicit allow set and
a denylist for the vendor `*_BASE_URL` keys.

---

## Consequences

- 43 exact keys, 7 prefixes and 2 suffixes denied as of this ADR.
- Both test tables (`src/util/validation.rs`,
  `src/config/tests/agents_env.rs`) derive from all three consts, so a new
  family is one row; floor asserts stop any table from shrinking silently.
- CHANGELOG `### Migration` tells affected users to move the var to the
  parent environment.
