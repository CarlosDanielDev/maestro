# ADR 004 — Agent Env Denylist: Extend, Don't Allowlist; HOME/SHELL Denied

- **Status:** Accepted
- **Date:** 2026-11-11
- **Tracking issue:** [#1107](https://github.com/CarlosDanielDev/maestro/issues/1107)
- **Implementation:** `src/util/validation.rs` (`AGENT_ENV_DENYLIST_EXACT`, `AGENT_ENV_DENYLIST_PREFIXES`)

---

## Problem

A repo-local `maestro.toml` can inject environment variables into every
spawned agent child through `[agents.<id>].env`. The #1029 denylist (PR #1104)
covered its own acceptance list, but the post-merge security review (MEDIUM,
CWE-426 / CWE-454) listed 20 more loader/interpreter keys that still passed
`validate_agent_env_key`, and the judge round added 6 sibling-parity keys
(`SSL_CERT_FILE`, `SSL_CERT_DIR`, `CARGO_HOME`, `RUSTUP_HOME`,
`NPM_CONFIG_PREFIX`, `PERL5DB`). The issue offered two forks: invert to an
allowlist of vendor prefixes, or extend the denylist.

---

## Decision

**Extend the denylist.** An allowlist (`ANTHROPIC_`, `OPENAI_`, …) breaks
every non-prefixed legit var (custom API keys, proxy settings), turns every
new provider or feature into a config migration, and surfaces each new
injection vector as user-visible breakage instead of a one-row addition. The
#1104 exact+prefix tables already gate the right shape; a new dangerous key is
one row away.

**`HOME` and `SHELL` are denied — deliberately diverging from the issue's
"likely: allow HOME" hint.** Agent children inherit maestro's own environment
(`apply_agent_env` overlays toml keys on top of it), so a legitimate home or
shell redirect belongs in the parent environment and keeps working; only the
toml override is blocked. A toml override of `HOME` hijacks
`$HOME/.claude/settings.json` hooks (code execution inside the agent) and
`.netrc` / `.gitconfig` / `.curlrc`; a forged `SHELL` is exec'd by `$SHELL`
callers.

**Revert path:** flipping a key back to "allowed" is one deleted row in
`AGENT_ENV_DENYLIST_EXACT` per key — both test tables derive from that const
(see below), so no test rows exist to update.

---

## Test-table derivation

The denylist was hand-maintained in three places (the const, the unit test
table in `src/util/validation.rs`, the config-load table in
`src/config/tests/agents_env.rs`). Both test tables now derive from the
consts, so a future key addition cannot forget a table, and the revert path
above touches the const only.

---

## Parity additions (judge round)

- `SSL_CERT_FILE` / `SSL_CERT_DIR` — swap the CA trust store for
  OpenSSL-based children; same CA-injection effect as the denied
  `NODE_EXTRA_CA_CERTS` (MITM of agent API traffic).
- `CARGO_HOME` / `RUSTUP_HOME` — redirect the Rust toolchain (`config.toml`
  `rustc` override, fake rustup toolchains); same toolchain-redirection
  effect as the denied `GEM_HOME` / `GEM_PATH`.
- `NPM_CONFIG_PREFIX` — redirects the npm global prefix / global module
  resolution; deny costs nothing, same parent-environment reasoning as
  `HOME` / `SHELL`.
- `PERL5DB` — debugger startup payload; pairs with the denied `PERL5OPT`,
  denying both closes the combo.

---

## Consequences

- 34 exact keys + 3 prefixes denied as of this ADR; new dangerous keys are a
  single const row.
- Users who legitimately redirected an agent's home or shell via toml must
  move that to the parent environment (`HOME=… maestro …`); `apply_agent_env`
  logs a warning when a denied key is skipped.
