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

### Step 5: Commit the Version Bump

```bash
git add Cargo.toml CHANGELOG.md
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
