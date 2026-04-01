# Maestro

> Multi-session Claude Code orchestrator with a Matrix-style terminal control center.

Maestro spawns and monitors multiple [Claude Code](https://claude.ai/claude-code) sessions working on the same project simultaneously. It provides real-time visibility into what each agent is doing, how much it's spending, and coordinates their work to prevent conflicts — all from a single TUI dashboard.

```
+==================================================================+
|  MAESTRO v0.1.0  |  3 agents  |  $8.45 spent  |  14:32:01       |
+==================================================================+
|  AGENT 1          |  AGENT 2          |  AGENT 3                 |
|  #42 Streaks      |  #43 Animations   |  #44 Bot Profile         |
|  ▶ RUNNING 4m32s  |  ▶ RUNNING 2m15s  |  ⏳ QUEUED               |
|  ctx: 45% ████░░  |  ctx: 23% ██░░░░  |  waiting for #43        |
|  $1.23            |  $0.87            |  $0.00                   |
|                   |                   |                          |
|  > Reading        |  > Writing        |  (pending)               |
|    StreakService   |    GameView.swift |                          |
+-------------------+-------------------+--------------------------+
|  ACTIVITY LOG                                                     |
|  14:32:01  [#42] Reading docs/api-contracts/streaks.json         |
|  14:31:58  [#43] Wrote GameView.swift                            |
+-------------------------------------------------------------------+
|  [q]uit [p]ause [k]ill [r]efresh [?]help                        |
+-------------------------------------------------------------------+
```

## Features

- **Single-session TUI** — spawn a Claude Code session and watch it work in real-time
- **Live stream parsing** — parses Claude CLI `stream-json` output for tool usage, messages, and costs
- **Session lifecycle** — QUEUED → SPAWNING → RUNNING → COMPLETED/ERRORED/PAUSED/KILLED
- **Keyboard controls** — pause (SIGSTOP), resume, kill sessions from the dashboard
- **State persistence** — session history and costs saved to `maestro-state.json`
- **Cost tracking** — per-session and total spending displayed in real-time
- **Multi-session pool** — run up to N concurrent Claude sessions with automatic queue promotion
- **Git worktree isolation** — each session works in its own worktree to prevent file conflicts
- **File claim system** — registry prevents two sessions from editing the same file simultaneously
- **GitHub issue queue** — fetch `maestro:ready`-labeled issues and run them as sessions
- **Milestone mode** — `--milestone <name>` runs all open issues in a milestone
- **Label lifecycle** — issues are automatically transitioned: `ready` → `in-progress` → `done`/`failed`
- **Automated PR creation** — on session completion, a PR is opened with cost report and file list
- **Dependency scheduling** — `blocked-by:#N` labels and body references create an ordered work graph
- **Priority ordering** — `priority:P0/P1/P2` labels determine scheduling order within the queue
- **Context overflow detection** — monitors context window usage per session; automatically forks into a continuation session at a configurable threshold with a structured handoff prompt
- **Fork depth limiting** — configurable maximum fork chain depth prevents runaway continuation loops
- **Multi-provider support** — works with GitHub (via `gh` CLI) or Azure DevOps (via `az` CLI); provider is auto-detected from the git remote or set explicitly in config

### Roadmap

| Phase | What | Status |
|-------|------|--------|
| **0** | Single-session TUI, stream parser, state persistence | Done |
| **1** | Multi-session pool, split-pane TUI, file claim system, git worktrees | Done |
| **2** | GitHub integration — issue fetching, auto-PR, label lifecycle, dependency graph | Done |
| **3** | Intelligence — context overflow detection, budget enforcement, stall detection | Done |
| **4** | Plugin system, mode system, cost dashboard, session resumption | Done |
| **5** | Multi-provider support — GitHub and Azure DevOps | Done |

## Requirements

- **Rust 1.75+** (tested on 1.94)
- **Claude Code CLI** (`claude`) installed and on your PATH
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

Supported targets:

| Platform | Archive |
|----------|---------|
| macOS (Apple Silicon) | `maestro-<version>-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `maestro-<version>-x86_64-apple-darwin.tar.gz` |
| Linux (x86_64) | `maestro-<version>-x86_64-unknown-linux-gnu.tar.gz` |

A `sha256sums.txt` file is included in each release for checksum verification.

### From source

```bash
git clone https://github.com/CarlosDanielDev/maestro.git
cd maestro
cargo build --release
# Binary at target/release/maestro
```

## Quick Start

```bash
# Initialize config
maestro init

# Run a single session with a prompt
maestro run --prompt "Refactor the auth module to use async/await"

# Run a session for a GitHub issue
maestro run --issue 42

# Run all open issues in a milestone (respects priority and dependencies)
maestro run --milestone "v1.0"

# Show all queued issues labelled maestro:ready
maestro queue

# Add a single issue to the work queue manually
maestro add 42

# Open the dashboard (empty, for monitoring)
maestro

# Check session status (no TUI)
maestro status

# View spending report
maestro cost
```

## Configuration

Maestro reads `maestro.toml` from the project root:

```toml
[project]
repo = "owner/repo"
base_branch = "main"

[sessions]
max_concurrent = 3        # Max parallel Claude sessions
stall_timeout_secs = 300  # Kill stalled sessions after 5 min
default_model = "opus"    # opus, sonnet, haiku
default_mode = "orchestrator"

[budget]
per_session_usd = 5.0     # Max spend per session
total_usd = 50.0          # Global budget cap
alert_threshold_pct = 80  # Warn at 80% of budget

[github]
issue_filter_labels = ["maestro:ready"]
auto_pr = true
cache_ttl_secs = 300        # How long issue data is cached (default: 5 min)

[notifications]
desktop = true
slack = false

[sessions.context_overflow]
overflow_threshold_pct = 70  # Auto-fork when context reaches this % (default: 70)
auto_fork = true             # Spawn a continuation session on overflow
commit_prompt_pct = 50       # Prompt an intermediate commit at this % (default: 50)
max_fork_depth = 5           # Max chained forks before overflow is ignored

# Optional: explicit provider configuration (auto-detected from git remote by default)
[provider]
kind = "github"              # "github" (default) or "azure_devops"

# Azure DevOps example:
# [provider]
# kind = "azure_devops"
# organization = "https://dev.azure.com/MyOrg"
# az_project = "MyProject"
```

## Architecture

See [directory-tree.md](directory-tree.md) for the complete project structure.

```
maestro (Rust binary)
├── src/
│   ├── main.rs              # CLI entry point (clap); Run/Queue/Add/Status/Cost/Init
│   ├── config.rs            # maestro.toml parsing; ProviderConfig
│   ├── provider/            # Multi-provider abstraction [Issue #29]
│   │   ├── mod.rs           # create_provider factory; detect_provider_from_remote
│   │   ├── types.rs         # ProviderKind (Github, AzureDevops); type re-exports
│   │   └── azure_devops.rs  # AzDevOpsClient (shells out to `az`)
│   ├── github/              # GitHub API integration [Phase 2]
│   │   ├── types.rs         # GhIssue, Priority, MaestroLabel, SessionMode
│   │   ├── client.rs        # GitHubClient trait + GhCliClient (shells out to `gh`)
│   │   ├── labels.rs        # Label lifecycle: ready→in-progress→done/failed
│   │   └── pr.rs            # Auto PR creation with cost report
│   ├── session/
│   │   ├── types.rs         # Session state machine, StreamEvent, issue_title
│   │   ├── parser.rs        # Claude stream-json line parser
│   │   ├── manager.rs       # Process spawn, stdin/stdout, lifecycle
│   │   ├── pool.rs          # Concurrent session pool [Phase 1]
│   │   └── worktree.rs      # Git worktree isolation [Phase 1]
│   ├── state/
│   │   ├── types.rs         # MaestroState, file claims, issue_cache
│   │   └── store.rs         # JSON persistence (atomic writes)
│   ├── work/                # Work queue and scheduling [Phase 2]
│   │   ├── types.rs         # WorkItem, WorkStatus
│   │   ├── dependencies.rs  # DAG: topological sort, cycle detection
│   │   └── assigner.rs      # Priority-ordered queue assignment
│   └── tui/
│       ├── app.rs           # App state, WorkAssigner, GitHubClient integration
│       ├── ui.rs            # ratatui rendering (panels, gauges, logs)
│       └── mod.rs           # Terminal setup, async event loop
├── Cargo.toml
└── maestro.toml             # Default config
```

## GitHub Label System

Maestro reads and writes GitHub labels to coordinate work. All label names are defined in `src/github/types.rs`.

### Status Labels (managed by Maestro)

| Label | Meaning |
|-------|---------|
| `maestro:ready` | Issue is queued and ready for a session to pick up |
| `maestro:in-progress` | A session is currently working on this issue |
| `maestro:done` | Session completed successfully; PR was opened |
| `maestro:failed` | Session ended with an error |

### Scheduling Labels (set by you)

| Label | Meaning |
|-------|---------|
| `priority:P0` | Highest priority — scheduled first |
| `priority:P1` | Medium priority |
| `priority:P2` | Default priority |
| `mode:orchestrator` | Run session in Orchestrator mode |
| `mode:vibe` | Run session in Vibe Coding mode |
| `blocked-by:#N` | This issue cannot start until issue #N is done |

Dependencies can also be declared in the issue body as `blocked-by: #N` (case-insensitive).

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `q` | Quit maestro (kills all sessions) |
| `p` | Pause all running sessions (SIGSTOP) |
| `r` | Resume all paused sessions (SIGCONT) |
| `k` | Kill all sessions |
| `Ctrl+C` | Emergency exit |

## How It Works

1. Maestro spawns `claude --print --output-format stream-json --model <model> "<prompt>"`
2. Parses the JSON stream line-by-line to extract tool usage, messages, costs
3. Renders real-time state in a ratatui TUI with agent panels and activity log
4. Persists session state to `maestro-state.json` for recovery and reporting

## Integration with `.claude/`

Maestro wraps — not replaces — your existing `.claude/` agent system. Each spawned Claude session inherits your project's `CLAUDE.md`, agents, skills, and commands. Maestro adds coordination context (file claims, peer awareness) via `--append-system-prompt`.

## License

MIT
