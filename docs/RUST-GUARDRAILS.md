# Rust Development Guardrails for maestro

**Status:** active  
**Toolchain:** Rust 1.94.1 (edition 2024)  
**Enforcement:** CI (clippy `-D warnings`, fmt check, file-size, `cargo deny`) + completion gates (`maestro.toml [sessions.completion_gates]`) + subagents cite specific sections at planning time.  
**Amendment:** open a PR editing this file + a line in `CHANGELOG.md`.

This document is the single source of truth for Rust coding policy in maestro. It is pragmatic: it forbids footguns that cause real regressions and documents the patterns that are already working so new contributors (human or AI) don't have to rediscover them. It does not chase aspirational lints (clippy::pedantic, clippy::nursery) or ceremony that adds friction without ROI.

Grounding sources:

- **Rust by Example** (`resources/rust-by-example.pdf` in vault) — chapters on Error handling, Testing, Unsafe Operations, Traits, Generics, Scoping rules.
- **Rust Compiler Development Guide** (`resources/rust-development-guide.pdf`) — used sparingly, mostly for test-infrastructure and tidy conventions.
- **Rust API Guidelines** (https://rust-lang.github.io/api-guidelines/) — the C-* conventions.
- **Clippy lint docs** (https://rust-lang.github.io/rust-clippy/master/) — for every lint we enable.
- **RustSec advisory DB** (https://rustsec.org/) — consulted via `cargo audit`.

---

## 0. Principles (the 8 rules)

1. **Safe by default.** Zero `unsafe` in the crate. New `unsafe` requires a Safety comment + ADR. Enforced via `#![forbid(unsafe_code)]` at `src/main.rs` and `src/lib.rs`.
2. **Errors are values; panics are bugs.** `Result<T, E>` everywhere in library code. `anyhow::Context` at I/O / subprocess / fs / network boundaries. Typed errors (hand-rolled enums with `Display + Error`, as in `src/session/transition.rs`) at module seams that callers must branch on.
3. **Async hygiene.** No blocking calls in async context — wrap in `tokio::task::spawn_blocking`. Prefer bounded channels (`mpsc::channel(N)`) over unbounded except for the one documented event bus. Every spawned task has a bounded shutdown path.
4. **Ownership over aliasing.** Minimize `Arc<Mutex<...>>`. Prefer channels + owned state machines (maestro's session pool pattern). If `Arc` is used, it must be justified in a comment.
5. **Tests as specification.** Unit tests inline (`#[cfg(test)] mod tests`); integration tests under `src/integration_tests/`. Real fakes preferred over mocks. Insta for user-visible output. `#[tokio::test]` for async. No `#[ignore]` without a linked issue.
6. **Supply-chain caution.** MSRV pinned via `rust-toolchain.toml`. `cargo deny` in CI. No wildcard versions. Licenses on an allow list.
7. **Readability is a feature.** rustfmt non-negotiable. clippy `-D warnings` non-negotiable. File ≤ 500 LOC target (hard cap enforced by `scripts/check-file-size.sh`). Function ≤ 80 LOC soft cap.
8. **Observability.** `tracing` over `println!` / `eprintln!` / `dbg!` in committed code. Structured fields with context. Error paths log at `warn` or `error` with enough detail to diagnose without a debugger.

---

## 1. Project layout

**Module discipline.** `src/lib.rs` exposes only self-contained modules for benchmarks (`icon_mode`, `icons`, `turboquant`, `util`, `session::{parser, transition, types}`). `src/main.rs` owns the binary; everything else is private to the binary unless promoted into `lib.rs` with intent.

**Subsystem layout.** Top-level modules correspond to real subsystems, not to type kinds:

```
src/
├── main.rs                  # CLI entry, logging, dispatch
├── lib.rs                   # narrow bench-facing surface
├── cli.rs                   # clap command tree
├── config.rs                # maestro.toml parsing (near LOC cap — split before growing)
├── session/                 # Claude CLI subprocess lifecycle, stream parsing, state machine
├── state/                   # JSON persistence, file claims, progress, prompt history
├── gates/                   # Completion-gate framework (fmt/clippy/test)
├── tui/                     # ratatui screens, widgets, drawing
├── adapt/                   # adapt pipeline
├── turboquant/              # context compression
├── provider/github/         # gh CLI wrappers, trait + mock
└── integration_tests/       # cross-module integration tests
```

**Visibility.** `pub mod` is explicit. No `pub use super::*`. Re-exports name their targets. New modules start private (`mod foo`) and are promoted to `pub mod foo` only when another module needs them.

**File size.** Soft target 500 LOC; hard cap enforced by `scripts/check-file-size.sh` (CI job `file-size`). Files approaching the cap (`config.rs` at ~1,623 LOC, `cli.rs` at ~1,009 LOC) must be split before adding new responsibilities.

**Function size.** Soft cap 80 LOC. A function over 80 LOC is a review signal, not a violation. Break out helpers named after what they compute (not `do_stuff`).

---

## 2. Error policy

**Two tiers.**

**Tier A — `anyhow` at boundaries.** fs, subprocess, network, serialization boundaries return `anyhow::Result<T>` with `.context("…")`. The template is `src/state/store.rs:19-67`:

```rust
// Good: every I/O op gets a .context() describing *what* was happening.
let file = OpenOptions::new().create(true).write(true).open(&lock_path)
    .with_context(|| format!("opening lock file {}", lock_path.display()))?;
```

Rules:
- `.context()` is not optional on `std::io::Error`, `serde_json::Error`, `std::process::Command` errors.
- Prefer `.with_context(|| …)` (lazy) over `.context(…)` when the message needs formatting.
- Error messages start lowercase, no trailing period, active voice: `"opening lock file /path/foo"` not `"Failed to open the lock file at /path/foo."`.

**Tier B — typed enums at seams.** When a caller needs to branch on error kind (e.g., "is this a transient failure or permanent?"), define a hand-rolled enum that implements `Display + std::error::Error`. Template is `src/session/transition.rs:37-55`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IllegalTransition {
    pub from: SessionStatus,
    pub to: SessionStatus,
}
impl std::fmt::Display for IllegalTransition { /* ... */ }
impl std::error::Error for IllegalTransition {}
```

When to reach for `thiserror`: if you end up writing 3+ hand-rolled `impl Display` + `impl Error` blocks in a single module. Until then, stay with hand-rolled — it's visible and reviewable.

**Forbidden.**
- `.unwrap()` and `.expect()` in `src/` (lints set to `warn`; promote to `deny` at module top when the module has zero exceptions).
- `panic!()` as flow control. `panic!()` is only for: (a) CLI arg parsing setup in `main.rs`, (b) tests, (c) `unreachable!()` after an exhaustive `match`.

**Allowed exceptions** (document with a `// Reason: …` comment):
- `.expect("static: …")` for indexing into a compile-time-known array / slice where the bound is provably correct.
- `unwrap_or_default()` is fine — it's not a panic.

**Error-log discipline.** When converting a `Result<T, E>` to a side effect (log + ignore), log at `warn!` or `error!`, include the context, and explain in a comment why continuing is safe.

---

## 3. Async policy

**Runtime.** Single tokio runtime in `main.rs` via `#[tokio::main]`. Never `Runtime::new()` inside a module — that's a nested-runtime hazard.

**Blocking.** Any operation that could block for > 5 ms (std fs, sync HTTP, heavy CPU) goes in `tokio::task::spawn_blocking`. maestro's `adapt/scanner.rs` uses this correctly for project scans.

**Spawn discipline.** Every `tokio::spawn` has:
1. A named purpose in a comment or function name (not "background task").
2. A shutdown path — either a `CancellationToken`, a `mpsc` receiver dropped by the owner, or a finite work item.
3. An explicit error log on panic/exit (tasks that silently die are the worst class of TUI bug).

**Channels.**
- Default to **bounded** `tokio::sync::mpsc::channel(N)`. Pick N based on expected burst size, not "infinity".
- `unbounded_channel` is allowed for the session→TUI event bus (documented exception — event loss is worse than memory growth in that path) and for oneshot-like signals.
- `tokio::sync::broadcast` only when fan-out is required and late joiners can survive missed messages.

**Mutex / critical sections.**
- `std::sync::Mutex` is fine when the critical section does not cross `.await`.
- `tokio::sync::Mutex` is required when you must hold the lock across `.await`. This is rare and a code smell — usually a redesign via message passing is better.
- **Never `.await` inside a `parking_lot::Mutex` guard.** parking_lot locks are not cancel-safe.

**Cancellation.** Long-running tasks (session event loops, file watchers) respect a shutdown signal and exit within ~1s. Tests enforce the shutdown invariant where feasible.

---

## 4. Concurrency & shared state

**Rule of least aliasing.** Prefer owned data + channels over `Arc<Mutex<T>>`. maestro's session pool is the exemplar: `SessionPool` owns a `Vec<ManagedSession>` and mutates via `&mut self`; cross-session communication happens through the event channel, not shared state.

**`Arc` acceptable uses.**
- Sharing read-only configuration or adapters (`Arc<TurboQuantAdapter>` in `src/session/fork.rs:40`).
- Test mocks where the assertion state must outlive the caller (`Arc<Mutex<MockState>>` in `src/provider/github/client.rs`). Production code does not mirror this.

**`Atomic*` acceptable uses.**
- Counters and flags where `SeqCst` ordering is obviously safe (e.g., `Arc<AtomicUsize>` counting widget reads).

**Anti-patterns.**
- `Arc<Mutex<State>>` as the primary state holder — redesign via `&mut self` + message passing.
- `RwLock` — almost always the wrong answer. If reads vastly outnumber writes, use `Arc<Snapshot>` swaps.
- Hidden `Sync` assumptions — if your type holds a raw pointer or `*const T`, it's not Sync and must be wrapped explicitly.

---

## 5. Subprocess & I/O

Reference: `src/session/manager.rs` (Claude CLI subprocess lifecycle).

**Spawning.**
- Use `tokio::process::Command` for async subprocess management.
- Always pipe stdin/stdout/stderr explicitly with `Stdio::piped()` / `Stdio::null()` — never rely on defaults.
- Capture stdout via `AsyncBufReadExt::lines()` for line-oriented protocols (`src/session/parser.rs` pattern).

**Lifecycle.**
- Store `Child` handles owned by the spawning struct, not leaked into `Arc`.
- Implement kill-on-drop where the subprocess is tied to a session's lifetime — or kill explicitly in the `Drop` impl with a comment explaining why.
- Wait on subprocesses before dropping state they reference.

**Timeouts.**
- External subprocesses (gh, git, cargo) must have a timeout via `tokio::time::timeout`. Default: 60s; session-spawned Claude processes: no timeout but a health watchdog.

**Stream parsing.**
- Line-delimited JSON → `serde_json::from_str` per line, wrapped in `.context("parse stream line: {...}")`.
- Partial-line handling: assume a line is complete only when `lines()` yields it. Buffer in the reader, not in a `String` you manage yourself.

---

## 6. Serialization & contracts

**serde derives.**
- `#[derive(Serialize, Deserialize, Debug, Clone)]` is the default for data types.
- **Configs** (anything loaded from `maestro.toml`, state files, user input): add `#[serde(deny_unknown_fields)]`. Unknown fields are a bug, not a feature.
- `#[serde(rename_all = "snake_case")]` for enums that serialize to JSON/TOML — consistency with Rust identifiers.
- Optional fields: use `Option<T>` + `#[serde(default)]`, not manual `Default` fallbacks.

**API contracts.**
- Every external JSON payload (gh API, Claude stream-json) has a schema under `docs/api-contracts/`.
- `/validate-contracts` slash command (see `.claude/commands/validate-contracts.md`) checks model structs against those schemas. Run before touching serde types.

**Versioning.**
- State file formats carry a version field. Migrations are explicit, not implicit "try parse, fall back".

---

## 7. Testing discipline

**Layout.**
- **Unit tests** — `#[cfg(test)] mod tests` at the bottom of the same file. Test private helpers and public API of that module.
- **Integration tests** — `src/integration_tests/` (maestro's convention; note this is `src/integration_tests/`, not the `tests/` directory, so they share the crate's private modules).
- **Snapshot tests** — `src/tui/snapshot_tests/` with insta. CI enforces `INSTA_UPDATE=no` and greps for "pending snapshots" (`.github/workflows/ci.yml:24-26`).
- **Benches** — `benches/parser.rs`, `benches/turboquant.rs` with criterion. Run locally; not in CI.

**Async tests.** `#[tokio::test]` for async test fns. `#[tokio::test(flavor = "multi_thread")]` only when testing concurrency explicitly.

**Mocking.** Trait + mock impl. Template is `SessionForker` (`src/session/fork.rs:26`) and `GitHubClient` (`src/provider/github/client.rs`). Do not reach for `mockall` — hand-written fakes stay readable and type-safe.

**Real fakes over mocks.** A `MemoryStore` that really stores, really reads back, really returns the right errors is better than a mock that asserts call counts. Mocks are for APIs you don't own.

**Coverage goal.** Every `pub fn` in `session/`, `state/`, `gates/`, `turboquant/` has either (a) a direct unit test, (b) coverage via integration test, or (c) a comment noting why it's untested (e.g., thin wrapper around tested API). No mandate on percentage.

**`#[ignore]`.** Every `#[ignore]` has a comment linking an issue and a reason. Un-reviewed `#[ignore]` in a PR is a blocker.

---

## 8. Unsafe & FFI

**Policy: no new `unsafe` in the maestro crate without an ADR.**

Enforced via `unsafe_code = "deny"` in `Cargo.toml` `[lints.rust]`. We use `deny`, not `forbid`, because the crate has two narrowly scoped FFI call sites (`libc::kill` for SIGSTOP/SIGCONT in `src/session/manager.rs:204-234`) that are allow-listed at the call site with a `SAFETY:` comment. `forbid` would require factoring the FFI into a separate crate for marginal benefit — `deny` with explicit allow-listing gives the same auditability.

**Existing exceptions** (grep-friendly):
- `src/session/manager.rs` — `libc::kill()` for process pause/resume. The invariant: the PID originates from a process we spawned, the signals are side-effect-only, and the return value is intentionally ignored because a race (child already exited) is handled by the surrounding state machine.

**Adding a new `unsafe` block.** Requires:

1. An ADR under `docs/adr/` explaining (a) why safe Rust can't express this, (b) the invariants the block maintains, (c) how they're tested.
2. A `// SAFETY:` comment per the Rust convention (RBE §Unsafe Operations) describing preconditions/postconditions for the block.
3. An explicit `#[allow(unsafe_code)]` attribute on the smallest enclosing item.
4. Two reviewers.

Transitive `unsafe` in dependencies is unavoidable and outside this policy.

---

## 9. Dependencies & supply chain

**MSRV.** Pinned via `rust-toolchain.toml` to the minimum that compiles the current tree (currently 1.89 stable; bump with an ADR). CI uses `dtolnay/rust-toolchain@stable`; the local pin ensures contributors don't accidentally use newer features.

**Wildcard versions.** Forbidden. Always pin to a SemVer-compatible range (`"1"`, `"0.12"`). No `"*"`.

**Adding a dep.** Checklist:

1. Is there a stdlib alternative or a crate already in the tree that suffices? If yes, use that.
2. License is on the `deny.toml` allow list? If no, add it only if it's permissive (MIT/Apache-2.0/BSD/ISC).
3. Is the crate maintained (last release within ~18 months)? If no, find an alternative or justify.
4. Does adding it introduce a duplicate of an existing transitive dep? Resolve the dup or document the skip in `deny.toml`.

**Auditing.**
- `cargo deny check` — CI job. Starts non-blocking; flips to blocking after first clean run.
- `cargo audit` — CI job, advisory only.
- Local: `cargo outdated -R` for major-version drift reviews before releases.

---

## 10. Performance budget

**Startup.** `maestro` CLI cold start < 100ms (measured via `hyperfine`). Regression > 20% requires justification.

**Release profile.** Already set in `Cargo.toml`: `lto = "fat"`, `codegen-units = 1`, `strip = true`, `panic = "abort"`. Do not relax without a benchmark.

**Allocations.**
- Avoid `.clone()` where `&` works; let the compiler / clippy tell you.
- `String::with_capacity(n)` when `n` is known.
- Prefer iterators + `collect()` over explicit loops building `Vec`s.
- In hot paths (stream parsing, render): benchmark before and after any change that adds an allocation.

**Benchmarks.** `cargo bench --bench parser` and `cargo bench --bench turboquant`. Record baseline numbers in PR descriptions for changes touching these paths.

---

## 11. Observability

**Logging.**
- `tracing` crate only. `tracing-subscriber` configured in `main.rs`.
- `info!` for lifecycle events (session spawn, gate complete).
- `warn!` for recoverable anomalies (retry, fallback).
- `error!` for unrecoverable errors before return/propagation.
- `debug!` / `trace!` for developer-oriented detail; gated behind `RUST_LOG`.
- **Never `println!` / `eprintln!`** in library code. The CLI's user-facing output goes through the TUI or a dedicated printer. Lint: `clippy::print_stdout`, `clippy::print_stderr` set to `warn`.
- **Never `dbg!`** in committed code. Lint: `clippy::dbg_macro` set to `deny`.

**Structured fields.** Prefer `info!(session_id = %id, status = ?s, "spawned")` over `info!("spawned session {} with status {:?}", id, s)`. Fields survive JSON logging; interpolated strings do not.

---

## 12. Style & naming

**Rustfmt.** `cargo fmt --check` is a CI gate. Run `cargo fmt` before every commit. Don't argue with rustfmt — amend the config if needed, but no per-file overrides.

**Clippy.** `cargo clippy -- -D warnings -A dead_code` is a CI gate. `dead_code` is allowed crate-wide because maestro has several trait methods designed for future integration with a `// Reason: …` comment. This is the only global allow.

**Naming** (from Rust API Guidelines):
- **Modules / functions / vars:** `snake_case`.
- **Types / traits / enum variants:** `UpperCamelCase`.
- **Constants / statics:** `SCREAMING_SNAKE_CASE`.
- **Type parameters:** `T`, `U`, `E`, or `Self`-like (e.g., `F: FnOnce()`).
- **Abbreviations:** only well-known (`ctx`, `cfg`, `id`, `url`, `tmp`). Spell out the rest.

**Conversions (RBE §Conversion).**
- `From`/`Into` for infallible conversions.
- `TryFrom`/`TryInto` with a typed error for fallible conversions.
- `as` casts only for primitive → primitive; never `as` for pointer / type erasure.

**Derive order.** `#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]` — standard traits first, third-party after.

**Doc comments.**
- `///` on every `pub` item — at minimum a one-line summary.
- `//!` at the top of modules that have non-obvious responsibilities.
- `# Examples` section for public API with non-trivial usage.
- Write comments that explain **why**, not **what** (covered in CLAUDE.md, but restated here because the maestro codebase follows this).

**Tidy analog.** The rustc-dev-guide describes Tidy, the compiler's style enforcer. maestro's analog is `scripts/check-file-size.sh` + rustfmt + clippy + this doc.

---

## 13. CI gates (summary)

Current CI (`.github/workflows/ci.yml`):

1. **test** — `cargo test --verbose` + insta snapshot enforcement.
2. **clippy** — `cargo clippy -- -D warnings -A dead_code`.
3. **fmt** — `cargo fmt -- --check`.
4. **file-size** — `scripts/check-file-size.sh`.

Added by this guardrails bundle:

5. **deny** — `cargo deny check advisories bans licenses sources` (non-blocking first pass).
6. **audit** — `cargo audit` (advisory).

Runtime enforcement via `maestro.toml [sessions.completion_gates]` (fmt/clippy/test required) — orthogonal to CI; fires post-session.

---

## 14. PR review checklist

Reviewers (and the `subagent-architect` / `subagent-qa` flows) walk through this list:

1. Does the PR cite this doc (or a specific section) when introducing new module/policy territory?
2. New `unwrap()` / `expect()` / `panic!()` — justified inline with a `// Reason: …`?
3. New `unsafe` block — ADR linked? `SAFETY:` comment present?
4. New `Arc` / `Mutex` / `RwLock` — justified in a comment?
5. New async task — shutdown path documented?
6. New serde struct touching external data — contract schema present under `docs/api-contracts/`?
7. New `pub fn` in `session/` / `state/` / `gates/` — test or rationale comment present?
8. New dependency — license on allow list? wildcard-free? maintained?
9. File-size cap respected? Function under 80 LOC?
10. `tracing` (not `println!` / `dbg!`) for diagnostic output?
11. `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test` all green?
12. Snapshot changes reviewed with `cargo insta review` and intentional?

---

## 15. Pointers to exemplar code

Don't re-derive patterns — read these and follow their shape:

| Pattern | File | Lines |
|---|---|---|
| Context-wrapped fs + file locking + atomic write | `src/state/store.rs` | 19-67 |
| Typed error at module seam | `src/session/transition.rs` | 37-55 |
| Async line-oriented stream parsing | `src/session/parser.rs` | top |
| tokio::process::Command lifecycle | `src/session/manager.rs` | 71+ |
| Trait + mock for testability | `src/session/fork.rs` | 26-34 |
| Session state machine transitions | `src/session/types.rs` | (state machine) |
| Insta snapshot testing | `src/tui/snapshot_tests/` | — |
| Completion gates runner | `src/gates/runner.rs` | — |
| Criterion benchmarks | `benches/parser.rs`, `benches/turboquant.rs` | — |

---

## Amendment history

| Date | Change | Author |
|------|--------|--------|
| 2026-04-20 | Initial guardrails document | feat/rust-development-guardrails |
