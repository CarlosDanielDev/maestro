# Maestro Roadmap

> Single source of truth for shipped and upcoming work.
> See [CHANGELOG.md](CHANGELOG.md) for detailed release notes per version.

---

## Released

### v0.28.0 — Template sync engine ✅ (2026-05-16)

Canonical command specs in `.maestro/templates/`, rendered per-provider via `maestro sync-templates`. SHA-256 lockfile + CI drift check. HTTP-provider runtime template injection. Cross-platform release tarball ergonomics.

### v0.26.0 — Team orchestration ✅ (2026-05-12)

Multi-agent coordination layer: L1/L2/L3 scheduler, five built-in presets (`default-coder`, `default-researcher`, `default-triager`, `default-reviewer`, `default-docs`), TUI launch wizard, CLI `team` subcommands (`list`, `new`, `launch`, `manage`, `explain`), three-tier preset resolution (built-in → user → project), headless `--yes` mode for CI.

### v0.25.1 — CI gates hardening ✅ (2026-05-06)

`cargo-dupes` regression gate added to CI; release-script resilience fix.

### v0.25.0 — Multi-agent runtime ✅ (2026-05-06)

Added Codex, Qwen, OpenCode (subprocess) and Ollama, MiniMax (HTTP) providers alongside Claude. Provider selected via `--agent <id>` or `[agents].default`; subprocess vs HTTP transport abstracted behind a single trait.

### v0.24.1 — README hero polish ✅ (2026-05-05)

Replaced distorted README hero image with a snapshot-driven SVG render produced by `scripts/render-readme-hero.sh`.

---

## What's Next

Items below are sourced from explicitly-deferred decisions in `docs/superpowers/specs/*.md`. They are candidates, not commitments — scope is set per release.

### From the orchestration-wizard spec (deferred to v2+)

- **Versioned `extends` syntax** — `extends = "default-coder@v1"` to insulate user presets from built-in drift; today, breaking changes to built-in presets are documented in CHANGELOG only.
- **Migration `assist` command** — convert legacy `[modes.*]` configs into team presets automatically.
- **Real-time team coordination** — subagents talking to each other mid-run (currently each run is independent).
- **Auto-tuning team bindings** based on past success rates.
- **Team marketplaces / preset sharing** via a registry.

### From the implement-harness-enforcement spec (deferred to v2)

- **Baseline-green cost optimization** — swap the full `cargo test` baseline assertion for `cargo test --no-run --quiet` (compile-only) to cut 30+ seconds off every `/implement` run on large projects.

### Ongoing

- **CI quality gates depth** — mutation testing, coverage tiers, and miri/ThreadSanitizer integration land incrementally per the CI gates design doc.

---

## How to follow along

- **Open milestones:** [github.com/CarlosDanielDev/maestro/milestones](https://github.com/CarlosDanielDev/maestro/milestones)
- **Latest release binary:** [releases/latest](https://github.com/CarlosDanielDev/maestro/releases/latest)
- **Per-release notes:** [`CHANGELOG.md`](CHANGELOG.md)
