# Rust Guardrails — Sidecar Quick Reference

> Distilled policy pointers for planning and review. For the full document, read `docs/RUST-GUARDRAILS.md` at the repo root.

## When a subagent should consult this

- `subagent-architect` — before proposing a new module, dependency, or cross-module refactor. Cite the specific guardrails section (§1 layout, §2 errors, §3 async, §5 subprocess, §7 testing, §9 deps, §11 observability) in the blueprint.
- `subagent-qa` — when designing the test blueprint. Cite §7 testing discipline; prefer real fakes over mocks; reuse trait-based mocking patterns.
- `subagent-security-analyst` — when reviewing unsafe blocks, subprocess handling, or deserialization from external input. Cite §8 unsafe and §6 serialization.

## The 8 principles — one-line each

| # | Principle | Enforcement |
|---|---|---|
| 1 | Safe by default — zero new `unsafe` without ADR | `unsafe_code = "deny"` in Cargo.toml |
| 2 | Errors are values; panics are bugs | `expect_used = "warn"`; review checklist |
| 3 | Async hygiene — no blocking, bounded channels, shutdown paths | Review checklist; `spawn_blocking` pattern |
| 4 | Ownership over aliasing — minimize Arc/Mutex | Review checklist; justified comment on Arc |
| 5 | Tests as specification — TDD, trait fakes, insta | CI test job + INSTA_UPDATE=no gate |
| 6 | Supply-chain caution — MSRV pinned, deny wildcards | `cargo deny` CI job; `rust-version` in Cargo.toml |
| 7 | Readability — rustfmt + clippy non-negotiable | CI fmt + clippy jobs + file-size lint |
| 8 | Observability — `tracing` not `println!` / `dbg!` | `dbg_macro = "deny"`; review checklist |

## Pattern exemplars (cite these, don't re-derive)

| Pattern | File | Notes |
|---|---|---|
| Context-wrapped fs + file locking + atomic write | `src/state/store.rs:19-67` | `.with_context(\|\| format!(...))` on every I/O op |
| Typed error at module seam | `src/session/transition.rs:37-55` | Hand-rolled `Display + Error` impls |
| Async line-oriented stream parsing | `src/session/parser.rs` | `AsyncBufReadExt::lines()` + serde_json per line |
| tokio::process::Command lifecycle | `src/session/manager.rs:71+` | Explicit `Stdio::piped()`, owned `Child`, kill-on-drop |
| libc::kill FFI (allow-listed unsafe) | `src/session/manager.rs:204-234` | Template for `#[allow(unsafe_code)]` + `SAFETY:` comment |
| Trait + mock for testability | `src/session/fork.rs:26-34` | `SessionForker` trait; mocks injected in tests |
| Mock GitHub client (real fake) | `src/provider/github/client.rs` | `Arc<Mutex<MockState>>` only in test impl |
| Insta snapshot testing | `src/tui/snapshot_tests/` | CI enforces `INSTA_UPDATE=no` |
| Completion gates runner | `src/gates/runner.rs` | Post-session quality checks |
| Integration tests in src/ | `src/integration_tests/*` | NOT `tests/` — shares crate-private modules |
| Criterion benchmarks | `benches/parser.rs`, `benches/turboquant.rs` | Not in CI; run locally for perf-sensitive changes |

## Fast-path review checklist (for blueprints)

When planning any new feature or refactor, the blueprint should answer:

1. Does this introduce any `.unwrap()` / `.expect()` / `panic!()`? If yes, justify inline.
2. Does this introduce any new `unsafe`? If yes, ADR required.
3. Does this spawn any new tokio tasks? If yes, describe the shutdown path.
4. Does this add any `Arc` / `Mutex` / `RwLock`? If yes, justify (comment or channel-based alternative considered).
5. Does this touch serde types loaded from external data? If yes, contract schema in `docs/api-contracts/`.
6. Does this add any `pub fn` in `session/` / `state/` / `gates/`? If yes, test or rationale comment.
7. Does this add a dependency? If yes, run the Adding-a-dep checklist (guardrails §9).
8. Does this push a file or function past the size caps? If yes, propose a split first.
9. Does this use `tracing` (not `println!` / `dbg!`) for diagnostic output?

## When the guardrails don't have an answer

Pre-amendment: use judgment, pattern-match to the closest exemplar, and note the gap in the PR description.

Post-amendment: open a PR editing `docs/RUST-GUARDRAILS.md`. Amendments to the guardrails doc require at least one reviewer who is familiar with the affected subsystem.
