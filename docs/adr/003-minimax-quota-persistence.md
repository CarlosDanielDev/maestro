# ADR 003 — MiniMax Quota Persistence: fs2, ~/.maestro, Sliding Window, Schema v1

- **Status:** Accepted
- **Date:** 2026-05-21
- **Tracking issue:** [#775](https://github.com/CarlosDanielDev/maestro/issues/775)
- **Implementation:** `src/agent_provider/minimax/quota.rs`

---

## Problem

MiniMax's free tier allows 4,500 requests per 5-hour rolling window. Two parallel
`maestro` processes can double-spend the window without coordination. The quota
state must survive process restarts and be readable by any process without a
daemon or server.

---

## Decision Points

### A — File-lock library: `fs2` vs `fslock` vs `std::fs::File::lock`

| Option | Pro | Con | Decision |
|--------|-----|-----|----------|
| `fs2 = "0.4"` | Stable, widely used, `FileExt` trait maps cleanly to `lock_exclusive` / `unlock` | Adds one dependency | **Chosen** |
| `fslock` | Zero-dependency pure-Rust | API mismatch: `LockFile` acquires on `open`, no explicit unlock; harder to express "try-lock then fall back" | Rejected |
| `std::fs::File::lock` (stabilized in Rust 1.75) | No extra dep | `File::lock` is blocking-only in 1.75; non-blocking `try_lock` landed in 1.80, which is above MSRV 1.89 — wait, MSRV 1.89 is above 1.80, so `try_lock` is available. However, `std::fs::File::lock` is not `async`-safe and requires the same `spawn_blocking` wrapping as `fs2`. `fs2` was already present in the transitive graph via another crate, so the marginal cost is zero. | Rejected (fs2 preferred for clarity; std option is viable if fs2 is ever removed) |

**Outcome:** `fs2 = "0.4"` added to `[dependencies]` in `Cargo.toml`.

### B — State file location: `~/.maestro/minimax-quota.json`

The state must be shared across maestro invocations from any working directory,
so a per-project path is wrong. Options:

| Option | Notes |
|--------|-------|
| `~/.maestro/minimax-quota.json` | Consistent with maestro's existing `~/.maestro/` convention for cross-project persistent state | **Chosen** |
| `$XDG_STATE_HOME/maestro/minimax-quota.json` | Correct on Linux; `$XDG_STATE_HOME` is not defined on macOS or Windows by default | Deferred (can be added as a follow-up when XDG support is formalized) |
| Per-project `.maestro/minimax-quota.json` | Wrong: the quota is per-account, not per-project | Rejected |

### C — Window algorithm: sliding window vs fixed counter

| Option | Accuracy | Burst Risk | Complexity |
|--------|----------|------------|------------|
| **Sliding window** (store per-request timestamps, prune on read) | Exact: `count(timestamps where ts >= now - 5h)` | No: a burst at minute 0 does not block minute 1 of the next cycle | `O(n)` prune on each check; `n` ≤ 4,500 so acceptable | **Chosen** |
| Fixed 5-hour bucket | Simple: one counter + reset timestamp | Yes: 4,499 requests at t=4:59 leaves only 1 slot for the next hour | Simpler but wrong for the use case | Rejected |

The `VecDeque<DateTime<Utc>>` representation keeps timestamps in insertion order,
making the prune step a single `pop_front` loop without sorting.

### D — Schema versioning: `schema_version: 1` + `deny_unknown_fields`

The file is deserialised with `#[serde(deny_unknown_fields)]` on `QuotaState`.
This means:

- A future schema change bumps `schema_version` to 2 and writes a new struct;
  the old code surfaces `UnsupportedSchemaVersion` and refuses to silently
  corrupt the counter.
- Unknown fields from a newer maestro do not silently accumulate in an older
  binary — the older binary returns an error early and the user must upgrade.

The cost: a future reader cannot remain forward-compatible unless it explicitly
handles `schema_version` before deserialising. Accepted — correctness over
forward-compat for a file whose consumer is always the same binary.

---

## Consequences

- `fs2 = "0.4"` is a new direct dependency. License: MIT. No transitive deps.
- `~/.maestro/` is created with `fs::create_dir_all` on first run.
- The quota file is written with `0o600` mode (owner-read/write only) on Unix
  via `OpenOptions::new().mode(0o600)`.
- Atomic write uses a `NamedTempFile` in the same directory, then `persist()`
  (an atomic rename). Cross-filesystem renames are not attempted.
- Warn threshold: 80% of limit. Refuse threshold: 95%. Both are module-level
  constants (`WARN_PCT`, `REFUSE_PCT`).
- `--force-quota` CLI flag bypasses the refuse gate for one invocation.
- The `Clock` trait is injected so tests can drive the 5-hour window without
  sleeping.

---

## Follow-up

- If `$XDG_STATE_HOME` support is added project-wide, migrate the quota path
  lookup to the shared XDG helper (issue to be filed).
- When MiniMax publishes paid-tier pricing, replace the `0.0` stub in
  `minimax/pricing.rs` with real rates (no structural change needed).
