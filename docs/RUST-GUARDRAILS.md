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
7. **Readability is a feature.** rustfmt non-negotiable. clippy `-D warnings` non-negotiable. File ≤ 400 LOC hard cap (enforced by `scripts/check-file-size.sh`; soft target 300 LOC). Function ≤ 80 LOC soft cap.
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

**File size.** Hard cap 400 LOC; soft target 300 LOC. Enforced by `scripts/check-file-size.sh` (CI job `file-size`). Files approaching the cap (`config.rs`, `cli.rs`, the TUI screens) are on `scripts/allowlist-large-files.txt` with deadlines — extensions require a PR with justification; reviewers push back on habitual extensions.

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

5. **deny** — `cargo deny check advisories bans licenses sources`. Ignore list for transitive-only unmaintained advisories (bincode via syntect, paste via ratatui/tui-textarea, yaml-rust via syntect) is documented in `deny.toml` with RUSTSEC links.
6. **audit** — `cargo audit` (RustSec advisory scan, same ignore list as deny).

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

## CI Quality Gates (Wave 1)

**Status:** active after PR for Chunk 1 of `docs/superpowers/plans/2026-04-22-ci-quality-gates-plan.md` lands.

**Cognitive complexity:** `clippy.toml` sets `cognitive-complexity-threshold = 20` (tightened from 25). Functions exceeding this fail clippy under `-D warnings`. Escape hatch is a targeted `#[allow(clippy::cognitive_complexity)]` with a `// Reason: <short>` comment immediately above the function. Crate-level `#![allow]` is deliberately avoided — every exception is local and reviewable.

**cargo-deny strict mode:** `deny.toml` has `multiple-versions = "deny"` (tightened from `"warn"`). The `skip` list documents every unresolvable transitive duplicate (ratatui/syntect/reqwest et al.) with `reason` + upstream tracking where relevant. Goal is an empty `skip` list; reality is pruning it as upstream releases land.

**Curated clippy nursery lints** (enabled via `#![warn(...)]` in `src/lib.rs` and `src/main.rs`):

- `clippy::needless_pass_by_ref_mut` — API hygiene.
- `clippy::redundant_clone` — ownership clarity.
- `clippy::significant_drop_tightening` — matches async policy (no long-held locks across await points).
- `clippy::fallible_impl_from` — matches §2 error policy (panics-as-bugs). Highest-signal lint in the subset.
- `clippy::path_buf_push_overwrite` — catches `PathBuf::push("/abs")` footgun.
- `clippy::branches_sharing_code` — refactoring signal.

**Evaluated and rejected** (deliberate exclusions):

- `clippy::missing_const_for_fn` — produced 213 violations on first enable during Wave 1 implementation. Dropped per this document's philosophy of avoiding aspirational lints that add friction without ROI. 15 genuinely-correct `const fn` promotions found during evaluation were kept.
- `clippy::missing_docs_in_private_items` — noisy for a binary crate.
- `clippy::use_self` — style opinion, not a bug catcher.
- `clippy::option_if_let_else` — variable signal.
- `clippy::suboptimal_flops` — codebase has almost no floating-point math.

**File-size allowlist format.**

`scripts/allowlist-large-files.txt` uses the deadlined format:

```
<path> # deadline: YYYY-MM-DD, owner: @handle, ticket: #N, plan: <brief>
```

CI fails when any deadline is in the past. Extensions require a PR bumping the deadline + a paragraph explaining why the refactor wasn't done. Repeated extensions on the same file are a signal to pick a different split strategy, not to extend again. Removing the gate is an ADR-level change.

During Wave 1 implementation, `scripts/check-file-size.sh` was found to have a pre-existing silent-pass bug: its `wc -l` output parsing left `lines` as an empty string, which made the `(( lines > MAX_LINES ))` comparison evaluate to `0 > 500 → false` for every file. The 500 LOC cap had been cosmetic since its introduction. Fixed in the same PR; 23 previously-invisible violations surfaced and were pre-added to the allowlist with the same 14-day placeholder deadline.

**Pre-flight hook.**

`.claude/hooks/preflight.sh` runs fast per-PR gates locally (fmt + clippy + file-size) so `/implement` catches regressions before a branch is created.

---

## CI Quality Gates (Wave 2.1 — Coverage)

**Status:** reporting-only during baseline phase. Per-tier floors activate when baseline reaches the respective floor.

**Tool:** `cargo-llvm-cov`. Tier manifest: `scripts/coverage-tiers.yml`. Enforcement: `scripts/check-coverage-tiers.sh`. CI job: `coverage` (runs per-PR, `continue-on-error: true` during baseline).

**Tier floors:**

| Tier | Paths | Floor | Aspiration |
|------|-------|-------|------------|
| core | `session/**`, `state/**`, `adapt/**`, `turboquant/**`, `gates/**`, `provider/**`, `config.rs`, `cli.rs` | 90.0% | 96.0% |
| tui | `src/tui/**` | 70.0% | — |
| excluded | `main.rs`, `lib.rs`, `integration_tests/**`, `*_test.rs`, `tests.rs` | — | — |

**Baseline measurement (2026-04-22, PR #431):**
- core: 87.6% (floor: 90.0%) — below by 2.4 pp
- tui: 67.4% (floor: 70.0%) — below by 2.6 pp

Both tiers are within striking distance of their floors; activation is near-term test-writing, not a multi-week project. Suggested sequence: add tests for the largest uncovered modules in each tier, rerun `coverage` to see the delta, repeat until ≥ floor, then open a follow-up PR that adds `--enforce` to the coverage script invocation for that tier.

---

## CI Quality Gates (Wave 2.2 — File Size 500 → 400)

**Status:** active (landed 2026-04-22 via chunk-3/file-size-400).

**Hard cap:** 400 LOC per `.rs` file under `src/`. Enforced by `scripts/check-file-size.sh`. Soft target is 300 LOC — anything approaching 400 is a review signal to split before adding responsibilities.

**Allowlist growth:** the cap flip added 16 files that were previously in the 400-500 band. Combined with Wave 1's 43 entries, the allowlist has 59 entries after Wave 2.2. Every entry has a deadline, owner, and plan field. Deadline tranches:

- **1 month** (2026-05-22): 4 outlier files (>1500 LOC).
- **2 months** (2026-06-22): 34 core modules (20 from Wave 1 + 14 from Wave 2.2 — note 1 of the 16 new entries is a test file).
- **3 months** (2026-07-22): 23 TUI files (18 from Wave 1 + 5 from Wave 2.2).
- **4 months** (2026-08-22): 3 test files (2 from Wave 1 + 1 new).

**Extension policy:** unchanged from Wave 1. PR with paragraph justifying the new deadline; reviewer rejects habitual extensions in favor of re-planning the split.

---

## CI Quality Gates (Wave 2.3 — Layer Violations)

**Status:** active (landed 2026-04-22 via chunk-4/layer-violations).

**Manifest:** `scripts/architecture-layers.yml`. Enforcement: `scripts/check-layers.sh` (CI job `layers`).

**Layers** (lowest to highest — a file in layer N may import from layer N or any lower layer; imports from higher layers are forbidden):

1. **primitives** — `src/icons.rs`, `src/icon_mode.rs`, `src/util/**`
2. **domain** — `src/session/**`, `src/state/**`, `src/adapt/**`, `src/turboquant/**`, `src/provider/**`, `src/gates/**`, `src/sanitize/**`, `src/work/**`, `src/config.rs`
3. **orchestration** — `src/cli.rs`, `src/commands/**`
4. **ui** — `src/tui/**`
5. **entry** — `src/main.rs`, `src/lib.rs`

**Forbidden pairs** catch specific disallowed imports independently of layer-number check: `session/**`, `state/**`, `provider/**`, and `config.rs` may not import from `tui/**`.

**Debt file:** `docs/layers-debt.txt`. Same deadlined-tolerance pattern as `scripts/allowlist-large-files.txt` but for (importer, target) pairs instead of individual files. Format:

```
<importer> → <target> # deadline: YYYY-MM-DD, owner: @handle, ticket: #N, plan: <brief>
```

CI fails on new violations not in the debt file, or on debt entries whose deadlines have passed (surfaced as `VIOLATION (DEADLINE PAST)` or `FORBIDDEN (DEADLINE PAST)`).

**Baseline (2026-04-22):** 6 unique (importer, target) pairs covering 9 import sites.

- 1 pair: `config.rs → tui/theme` (2026-05-22) — resolution tied to config.rs refactor.
- 2 pairs: `sanitize/screen.rs → tui/*` (2026-06-22) — tied to sanitize/screen.rs split.
- 3 pairs: `commands/{run,dashboard,setup}.rs → tui/app/*` (2026-07-22) — dependency inversion.

**Script v1 limitations (documented):** brace-group imports (`use crate::mod::{A, B};`) are silently skipped; `pub use` re-exports are treated like plain `use`. Addressable with a future Rust-parser upgrade; acceptable for v1 per the spec's "simple enough not to need a full parser" rationale.

---

## CI Quality Gates (Wave 3 — Nightly Heavyweight)

**Status:** infrastructure landed 2026-04-22 via chunk-5/wave-3-nightly. Branch protection activation deferred until after warmup (2 weeks mutation, 1 week miri).

**Scheduled workflows:**

- **`.github/workflows/nightly.yml`** — runs daily at 03:00 UTC.
  - `mutation` (4 shards, `--baseline=skip`): cargo-mutants on the core tier (same exclude_globs as the coverage core tier). Target ≥ 80% mutation score after warmup.
  - `miri`: `cargo miri test --package maestro --test '*'` on the integration test surface. Pass/fail threshold. Tests that spawn subprocesses or hit FFI need `#[cfg_attr(miri, ignore)]` annotations — added incrementally as miri surfaces failures during warmup.

- **`.github/workflows/weekly.yml`** — runs Sundays 03:00 UTC.
  - `tsan`: ThreadSanitizer (`RUSTFLAGS="-Zsanitizer=thread"`) on async tests. **Informational only** — posts results to a pinned GitHub issue (`tsan-weekly` label). Does NOT block main. Rationale for informational: tsan has known false positives on tokio internals; maintaining a suppressions list isn't worth the cost for the signal weekly reports already provide.

**Branch protection (activation pending):**

- `nightly-freshness` status check required. Provided by the freshness bot at `.github/actions/freshness/` (Node 20 GitHub Action, stdlib only — no npm deps). Fails when the most recent scheduled nightly on main succeeded > 3 days ago (or failed, or didn't run). Workflow: `.github/workflows/freshness.yml` runs on every PR to main.

**Warmup:** nightly workflow lands and reports for 2 weeks before branch protection activates for mutation; 1 week for miri. During warmup, iterate on timeouts / exclude-globs / `#[cfg_attr(miri, ignore)]` annotations.

**Activation procedure:** once warmup is clean, edit the branch protection rule (Settings → Branches → main) to add the following required status checks:
- `Nightly / mutation (shard 0)` through `Nightly / mutation (shard 3)`
- `Nightly / miri`
- `nightly-freshness`

**Rollback:** if nightly regressions produce merge-blocking friction, remove those checks from the required-status list. Nightly still runs (catches regressions); doesn't block. Investigate, then re-activate.

**Smoke check:** `docs/ci-smoke-check.md` documents 10 scenarios for manual verification before tagging any release that modifies CI infrastructure.

**Activation policy:** the `check-coverage-tiers.sh` script runs in **report mode by default** — it prints tier percentages and any VIOLATION lines but exits 0 so the CI check stays green while baseline is below floor. To activate enforcement, add `--enforce` to the script invocation in the `coverage` job (`.github/workflows/ci.yml`). Once baseline reaches a floor for a tier, a dedicated PR adds `--enforce` for that tier's first blocking run. (Per-tier activation can be modeled by running the checker twice with different manifests pointing at a subset of tiers — simplest evolution when we get there.)

**Ratchet:** deferred until after floor activation. Enabling ratchet during baseline phase would block every PR that doesn't add tests, including refactors and documentation changes.

**Local measurement prerequisite:** `cargo-llvm-cov` requires the `llvm-tools-preview` component, installed via `rustup component add llvm-tools-preview`. Machines without `rustup` (brew-installed Rust, Nix-installed Rust) can't run coverage locally; use CI artifacts instead.

---

## Amendment history

| Date | Change | Author |
|------|--------|--------|
| 2026-04-20 | Initial guardrails document | feat/rust-development-guardrails |
| 2026-04-22 | Appended CI Quality Gates (Wave 1) | chunk-1/ci-wave-1 |
| 2026-04-22 | Appended CI Quality Gates (Wave 2.1 — Coverage) | chunk-2/coverage-infrastructure |
| 2026-04-22 | File-size hard cap tightened 500 → 400 (Wave 2.2) | chunk-3/file-size-400 |
| 2026-04-22 | Appended CI Quality Gates (Wave 2.3 — Layer Violations) | chunk-4/layer-violations |
| 2026-04-22 | Appended CI Quality Gates (Wave 3 — Nightly Heavyweight) | chunk-5/wave-3-nightly |
