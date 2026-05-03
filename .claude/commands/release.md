# Release

Create a semantic version release — bump version, update changelog, tag, push, and create GitHub Release.

**Usage:** `/release` or `/release v0.4.0` or `/release --milestone "v0.4.0"`

---

## Arguments

`$ARGUMENTS` may contain:
- A version string (e.g., `v0.4.0`, `0.4.0`) — use it directly
- `--milestone <name>` — derive version from milestone name
- Nothing — auto-detect from the active milestone with the most recently closed issues

---

## Instructions

Execute ALL steps in order.

### Step 1: Determine the Version

**If a version was provided:** use it (strip leading `v` if needed for Cargo.toml, keep `v` prefix for git tag).

**If `--milestone` was provided:** use the milestone name as the version (e.g., milestone "v0.4.0" → version 0.4.0).

**If nothing provided:**
1. Run `gh api repos/{owner}/{repo}/milestones --jq '.[] | select(.open_issues == 0 and .closed_issues > 0) | .title'` to find fully-completed milestones
2. If none found, run `gh api repos/{owner}/{repo}/milestones --jq '.[] | .title + " (" + (.closed_issues|tostring) + "/" + ((.open_issues + .closed_issues)|tostring) + " closed)"'` and ask the user to pick
3. Extract version from milestone title

### Step 2: Validate Preconditions

1. Must be on `main` branch — if not, ask user to switch
2. Working tree must be clean — if dirty, ask to commit or stash
3. Tag must not already exist — run `git tag -l v<version>` to check
4. Run `cargo test` to ensure all tests pass
5. Run `cargo clippy -- -D warnings -A dead_code` to ensure no lint errors
6. Run `cargo fmt -- --check` to ensure formatting is clean

If any check fails, STOP and report the issue.

> **Note:** These pre-bump tests confirm `main` is green. The post-bump tests in Step 4b are the ones that catch changelog-driven snapshot drift.

### Step 3: Gather Changelog Content

1. Get the current version from `Cargo.toml`: `grep '^version' Cargo.toml`
2. Fetch all closed issues in the milestone: `gh issue list --milestone "<milestone>" --state closed --json number,title,labels`
3. Group issues by type using labels:
   - `feat:` — issues with labels: enhancement, feature, type:feature
   - `fix:` — issues with labels: bug, type:bug
   - `refactor:` — issues with labels: refactor, type:refactor
   - `docs:` — issues with labels: documentation, type:docs
   - `ci:` — issues with labels: ci, type:ci
   - `perf:` — issues with labels: performance, type:perf
   - `test:` — issues with labels: test, type:test
   - `chore:` — anything else
4. Format as a changelog section:

```markdown
## [<version>] - <YYYY-MM-DD>

### Added
- <feat issues as bullet points with #number>

### Fixed
- <fix issues>

### Changed
- <refactor/chore issues>

### Documentation
- <docs issues>
```

### Step 4: Update Files

1. **Cargo.toml**: Update `version = "<new-version>"`
2. **CHANGELOG.md**: Insert the new version section between `## [Unreleased]` and the previous version
3. If `## [Unreleased]` has content, move it into the new version section and leave `## [Unreleased]` empty

### Step 4b: Post-Bump Test Gate (MANDATORY — catches snapshot drift)

**Why this step exists:** several TUI screens embed the version string, so bumping the version causes their insta snapshots to drift every release. Three groups are known to fail every release:

| Snapshot group | Count | Why it drifts |
|---|---|---|
| `tui::snapshot_tests::dashboard::home_screen_*` | 4 | "What's New" widget reads top CHANGELOG entry at runtime |
| `tui::snapshot_tests::landing::landing_welcome_*` | 6 | Welcome screen renders the version string (#582) |
| `tui::snapshot_tests::agent_graph_dispatcher::agent_graph_dispatcher_*` | 2 | Agent-graph dispatcher view renders the version string |

Skipping this step guarantees CI failure on the release PR.

1. Run `cargo test --bin maestro` — record the output.
2. **If all tests pass**: proceed to Step 5.
3. **If tests fail AND every failure belongs to one of the three known-drift groups above**:
   a. Run (use the absolute path `~/.cargo/bin/cargo-insta` if `cargo insta` isn't on PATH):
      ```bash
      ~/.cargo/bin/cargo-insta test --accept -- tui::snapshot_tests::dashboard
      ~/.cargo/bin/cargo-insta test --accept -- tui::snapshot_tests::landing
      ~/.cargo/bin/cargo-insta test --accept -- tui::snapshot_tests::agent_graph_dispatcher
      ```
      (Skip any group that had no failures — `cargo insta` is a no-op when nothing needs review.)
   b. Re-run `cargo test --bin maestro` and confirm it now passes.
   c. These snapshot updates will be included in the **same** commit as the version bump in Step 5.
4. **If any other tests fail** (anything outside the three groups listed above): STOP — do NOT auto-accept. Report the failures to the user. These are real regressions and must be investigated.

> **Do NOT blanket-accept snapshots.** Only the known-drift groups above are expected to fail from a version bump. Any failure outside those groups is a real regression hiding behind an automated accept.

### Step 5: Commit the Version Bump

```bash
git add Cargo.toml CHANGELOG.md
# Also include any snapshot updates from Step 4b (all three known-drift groups):
git add src/tui/snapshot_tests/snapshots/maestro__tui__snapshot_tests__dashboard__home_screen_*.snap 2>/dev/null || true
git add src/tui/snapshot_tests/snapshots/maestro__tui__snapshot_tests__landing__landing_welcome_*.snap 2>/dev/null || true
git add src/tui/snapshot_tests/snapshots/maestro__tui__snapshot_tests__agent_graph_dispatcher__agent_graph_dispatcher_*.snap 2>/dev/null || true
git commit -m "chore: release v<version>

<one-line summary of what's in this release>

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

### Step 6: Create and Push Tag

```bash
git tag -a v<version> -m "v<version> — <milestone description or summary>

Includes:
<list of issue titles with #numbers>"
```

Push both:
```bash
git push origin main --tags
```

**If direct push to `main` is rejected by branch-protection rules** (e.g. `GH013: Repository rule violations` / required status checks): the tag push will still have succeeded because `git push origin main --tags` pushes each ref independently. Verify this first:

```bash
git ls-remote origin refs/tags/v<version>
```

If the tag is on origin, the `release.yml` workflow has already been triggered — do NOT delete or re-push the tag. Instead, land the release commit via PR:

```bash
git checkout -b release/v<version>
git push -u origin release/v<version>
gh pr create --base main --head release/v<version> \
  --title "chore: release v<version>" \
  --body "Release PR for v<version>. Tag already pushed; binaries are being built by release.yml. Merging this makes main reflect the release commit."
```

Report both the PR URL and the running `release.yml` workflow URL to the user, and ask them to merge the PR once checks are green.

### Step 7: Wait for Release Workflow (if configured)

Check if `.github/workflows/release.yml` exists:
- If yes: the release workflow will auto-create the GitHub Release with binaries. Report this to the user.
- If no: create a GitHub Release manually (Step 8).

### Step 8: Create GitHub Release (if no workflow)

Only if no release workflow exists:

```bash
gh release create v<version> --title "v<version> — <short summary>" --notes "<changelog content for this version>"
```

### Step 9: Close Milestone (if applicable)

If a milestone was used:
```bash
gh api repos/{owner}/{repo}/milestones/<milestone-number> -X PATCH -f state=closed
```

### Step 10: Summary

```
Release Complete!

  Version:    v<version>
  Tag:        v<version>
  Commit:     <hash>
  Milestone:  <name> (closed)
  Issues:     <N> issues included
  Release:    <url or "building via release workflow">

Changelog:
<the changelog section that was added>
```

---

## Error Handling

- If `cargo test` fails → STOP, do not release broken code
- If tag already exists → ask user if they want to re-tag (delete and recreate)
- If push fails → STOP, do not create release with missing tag
- If milestone not found → ask user to provide version manually
- NEVER release from a non-main branch without explicit user confirmation

---

## Safety Checks

- NEVER force push tags
- NEVER skip tests before releasing
- NEVER release if there are uncommitted changes
- Always show the user the changelog before committing
- Ask for confirmation before pushing the tag
- ALWAYS run the Step 4b post-bump test gate — this is how we catch the four `dashboard::home_screen_*` insta snapshot failures that happen every release because the "What's New" widget reads CHANGELOG.md at runtime
- NEVER blanket `cargo insta accept` — only accept the specific `home_screen_*` dashboard snapshots; any other snapshot drift is a real regression
- If branch-protection blocks `git push origin main`, do NOT delete or re-push the tag; fall back to a `release/v<version>` PR (see Step 6 fallback)
