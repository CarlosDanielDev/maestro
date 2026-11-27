Parent: #1059

maestro has no landing page: `gh repo view` shows `homepageUrl: ""`; the closest asset is the hero SVG (`README.md:14`). The repo is public with a strong README, a hero SVG (`docs/assets/readme-hero.svg`), a wiki, and a Discord — but nothing shareable as a URL. Decision (owner, 2026-10-15): project Pages on an orphan `gh-pages` branch, one static HTML file, zero frameworks and zero build steps — the site must never need a toolchain to change. The install command the page teaches is the crate named at `Cargo.toml:1`.

## Por que não é story

No product behavior changes; the gain is the URL existing at all — infra for the slices that fill it.

## Done when

- [ ] an orphan `gh-pages` branch exists containing `index.html` (single file, inline CSS, dark terminal aesthetic, no JS framework, no external assets beyond fonts already system-available) plus `docs/assets/readme-hero.svg` copied in — `git ls-tree -r gh-pages --name-only` shows exactly those — owner: implementer
- [ ] GitHub Pages is enabled serving the `gh-pages` branch root — `gh api repos/CarlosDanielDev/maestro/pages --jq .html_url` prints `https://carlosdanieldev.github.io/maestro/` — owner: implementer
- [ ] the page responds 200 and contains the tagline, the install command (`cargo install maestro` — confirm the real crate name on crates.io first; if unpublished, use `cargo install --git https://github.com/CarlosDanielDev/maestro` and say so in the PR), and a link to the repo — `curl -s <url> | grep -c "cargo install"` prints ≥ 1 — owner: implementer
- [ ] the HTML carries title + description meta tags matching the repo description — owner: implementer

## ADR stub

Serving shape: (a) orphan `gh-pages` branch (this issue — zero coupling to the Rust tree, no `/docs` pollution: `docs/` already holds real project docs); (b) Pages from `/docs` on main (rejected — serves the whole docs tree); (c) static site generator (rejected — a toolchain to maintain for one page). Recommendation (to argue with): (a). Framework: none — inline CSS only.

## Out of scope

- Custom domain, analytics, SEO beyond meta tags, i18n.
- Any JS interactivity — the page is static text and images.
- The demo capture (#1061) and full content (#1062) — this issue ships the shell with tagline + install + repo link only.

## Handoff

```bash
git worktree add -b chore/1060-gh-pages-scaffold ../maestro-1060 origin/main
```

The `gh-pages` branch is created from scratch (`git checkout --orphan`) inside this worktree, committed, pushed; the PR to `main` carries only the note that Pages was enabled (or is a no-op PR if `gh api` needs nothing on main — state which). Enabling Pages: `gh api repos/CarlosDanielDev/maestro/pages -X POST -f source[branch]=gh-pages -f source[path]=/` (admin token required — if 403, stop and report). Gate: the Rust gate does not apply to the gh-pages tree; run `curl` checks + `gh api` as evidence. No AI attribution; none of the forbidden git commands. `Closes 1060`.
