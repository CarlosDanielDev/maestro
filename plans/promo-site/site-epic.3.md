Parent: #1059

The scaffold page (#1060) has a tagline; features live in `README.md:20` onward and an install command; the demo (#1061) gives it motion. This slice fills the words — features distilled from the README, a quickstart validated against the real binary — and wires the URL back into the repo so people can find it.

## Done when

- [ ] the page carries a features section (≤ 6 items, plain HTML list) distilled from the README's own feature headings — each item traceable to a README line, no invented claims — owner: implementer
- [ ] a quickstart section with commands mirroring `src/cli.rs` `Commands`, copied from the README and **run for real** before publishing (`cargo install --git …` or the documented path in a temp dir; paste the transcript in the PR) — owner: implementer
- [ ] links section: repo, releases, wiki, Discord, CHANGELOG — every link `curl`-checked 200/301→200 in the PR evidence — owner: implementer
- [ ] repo wiring: `homepageUrl` set to the page (`gh api repos/CarlosDanielDev/maestro -X PATCH -f homepage=…`), and the README top block gains one line linking the page — owner: implementer
- [ ] OpenGraph meta tags (title, description, url, image = the hero asset) present — `curl -s <url> | grep -c "og:"` ≥ 4 — owner: implementer

## ADR stub

Feature list source: (a) hand-distill from README headings (six strongest, plain words); (b) auto-generate from docs (rejected — a script to maintain for a static list); (c) marketing copy written fresh (rejected — invented claims drift from reality, the README is the truth). Recommendation (to argue with): (a). Quickstart commands are executed, not assumed — the page must not teach a broken command.

## Out of scope

- Blog/changelog feed, i18n, custom domain, analytics.
- Wiki content changes (the page links to it, never mirrors it).
- Crates.io publishing (referenced only if it exists at implementation time).

## Handoff

```bash
git worktree add -b docs/1062-content-pass ../maestro-1062 origin/main
```

Read `README.md` (features + install), `CHANGELOG.md` if present, and the `gh-pages` tree from #1060/#1061. Gate: link checks + `curl` greps as evidence; the README change rides the normal Rust gate. No AI attribution; none of the forbidden git commands. `Closes 1062`.
