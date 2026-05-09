# `default-docs` — single-pass documentation team

A one-shot docs dispatch. Use to refresh README, CHANGELOG, inline docs, or `docs/` content without touching code.

## Canonical TOML

```toml
extends = ""
primitive = "single-pass"
min_agents = ["claude"]
docs = "claude"
```

## Bindings

| Role | Agent | Notes |
|---|---|---|
| docs | `claude` | walks the relevant code, updates surrounding documentation, reports `DocsChange` |

## When to use

- Documentation issue (`type:docs` label) that has no code changes.
- Backfill — refreshing docs after several feature merges that landed without doc updates.
- Pre-release docs sweep before cutting a tag.

## Output shape

`PrimitiveOutput::SinglePass { role: Docs, result: DocsChange { files_touched, summary } }`.

## Customise — dual-locale docs

If your project ships docs in multiple languages, override the docs agent with a translation-aware mode:

```toml
# ~/.config/maestro/maestro/teams/i18n-docs.toml
extends = "default-docs"

[role_overrides.docs]
agent = "claude"
mode = "docs-i18n"
prompt_addendum = "Update both docs/en/ and docs/pt/. Keep them structurally aligned."
```

Wire `[modes.docs-i18n]` in `maestro.toml` to define `system_prompt`, `allowed_tools`, and `permission_mode`.

## Inspect

```sh
maestro team explain default-docs
```

## See also

- [`default-coder.md`](default-coder.md) — pipeline uses a `docs` step at the end
- [`README.md`](README.md) — preset overview
