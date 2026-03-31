# Maestro Roadmap

> Single source of truth for project milestones and implementation order.
> See [CHANGELOG.md](CHANGELOG.md) for detailed release notes.

---

## Milestone: v0.1.0 — Foundation + Core Features ✅

**Status:** Released (2026-03-24)

All core orchestration features from Phases 0–4 of the PRD.

| Phase | Description | Issue | PR | Status |
|-------|-------------|-------|----|--------|
| Phase 0 | Foundation — single-session TUI, parser, state | — | `ebfb959` | ✅ Done |
| Phase 1 | Multi-session pool, split-pane TUI, worktrees, file claims | [#2] | [#6] | ✅ Done |
| Phase 2 | GitHub integration — issues, PRs, labels, dependencies | [#3] | [#7] | ✅ Done |
| Phase 3 | Intelligence — budget, stall detection, retry, gates | [#4] | [#8] | ✅ Done |
| Phase 4 | Plugins, modes, TUI polish, resume, completions | [#5] | [#11] | ✅ Done |

---

## Milestone: v0.2.0 — Quality & Hardening 🔧

**Status:** In Progress

Hardening, missing PRD features, test coverage, and distribution.

### Missing PRD Features

| Issue | Title | Priority | Status |
|-------|-------|----------|--------|
| [#12] | Context overflow detection and auto-fork | P1 | Done |
| [#13] | Real-time conflict detection via stream parsing | P1 | Planned |
| [#14] | Slack webhook integration for notifications | P2 | Planned |

### Testing & Quality

| Issue | Title | Priority |
|-------|-------|----------|
| [#15] | Integration test suite for end-to-end session lifecycle | P1 |
| [#16] | TUI rendering snapshot tests | P2 |
| [#19] | Benchmark session parser throughput | P2 |

### Infrastructure & Docs

| Issue | Title | Priority |
|-------|-------|----------|
| [#17] | Release workflow for binary builds and distribution | P2 |
| [#18] | Man page and shell completion installation guide | P2 |

---

## Milestone: v0.3.0 — Multi-Project Task Management 🌐

**Status:** Planned

Extend Maestro from single-project to multi-project orchestration (Phase 5).

**Tracking issue:** [#9]

| Issue | Title | Blocked By | Priority |
|-------|-------|------------|----------|
| [#20] | Workspace configuration with multi-project maestro.toml | — | P1 |
| [#21] | Cross-project session orchestration | [#20] | P1 |
| [#22] | Multi-project TUI dashboard | [#21] | P1 |
| [#23] | CLI extensions for multi-project management | [#20] | P2 |
| [#24] | Cross-project notifications and event routing | [#21] | P2 |

---

## Implementation Order

```
v0.1.0 ✅ Complete
  └── Phases 0-4 (all merged)

v0.2.0 🔧 In Progress
  ├── #12 Context auto-fork ─────────✅ Done
  ├── #13 Conflict detection ────────┐ PRD gaps (parallel)
  ├── #14 Slack notifications ───────┘ 
  ├── #15 Integration tests ─────────┐
  ├── #16 TUI snapshot tests ────────┤ Quality (parallel)
  ├── #19 Parser benchmarks ─────────┘
  ├── #17 Release workflow
  └── #18 Completion docs

v0.3.0 🌐 Planned
  ├── #20 Workspace config ──────────┐
  │   ├── #21 Cross-project orch. ───┤ Sequential
  │   │   ├── #22 Multi-project TUI  │
  │   │   └── #24 Cross-proj notifs  │
  │   └── #23 CLI extensions ────────┘
  └── (all depend on #20 workspace config)
```

---

[#2]: https://github.com/CarlosDanielDev/maestro/issues/2
[#3]: https://github.com/CarlosDanielDev/maestro/issues/3
[#4]: https://github.com/CarlosDanielDev/maestro/issues/4
[#5]: https://github.com/CarlosDanielDev/maestro/issues/5
[#6]: https://github.com/CarlosDanielDev/maestro/pull/6
[#7]: https://github.com/CarlosDanielDev/maestro/pull/7
[#8]: https://github.com/CarlosDanielDev/maestro/pull/8
[#9]: https://github.com/CarlosDanielDev/maestro/issues/9
[#11]: https://github.com/CarlosDanielDev/maestro/pull/11
[#12]: https://github.com/CarlosDanielDev/maestro/issues/12
[#13]: https://github.com/CarlosDanielDev/maestro/issues/13
[#14]: https://github.com/CarlosDanielDev/maestro/issues/14
[#15]: https://github.com/CarlosDanielDev/maestro/issues/15
[#16]: https://github.com/CarlosDanielDev/maestro/issues/16
[#17]: https://github.com/CarlosDanielDev/maestro/issues/17
[#18]: https://github.com/CarlosDanielDev/maestro/issues/18
[#19]: https://github.com/CarlosDanielDev/maestro/issues/19
[#20]: https://github.com/CarlosDanielDev/maestro/issues/20
[#21]: https://github.com/CarlosDanielDev/maestro/issues/21
[#22]: https://github.com/CarlosDanielDev/maestro/issues/22
[#23]: https://github.com/CarlosDanielDev/maestro/issues/23
[#24]: https://github.com/CarlosDanielDev/maestro/issues/24
