Parent: #{{herdr.1.n}}

The owner flagged herdr (github.com/herdrdev/herdr — Rust, Apache-2.0, v0.4.0, 44.3k stars) as "a ideia é muito boa": a client-server terminal runtime built for AI agents — panes marked `working / blocked / idle`, agents drive it through a CLI + socket API (spawn panes, prompt each other, wait until a peer is genuinely blocked), detach/reattach that survives dead SSH, layout restore. maestro owns the *work* (pool, worktrees, gates, costs) but not the *terminal runtime*: the TUI dies with the SSH session, stuck-agents are inferred from the state machine (`SessionStatus::Stalled`, `src/session/types.rs:25`), and agents cannot drive maestro programmatically. The owner wants a bench before deciding what — if anything — gets added.

## Por que não é story

No product behavior changes; the gain is a measured decision — integrate, coexist, or skip — backed by a scripted comparison instead of taste.

## Done when

- [ ] a written feature-comparison matrix exists (herdr vs maestro) covering: status signals (working/blocked/idle vs `SessionStatus` machine), detach/reattach durability, agent-driven control (socket API vs `maestro` CLI + TUI), worktree isolation, multi-provider spawning, cost tracking, remote/SSH — each cell cites herdr docs (herdr.dev) or maestro `file:line` — owner: implementer
- [ ] a scripted bench ran ≥ 3 scenarios against BOTH tools on this machine: (1) detach mid-session and reattach (what survives? agent process? layout? transcript?), (2) stuck-agent detection (kill an agent's stdin mid-turn — who notices first and what signal?), (3) programmatic control (spawn + prompt + collect via herdr socket API vs maestro CLI/TUI) — script checked into `docs/` or `benches/`, raw outputs in the PR — owner: implementer
- [ ] the integration ADR is written and decided: (a) maestro sessions run inside herdr panes (maestro as orchestrator, herdr as terminal layer — the `[agents.x] command` + socket-API bridge), (b) maestro adopts the mechanisms (working/blocked/idle attention signals, detach durability) natively, (c) coexist with a doc, or (d) skip — with the bench data as the argument, owner's call recorded — owner: reviewer
- [ ] if (a) or (b): a follow-up epic is drafted in `plans/` with the integration slices; if (c)/(d): the matrix itself is the deliverable — owner: implementer

## ADR stub

The two tools are complements, not competitors: herdr is "the runtime your coding agents live on" (their words), maestro is the work orchestrator. The live question is the seam: does maestro become herdr-aware (detect herdr, drive panes via its socket API, map pane status → session state) or terminal-agnostic? Recommendation (to argue with): run the bench first — the detach scenario decides it; if herdr keeps agent processes alive where maestro loses them, (a) is the cheap win; if the state machine already covers detection, (b)'s attention signals are the only steal. Note: maestro's `run --once` TUI short-circuit and the PTY transport (#1053) change what "detach" even means per transport — the bench must test both.

## Out of scope

- Replacing maestro's TUI with herdr panes.
- Any production integration code — this spike only measures and decides.
- herdr plugin development (their marketplace) — a follow-up if the ADR says (a).

## Handoff

```bash
git worktree add -b spike/{{herdr.1.n}}-herdr-bench ../maestro-{{herdr.1.n}} origin/main
```

Read herdr.dev (quick start, concepts, socket API) and `src/session/types.rs` state machine. Install herdr via their install script in an isolated prefix (never `| sh` into the repo — use a temp dir). Bench runs use the stub-agent pattern from #1049 (deterministic, no provider cost). Gate: no Rust changes expected; the deliverables are the matrix, the script, and the ADR. No AI attribution; none of the forbidden git commands. `Closes {{herdr.1.n}}`.
