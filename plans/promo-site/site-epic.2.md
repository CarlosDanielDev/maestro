Parent: #1059

The hero section of the promo page needs motion: a static SVG mock (`README.md:14` → `docs/assets/readme-hero.svg`) cannot show what maestro feels like — sessions launching, the dashboard updating, an interaction turn. The capture must come from the real binary, scripted and reproducible, embedded in the `gh-pages` page built by #1060. The capture drives the real binary whose TUI loop lives in `src/tui/mod.rs` `run`.

## Done when

- [ ] a scripted terminal capture of the real `maestro` TUI exists (launch → dashboard with ≥ 2 sessions → an interaction turn), produced by a script checked into the repo (`docs/assets/capture.sh` or similar) so it can be re-run when the UI changes — owner: implementer
- [ ] the capture is encoded as GIF or WebM under 2 MB and committed to the `gh-pages` branch — `git ls-tree -r gh-pages --name-only` shows it; `git cat-file -s` confirms the size — owner: implementer
- [ ] the hero section of `index.html` embeds it (`<img>` for GIF, `<video autoplay muted loop>` for WebM) with the static hero SVG kept as fallback — `curl -s <page-url> | grep -c "<video\|hero"` ≥ 1 — owner: implementer
- [ ] the capture script's tooling is named in the script header (e.g. `vhs`, `asciinema` + `agg`) with install instructions, and the script runs headless (no human keystrokes) — owner: implementer

## ADR stub

Capture tooling: (a) `vhs` (charmbracelet — scripted .tape → GIF/WebM, headless, one binary); (b) `asciinema` + `agg` (two tools, GIF only); (c) manual screen recording (rejected — not reproducible, violates the script requirement). Recommendation (to argue with): (a). Sessions in the capture run against the stub agent pattern from the E2E harness (#1049) so the demo is deterministic and costs nothing — real providers not needed for motion.

## Out of scope

- Copy/text content of the page (#1062).
- Sound, cursor-effects libraries, JS players.
- Capturing provider API traffic or real credentials on screen — the stub keeps the demo clean.

## Handoff

```bash
git worktree add -b feat/1061-demo-capture ../maestro-1061 origin/main
```

Read `docs/assets/readme-hero.svg`, the E2E stub pattern from #1049 (`tests/fixtures/` if merged), and the `gh-pages` index.html from #1060. Gate: asset size check + `curl` checks on the live page after push to `gh-pages`. No AI attribution; none of the forbidden git commands. `Closes 1061`.
