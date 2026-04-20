---
name: project-patterns
version: "1.0.0"
description: Maestro project patterns — Rust CLI with ratatui TUI, tokio async, Claude CLI process management, and stream-json parsing.
allowed-tools: Read, Grep, Glob, WebSearch
---

# Maestro Project Patterns

> Quick reference for development patterns used in the maestro project.

## Technology Stack

- **Language**: Rust (2024 edition)
- **TUI**: ratatui + crossterm
- **Async**: tokio (full features)
- **CLI**: clap (derive macros)
- **Serialization**: serde + serde_json + toml
- **Error Handling**: anyhow

## Module Structure

```
src/
├── main.rs              # CLI entry (clap), command dispatch
├── config.rs            # maestro.toml parsing (serde + toml)
├── session/
│   ├── mod.rs
│   ├── types.rs         # Session state machine, StreamEvent enum
│   ├── parser.rs        # Claude stream-json line parser
│   └── manager.rs       # Process spawn, lifecycle, stdin/stdout
├── state/
│   ├── mod.rs
│   ├── types.rs         # MaestroState, file claims
│   └── store.rs         # JSON persistence (atomic write-rename)
└── tui/
    ├── mod.rs           # Terminal setup, async event loop
    ├── app.rs           # App state, session event handling
    └── ui.rs            # ratatui rendering (panels, gauges, logs)
```

## Patterns

### State Machine for Sessions
Sessions follow: `QUEUED → SPAWNING → RUNNING → COMPLETED/ERRORED/PAUSED/KILLED`
- Use `SessionStatus` enum with `is_terminal()` helper
- State transitions happen in `ManagedSession::handle_event()`

### Stream-JSON Parsing
- Claude CLI outputs one JSON object per line
- Parser is line-by-line, returns `StreamEvent` enum
- Key event types: `assistant` (text/tool_use), `tool_result`, `result`, `error`

### Atomic State Persistence
- Write to `.tmp` file, then `rename()` for atomicity
- State is a single `MaestroState` struct serialized as JSON

### TUI Architecture
- `App` owns all state (sessions, activity log, event channels)
- `ui::draw()` is pure rendering (takes `&App`, produces frame)
- Event loop: poll keyboard (50ms timeout) + drain session events
- `mpsc::unbounded_channel` for session → TUI communication

### Error Handling
- Use `anyhow::Result` for all fallible operations
- Use `.context()` for adding context to errors
- Never `unwrap()` in production paths

### Testing
- Unit tests in the same file: `#[cfg(test)] mod tests { ... }`
- Use `#[test]` for sync tests, `#[tokio::test]` for async
- Parser has comprehensive tests for all event types

## Anti-Patterns to Avoid
- Blocking the TUI event loop (use tokio::spawn for long operations)
- Mutable borrow conflicts in App (extract data before calling &mut self methods)
- Direct stdout writes (TUI owns the terminal)
- `unwrap()` or `expect()` on external input

## Rust Guardrails

**Canonical policy:** `docs/RUST-GUARDRAILS.md` — the single source of truth for Rust coding policy in maestro. Consult this doc before proposing new modules, introducing new dependencies, or reviewing PRs that touch error handling, async, unsafe, serialization, or observability.

Quick-reference: the 8 principles (safe-by-default, errors-are-values, async hygiene, ownership-over-aliasing, tests-as-specification, supply-chain caution, readability-is-a-feature, observability).

**Detailed sidecar:** `rust-guardrails.md` in this skill directory — distilled rules with pattern exemplars (file paths and line numbers) from the maestro codebase. Read when you need a shorter answer than the full doc.

**Enforced by:**
- `Cargo.toml [lints]` — `unsafe_code = "deny"`, `expect_used = "warn"`, `dbg_macro = "deny"`.
- `rustfmt.toml`, `clippy.toml`, `deny.toml`, `rust-toolchain.toml` at repo root.
- CI jobs: `test`, `clippy`, `fmt`, `file-size`, `deny` (non-blocking), `audit` (non-blocking).
- Completion gates in `maestro.toml [sessions.completion_gates]`.
