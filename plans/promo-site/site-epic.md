Parent: #1059

maestro is public but has no landing page: `homepageUrl` is empty (`gh repo view --json homepageUrl`), and the README hero is an SVG mock (`README.md:14`). A GitHub Pages site under `carlosdanieldev.github.io/maestro/` gives the project a shareable URL for the README, Discord, and release notes. Council of one (owner delegated, 2026-10-15): leanest shape — orphan `gh-pages` branch, single static HTML file, no framework, no build step. The install command the page teaches is the crate named at `Cargo.toml:1`.

Children:

- #1060 — the scaffold: orphan branch, page live.
- #1061 — the hero: real TUI capture embedded.
- #1062 — the words: features + quickstart + repo wiring.

## Done when

- [ ] all three children closed, each by a PR whose ledger marks every Done-when item — owner: reviewer

## Handoff

Parents are never dispatched — the leaves carry the handoffs. Closes when children close (`wave close-parents`).

```bash
git worktree add -b epic/1059-promo-site ../maestro-1059 origin/main
```
