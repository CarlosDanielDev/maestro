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

### Roadmap

| Phase | What | Status |
|-------|------|--------|
| **0** | Single-session TUI, stream parser, state persistence | ✅ Done |
| **1** | Multi-session pool, split-pane TUI, file claim system, git worktrees | Planned |
| **2** | GitHub integration — issue fetching, auto-PR, label lifecycle | Planned |
| **3** | Intelligence — context overflow detection, budget enforcement, stall detection | Planned |
| **4** | Plugin system, mode system, cost dashboard, session resumption | Planned |

## Requirements

- **Rust 1.75+** (tested on 1.94)
- **Claude Code CLI** (`claude`) installed and on your PATH
- macOS, Linux, or WSL

## Install

```bash
# From source
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

[notifications]
desktop = true
slack = false
```

## Architecture

```
maestro (Rust binary)
├── src/
│   ├── main.rs              # CLI entry point (clap)
│   ├── config.rs            # maestro.toml parsing
│   ├── session/
│   │   ├── types.rs         # Session state machine, StreamEvent
│   │   ├── parser.rs        # Claude stream-json line parser
│   │   └── manager.rs       # Process spawn, stdin/stdout, lifecycle
│   ├── state/
│   │   ├── types.rs         # MaestroState, file claims
│   │   └── store.rs         # JSON persistence (atomic writes)
│   └── tui/
│       ├── app.rs           # App state, event coordination
│       ├── ui.rs            # ratatui rendering (panels, gauges, logs)
│       └── mod.rs           # Terminal setup, async event loop
├── Cargo.toml
└── maestro.toml             # Default config
```

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
