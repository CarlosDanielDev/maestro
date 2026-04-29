# Maestro

[![CI](https://github.com/CarlosDanielDev/maestro/actions/workflows/ci.yml/badge.svg)](https://github.com/CarlosDanielDev/maestro/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/CarlosDanielDev/maestro)](https://github.com/CarlosDanielDev/maestro/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV](https://img.shields.io/badge/rustc-1.89%2B-orange.svg)](Cargo.toml)

> Multi-session [Claude Code](https://claude.ai/claude-code) orchestrator with a Matrix-style terminal control center.

Maestro spawns and monitors multiple Claude Code sessions working on the same project simultaneously. It provides real-time visibility into what each session is doing, how much it's spending, and coordinates their work to prevent conflicts — all from a single TUI dashboard.

<!-- TODO(carlos): replace with an up-to-date capture showing the landing screen, F-key bar, and active session pool. -->
<img width="800" height="600" alt="Maestro TUI dashboard" src="https://github.com/user-attachments/assets/0f552553-43f5-4104-9533-975f92b9c71f" />

Most deep guides live in the [project Wiki](https://github.com/CarlosDanielDev/maestro/wiki). This README is the on-ramp.

## Features

### Session orchestration
- Multi-session pool — run up to N concurrent Claude sessions with automatic queue promotion
- Git worktree isolation per session prevents file conflicts
- File claim registry blocks two sessions from editing the same file simultaneously
- Full session state machine (QUEUED → SPAWNING → RUNNING → GATES_RUNNING → COMPLETED / NEEDS_REVIEW / ERRORED / PAUSED / KILLED)
- State and costs persisted to `maestro-state.json` for recovery and reporting

→ [Wiki › Sessions and Pool](https://github.com/CarlosDanielDev/maestro/wiki/Feature-Sessions-and-Pool)

### TUI dashboard
- Interactive landing screen with quick-actions menu, contextual work suggestions, and recent activity
- DOS-style F-key status bar (F1 Help, F2 Summary, F3 Full, F4 Costs, F5 Tokens, F6 Deps, F9 Pause, F10 Kill, Alt-X Exit)
- Rich activity log — file paths, command previews, tool durations, extended-thinking markers
- Status-transition flash on session state changes
- Completion-summary overlay when all sessions finish
- Nerd-font icons throughout

→ [Wiki › TUI Dashboard](https://github.com/CarlosDanielDev/maestro/wiki/Feature-TUI-Dashboard) · [Home and Landing Screens](https://github.com/CarlosDanielDev/maestro/wiki/Feature-Home-and-Landing-Screens)

### GitHub & Azure DevOps integration
- Multi-provider — auto-detected from the git remote, or set explicitly in `[provider]`
- Issue browser and milestone overview directly inside the TUI
- Issue/Milestone wizards for guided launches
- Label lifecycle: `maestro:ready` → `maestro:in-progress` → `maestro:done` / `maestro:failed`
- Dependency scheduling via `blocked-by:#N` labels and body references; priority ordering via `priority:P0/P1/P2`
- Automated PR creation on session completion with cost report and file list
- PR Review automation with optional bypass mode

→ [Wiki › Multi-Provider (GitHub & Azure)](https://github.com/CarlosDanielDev/maestro/wiki/Feature-Multi-Provider-GitHub-Azure) · [PR Review Automation](https://github.com/CarlosDanielDev/maestro/wiki/Feature-PR-Review-Automation) · [Issue and Milestone Wizards](https://github.com/CarlosDanielDev/maestro/wiki/Feature-Issue-and-Milestone-Wizards)

### Quality & autonomy
- `maestro doctor` preflight checks — verifies `claude`, `gh`/`az`, `git` are installed and authenticated before spending API credits
- Configurable completion gates (fmt / clippy / test or any custom command) run after every session; failures of required gates block PR creation
- Language-aware session prompt guardrails (Rust / TS / Python / Go) auto-injected, overridable via `guardrail_prompt`
- Continuous mode (`--continuous` / `-C`) auto-advances through ready issues with a pause overlay on failure
- Context-overflow auto-fork with structured handoff prompts and configurable fork-depth limit
- Smart retry policies on transient errors

→ [Wiki › Doctor and Preflight](https://github.com/CarlosDanielDev/maestro/wiki/Feature-Doctor-and-Preflight) · [Completion Gates](https://github.com/CarlosDanielDev/maestro/wiki/Feature-Completion-Gates) · [Context Overflow and Forking](https://github.com/CarlosDanielDev/maestro/wiki/Feature-Context-Overflow-and-Forking) · [Milestone Mode and Continuous](https://github.com/CarlosDanielDev/maestro/wiki/Feature-Milestone-Mode-and-Continuous)

### Power features
- TurboQuant context compression (PolarQuant + QJL residual) with a runtime toggle via `Ctrl+q`
- Cost and token dashboards with per-session and total spend tracking
- Self-upgrade with automatic backup and restart confirmation
- Shell completions (bash / zsh / fish) and a man page in every release
- Desktop and Slack notifications

→ [Wiki › TurboQuant Compression](https://github.com/CarlosDanielDev/maestro/wiki/Feature-TurboQuant-Compression) · [Cost and Token Dashboards](https://github.com/CarlosDanielDev/maestro/wiki/Feature-Cost-and-Token-Dashboards) · [Self-Upgrade](https://github.com/CarlosDanielDev/maestro/wiki/Feature-Self-Upgrade) · [Notifications](https://github.com/CarlosDanielDev/maestro/wiki/Feature-Notifications-Desktop-and-Slack)

## Requirements

- **Rust 1.89+** (edition 2024)
- **Claude Code CLI** (`claude`) installed and on your `PATH`
- **GitHub CLI** (`gh`) — required when using the GitHub provider (default)
- **Azure CLI** (`az`) — required when using the Azure DevOps provider; run `az login` to authenticate
- macOS, Linux, or WSL

## Install

### Homebrew (macOS and Linux)

```bash
brew tap CarlosDanielDev/tap
brew install carlosdanieldev/tap/maestro --formula
```

> **Note:** The `--formula` flag is required because an unrelated cask named "maestro" exists in Homebrew core.

### Pre-built binary

Download the tarball for your platform from the [latest GitHub Release](https://github.com/CarlosDanielDev/maestro/releases/latest), extract it, and place the `maestro` binary on your `PATH`.

| Platform | Archive |
|----------|---------|
| macOS (Apple Silicon) | `maestro-<version>-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `maestro-<version>-x86_64-apple-darwin.tar.gz` |
| Linux (x86_64) | `maestro-<version>-x86_64-unknown-linux-gnu.tar.gz` |

A `sha256sums.txt` file is included for checksum verification.

### From source

```bash
git clone https://github.com/CarlosDanielDev/maestro.git
cd maestro
cargo build --release
# Binary at target/release/maestro
```

### Updating

Maestro checks for new versions on startup and shows an in-TUI banner — press `[u]` to upgrade with automatic backup and restart confirmation. You can also run `brew upgrade carlosdanieldev/tap/maestro --formula` (Homebrew) or `git pull && cargo build --release` (source).

→ [Wiki › Self-Upgrade](https://github.com/CarlosDanielDev/maestro/wiki/Feature-Self-Upgrade)

## Quick Start

```bash
maestro init                                              # generate maestro.toml
maestro doctor                                            # verify gh/az/claude/git
maestro run --prompt "Refactor the auth module to async"  # ad-hoc session
maestro run --issue 42                                    # session for a GitHub issue
maestro run --milestone "v1.0"                            # all open issues in a milestone
maestro                                                   # open the TUI dashboard
```

For the full command catalogue see [Wiki › CLI Reference](https://github.com/CarlosDanielDev/maestro/wiki/CLI-Reference) or run `maestro --help`.

## Configuration

Maestro reads `maestro.toml` from the project root. Run `maestro init` to generate it — the command auto-detects your project's tech stack (Rust, Node, Python, Go, or polyglot) and fills in sensible defaults for `build_command`, `test_command`, and `run_command`. Run `maestro init --reset` to re-detect and merge results into an existing file (existing keys are preserved).

A minimal `maestro.toml`:

```toml
[project]
repo        = "owner/repo"
base_branch = "main"

[sessions]
max_concurrent = 3            # parallel Claude sessions
default_model  = "opus"       # opus | sonnet | haiku
default_mode   = "orchestrator"

[budget]
per_session_usd     = 5.0
total_usd           = 50.0
alert_threshold_pct = 80      # warn at 80% of budget

[github]
auto_pr = true
```

The full schema — completion gates, context-overflow tuning, TurboQuant, provider/Azure DevOps configuration, notifications, and feature flags — is documented in [Wiki › Configuration Reference](https://github.com/CarlosDanielDev/maestro/wiki/Configuration-Reference).

## Architecture

See [`directory-tree.md`](directory-tree.md) for the full source layout. At a high level:

1. `src/cli.rs` parses the command and dispatches to a handler.
2. The session pool spawns `claude` subprocesses, each in an isolated git worktree, parsing their `stream-json` output.
3. The ratatui TUI renders the pool state, activity log, and dashboards from a shared in-memory store persisted to `maestro-state.json`.

→ [Wiki › Architecture](https://github.com/CarlosDanielDev/maestro/wiki/Architecture)

## GitHub Label System

Maestro reads and writes a small set of GitHub labels to coordinate work. All names are defined in `src/provider/github/types.rs`.

| Label | Purpose |
|-------|---------|
| `maestro:ready` | Queued and ready for a session to pick up (managed) |
| `maestro:in-progress` | A session is currently working on this issue (managed) |
| `maestro:done` | Session completed successfully and PR was opened (managed) |
| `maestro:failed` | Session ended with an error (managed) |
| `priority:P0` / `P1` / `P2` | Scheduling priority — P0 first (set by you) |
| `mode:orchestrator` / `mode:vibe` | Run session in the named mode (set by you) |
| `blocked-by:#N` | Issue cannot start until #N closes (also accepted in body) |

## Keyboard Shortcuts

The full key map (global, overview, home screen, issue browser, milestone overview, prompt input) is at [Wiki › Keyboard Shortcuts](https://github.com/CarlosDanielDev/maestro/wiki/Keyboard-Shortcuts).

## Shell Completions

Maestro ships pre-generated bash, zsh, and fish completions plus a `maestro.1` man page in every release tarball; Homebrew installs them automatically. To regenerate on demand: `maestro completions <bash|zsh|fish>`. Per-shell installation paths and zsh `fpath` configuration are covered in [Wiki › Installation](https://github.com/CarlosDanielDev/maestro/wiki/Installation).

## Integration with `.claude/`

Maestro wraps — not replaces — your existing `.claude/` agent system. Each spawned Claude session inherits your project's `CLAUDE.md`, agents, skills, and commands. Maestro adds coordination context (file claims, peer awareness) via `--append-system-prompt`.

## Roadmap

[`ROADMAP.md`](ROADMAP.md) is the single source of truth for milestones and implementation order; [`CHANGELOG.md`](CHANGELOG.md) has detailed release notes. The active milestone is **[v0.17.0 – Documentation & Community](https://github.com/CarlosDanielDev/maestro/milestone/28)**; browse all open milestones at [github.com/CarlosDanielDev/maestro/milestones](https://github.com/CarlosDanielDev/maestro/milestones), and the latest released version is always at [releases/latest](https://github.com/CarlosDanielDev/maestro/releases/latest).

## Contributing

Contributions are welcome. The full developer guide lives in [Wiki › Contributing](https://github.com/CarlosDanielDev/maestro/wiki/Contributing).

Quick orientation:

```bash
cargo build           # debug build
cargo test            # run tests
cargo clippy && cargo fmt    # lint and format
```

The Rust coding policy — error handling, async discipline, `unsafe`, testing, dependencies, style — is in [`docs/RUST-GUARDRAILS.md`](docs/RUST-GUARDRAILS.md). The `.claude/` directory hosts the orchestrator-mode agent system; see [`.claude/CLAUDE.md`](.claude/CLAUDE.md) for the workflow rules.

## License

[MIT](LICENSE)
