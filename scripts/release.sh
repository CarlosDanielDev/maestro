#!/usr/bin/env bash
# scripts/release.sh — Automates the /release skill workflow without spending AI tokens.
#
# Usage:
#   ./scripts/release.sh v0.4.0
#   ./scripts/release.sh 0.4.0
#   ./scripts/release.sh --milestone "v0.4.0"
#   ./scripts/release.sh            # auto-detect from fully-completed milestones
#
# Requirements: git, gh, cargo, cargo-insta, jq, python3

set -euo pipefail

REPO="CarlosDanielDev/maestro"
SNAPSHOTS_DIR="src/tui/snapshot_tests/snapshots"

# ── colours ────────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${CYAN}▸ $*${RESET}"; }
success() { echo -e "${GREEN}✔ $*${RESET}"; }
warn()    { echo -e "${YELLOW}⚠ $*${RESET}"; }
die()     { echo -e "${RED}✖ $*${RESET}" >&2; exit 1; }
confirm() {
  local prompt="$1"
  echo -e "${YELLOW}${prompt} [y/N]${RESET}"
  read -r ans
  [[ "$ans" =~ ^[Yy]$ ]]
}

# ── Step 1: Determine version ───────────────────────────────────────────────────
MILESTONE_NAME=""
RAW_VERSION=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --milestone) MILESTONE_NAME="$2"; shift 2 ;;
    v*|[0-9]*) RAW_VERSION="$1"; shift ;;
    *) die "Unknown argument: $1" ;;
  esac
done

if [[ -n "$MILESTONE_NAME" ]]; then
  RAW_VERSION="$MILESTONE_NAME"
fi

strip_v() { echo "${1#v}"; }

if [[ -z "$RAW_VERSION" ]]; then
  info "Auto-detecting version from fully-completed milestones..."
  COMPLETED=$(gh api "repos/${REPO}/milestones" --jq \
    '.[] | select(.open_issues == 0 and .closed_issues > 0) | .title' 2>/dev/null || true)

  if [[ -z "$COMPLETED" ]]; then
    warn "No fully-completed milestones found. Available milestones:"
    gh api "repos/${REPO}/milestones" --jq \
      '.[] | .title + " (" + (.closed_issues|tostring) + "/" + ((.open_issues + .closed_issues)|tostring) + " closed)"'
    die "Provide a version or --milestone."
  fi

  COUNT=$(echo "$COMPLETED" | wc -l | tr -d ' ')
  if [[ "$COUNT" -eq 1 ]]; then
    RAW_VERSION="$COMPLETED"
    info "Using milestone: $RAW_VERSION"
  else
    echo "Multiple completed milestones:"
    echo "$COMPLETED" | nl -ba
    echo -e "${YELLOW}Enter the number of the milestone to release:${RESET}"
    read -r PICK
    RAW_VERSION=$(echo "$COMPLETED" | sed -n "${PICK}p")
    [[ -z "$RAW_VERSION" ]] && die "Invalid selection."
  fi
fi

VERSION=$(strip_v "$RAW_VERSION")
TAG="v${VERSION}"

[[ -z "$VERSION" ]] && die "Could not determine a version."
info "Releasing ${BOLD}${TAG}${RESET}${CYAN}"

# ── Step 2: Preconditions ───────────────────────────────────────────────────────
info "Checking preconditions..."

BRANCH=$(git rev-parse --abbrev-ref HEAD)
[[ "$BRANCH" != "main" ]] && die "Must be on 'main' branch (currently on '${BRANCH}')."
success "Branch: main"

if ! git diff --quiet || ! git diff --cached --quiet; then
  die "Working tree is dirty. Commit or stash changes first."
fi
success "Working tree: clean"

if git tag -l "$TAG" | grep -q .; then
  die "Tag ${TAG} already exists. Remove it first if you want to re-release."
fi
success "Tag ${TAG}: not yet present"

info "Running cargo test..."
cargo test --bin maestro 2>&1 | tail -5
success "Tests pass (pre-bump)"

info "Running cargo clippy..."
cargo clippy -- -D warnings -A dead_code 2>&1 | tail -5
success "Clippy: clean"

info "Running cargo fmt --check..."
cargo fmt -- --check 2>&1 || die "Formatting issues found. Run 'cargo fmt' first."
success "Formatting: clean"

# ── Step 3: Gather changelog content ───────────────────────────────────────────
info "Fetching closed issues for milestone '${TAG}'..."

ISSUES_JSON=$(gh issue list \
  --milestone "$TAG" \
  --state closed \
  --json number,title,labels \
  --limit 200 2>/dev/null || echo "[]")

ISSUE_COUNT=$(echo "$ISSUES_JSON" | jq length)
[[ "$ISSUE_COUNT" -eq 0 ]] && warn "No closed issues found for milestone '${TAG}'."

TODAY=$(date +%Y-%m-%d)

# Group issues by label — pass JSON via env var to avoid stdin conflicts
CHANGELOG_SECTION=$(VERSION="$VERSION" TODAY="$TODAY" ISSUES_JSON="$ISSUES_JSON" python3 <<'PYEOF'
import sys, json, os

version     = os.environ["VERSION"]
today       = os.environ["TODAY"]
issues_raw  = os.environ.get("ISSUES_JSON", "[]")
issues      = json.loads(issues_raw) if issues_raw.strip() else []

LABEL_MAP = {
    "enhancement": "feat", "feature": "feat", "type:feature": "feat",
    "bug": "fix", "type:bug": "fix",
    "refactor": "refactor", "type:refactor": "refactor",
    "documentation": "docs", "type:docs": "docs",
    "ci": "ci", "type:ci": "ci",
    "performance": "perf", "type:perf": "perf",
    "test": "test", "type:test": "test",
}

buckets = {"feat": [], "fix": [], "refactor": [], "docs": [], "ci": [], "perf": [], "test": [], "chore": []}

for issue in issues:
    num    = issue["number"]
    title  = issue["title"]
    labels = [l["name"] for l in issue.get("labels", [])]
    bucket = "chore"
    for lbl in labels:
        if lbl in LABEL_MAP:
            bucket = LABEL_MAP[lbl]
            break
    buckets[bucket].append(f"- {title} (#{num})")

lines = [f"## [{version}] - {today}", ""]

SECTION_TITLES = {
    "feat":     "### Added",
    "fix":      "### Fixed",
    "refactor": "### Changed",
    "docs":     "### Documentation",
    "ci":       "### CI",
    "perf":     "### Performance",
    "test":     "### Tests",
    "chore":    "### Chore",
}

for bucket, header in SECTION_TITLES.items():
    if buckets[bucket]:
        lines.append(header)
        lines.extend(buckets[bucket])
        lines.append("")

print("\n".join(lines).rstrip())
PYEOF
)

echo ""
echo -e "${BOLD}Changelog section to be added:${RESET}"
echo "────────────────────────────────────────"
echo "$CHANGELOG_SECTION"
echo "────────────────────────────────────────"
echo ""
confirm "Proceed with this changelog?" || die "Aborted by user."

# ── Step 4: Update files ────────────────────────────────────────────────────────
info "Updating Cargo.toml..."
CURRENT_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
sed -i '' "s/^version = \"${CURRENT_VERSION}\"/version = \"${VERSION}\"/" Cargo.toml
success "Cargo.toml: ${CURRENT_VERSION} → ${VERSION}"

info "Updating CHANGELOG.md..."
# Pass new section via env var to avoid stdin conflicts
NEW_SECTION="$CHANGELOG_SECTION" python3 <<'PYEOF'
import os, re

new_section = os.environ["NEW_SECTION"]

with open("CHANGELOG.md", "r") as f:
    content = f.read()

# If [Unreleased] has content, prepend it into the new section then clear it
unreleased_pattern = re.compile(
    r'(## \[Unreleased\]\n)(.*?)((?=## \[))',
    re.DOTALL
)
m = unreleased_pattern.search(content)
if m:
    unreleased_content = m.group(2).strip()
    if unreleased_content:
        new_section = new_section + "\n" + unreleased_content
    content = unreleased_pattern.sub(r'\1\n', content)

# Insert new section right after ## [Unreleased] + blank line
insert_after = "## [Unreleased]\n\n"
if insert_after not in content:
    insert_after = "## [Unreleased]\n"

content = content.replace(insert_after, insert_after + new_section + "\n\n", 1)

with open("CHANGELOG.md", "w") as f:
    f.write(content)

print("CHANGELOG.md updated.")
PYEOF
success "CHANGELOG.md updated"

# ── Step 4b: Post-bump test gate ────────────────────────────────────────────────
info "Running post-bump test gate..."

# Known-drift groups: these three groups fail every release due to version/changelog embedding
KNOWN_DRIFT_GROUPS=(
  "tui::snapshot_tests::dashboard"
  "tui::snapshot_tests::landing"
  "tui::snapshot_tests::agent_graph_dispatcher"
)

KNOWN_DRIFT_PATTERNS=(
  "tui::snapshot_tests::dashboard::home_screen_"
  "tui::snapshot_tests::landing::landing_welcome_"
  "tui::snapshot_tests::agent_graph_dispatcher::agent_graph_dispatcher_"
)

# Capture full output; tolerate non-zero exit (failures are expected)
TEST_OUTPUT=$(cargo test --bin maestro 2>&1 || true)

if echo "$TEST_OUTPUT" | grep -qE "^test result: ok"; then
  success "Post-bump tests pass"
else
  # Parse failed test names from cargo output:  "test <name> ... FAILED"
  FAILURES=$(echo "$TEST_OUTPUT" | grep -E "^test .+ \.\.\. FAILED" | sed 's/^test \(.*\) \.\.\. FAILED$/\1/' || true)

  if [[ -z "$FAILURES" ]]; then
    # Fallback: parse the "failures:" block
    FAILURES=$(echo "$TEST_OUTPUT" | awk '/^failures:/{found=1;next} found && /^    /{print $1} found && /^$/{exit}' || true)
  fi

  if [[ -z "$FAILURES" ]]; then
    # Something else went wrong (compile error, etc.)
    echo "$TEST_OUTPUT" | tail -20
    die "Tests failed but no test names could be parsed. Check output above."
  fi

  # Verify every failure belongs to a known-drift group
  UNKNOWN_FAILURES=""
  while IFS= read -r failure; do
    [[ -z "$failure" ]] && continue
    IS_KNOWN=false
    for pattern in "${KNOWN_DRIFT_PATTERNS[@]}"; do
      if [[ "$failure" == *"$pattern"* ]]; then
        IS_KNOWN=true
        break
      fi
    done
    if [[ "$IS_KNOWN" == false ]]; then
      UNKNOWN_FAILURES="${UNKNOWN_FAILURES}\n  ${failure}"
    fi
  done <<< "$FAILURES"

  if [[ -n "$UNKNOWN_FAILURES" ]]; then
    echo -e "${RED}Unknown test failures (NOT auto-accepted):${RESET}"
    echo -e "$UNKNOWN_FAILURES"
    die "Real regressions detected. Fix before releasing."
  fi

  warn "Known-drift snapshot failures detected. Accepting..."

  CARGO_INSTA="${HOME}/.cargo/bin/cargo-insta"
  [[ ! -x "$CARGO_INSTA" ]] && CARGO_INSTA="cargo-insta"
  command -v "$CARGO_INSTA" > /dev/null 2>&1 || die "cargo-insta not found. Run: cargo install cargo-insta"

  for i in "${!KNOWN_DRIFT_GROUPS[@]}"; do
    GROUP="${KNOWN_DRIFT_GROUPS[$i]}"
    PATTERN="${KNOWN_DRIFT_PATTERNS[$i]}"
    if echo "$FAILURES" | grep -q "$PATTERN" 2>/dev/null; then
      info "Accepting snapshots for ${GROUP}..."
      "$CARGO_INSTA" test --accept -- "$GROUP" 2>&1 | tail -3
    fi
  done

  info "Re-running tests after snapshot acceptance..."
  cargo test --bin maestro > /dev/null 2>&1 || die "Tests still failing after snapshot acceptance."
  success "Post-bump tests pass (after snapshot update)"
fi

# ── Step 5: Commit ──────────────────────────────────────────────────────────────
info "Staging files..."
git add Cargo.toml Cargo.lock CHANGELOG.md

# Stage known-drift snapshots if they were updated
git add "${SNAPSHOTS_DIR}/maestro__tui__snapshot_tests__dashboard__home_screen_"*.snap 2>/dev/null || true
git add "${SNAPSHOTS_DIR}/maestro__tui__snapshot_tests__landing__landing_welcome_"*.snap 2>/dev/null || true
git add "${SNAPSHOTS_DIR}/maestro__tui__snapshot_tests__agent_graph_dispatcher__agent_graph_dispatcher_"*.snap 2>/dev/null || true

SUMMARY=$(echo "$CHANGELOG_SECTION" | grep -E '^- ' | head -3 | sed 's/^- //' | paste -sd '; ' -)

git commit -m "chore: release ${TAG}

${SUMMARY}"
success "Committed version bump"

# ── Step 6: Tag and push ────────────────────────────────────────────────────────
info "Creating annotated tag ${TAG}..."

ISSUE_LIST=$(echo "$ISSUES_JSON" | jq -r '.[] | "  - " + .title + " (#" + (.number|tostring) + ")"' 2>/dev/null || echo "  (no issues)")

git tag -a "$TAG" -m "${TAG}

Includes:
${ISSUE_LIST}"
success "Tag ${TAG} created"

confirm "Push commit and tag to origin/main?" || die "Aborted by user. Commit and tag exist locally."

PUSH_FAILED=false
git push origin main --tags 2>&1 || PUSH_FAILED=true

if [[ "$PUSH_FAILED" == true ]]; then
  # Branch-protection may have blocked commit push but allowed tag push — check
  TAG_ON_REMOTE=$(git ls-remote origin "refs/tags/${TAG}" 2>/dev/null | awk '{print $1}')
  if [[ -z "$TAG_ON_REMOTE" ]]; then
    git push origin "$TAG" 2>/dev/null || true
    TAG_ON_REMOTE=$(git ls-remote origin "refs/tags/${TAG}" 2>/dev/null | awk '{print $1}')
  fi

  BRANCH_NAME="release/${TAG}"
  info "Branch-protection blocked direct push to main. Creating PR branch..."
  git checkout -b "$BRANCH_NAME"
  git push -u origin "$BRANCH_NAME"

  PR_URL=$(gh pr create \
    --base main \
    --head "$BRANCH_NAME" \
    --title "chore: release ${TAG}" \
    --body "Release PR for ${TAG}. Tag already pushed; binaries are being built by release.yml. Merging this makes main reflect the release commit.")

  warn "Direct push to main was rejected by branch-protection."
  echo -e "${CYAN}Tag on remote:${RESET} ${TAG_ON_REMOTE:-not found (check manually)}"
  echo -e "${CYAN}Release PR:${RESET} ${PR_URL}"
  echo -e "${YELLOW}Merge the PR once checks are green.${RESET}"
else
  success "Pushed main + ${TAG} to origin"
fi

# ── Step 7: Release workflow check ─────────────────────────────────────────────
if [[ -f ".github/workflows/release.yml" ]]; then
  success "release.yml exists — GitHub Actions will build binaries automatically."
  echo -e "${CYAN}Workflow:${RESET} https://github.com/${REPO}/actions/workflows/release.yml"
else
  info "No release.yml found. Creating GitHub Release manually..."
  gh release create "$TAG" \
    --title "${TAG}" \
    --notes "$CHANGELOG_SECTION"
  success "GitHub Release created"
fi

# ── Step 9: Close milestone ─────────────────────────────────────────────────────
MILESTONE_NUMBER=$(gh api "repos/${REPO}/milestones" \
  --jq ".[] | select(.title == \"${TAG}\") | .number" 2>/dev/null || true)

if [[ -n "$MILESTONE_NUMBER" ]]; then
  info "Closing milestone ${TAG} (#${MILESTONE_NUMBER})..."
  gh api "repos/${REPO}/milestones/${MILESTONE_NUMBER}" -X PATCH -f state=closed > /dev/null
  success "Milestone ${TAG} closed"
else
  warn "Milestone '${TAG}' not found on GitHub — skipping close."
fi

# ── Step 10: Summary ────────────────────────────────────────────────────────────
COMMIT_HASH=$(git rev-parse --short HEAD)
echo ""
echo -e "${BOLD}${GREEN}Release Complete!${RESET}"
echo ""
echo -e "  ${BOLD}Version:${RESET}   ${TAG}"
echo -e "  ${BOLD}Tag:${RESET}       ${TAG}"
echo -e "  ${BOLD}Commit:${RESET}    ${COMMIT_HASH}"
echo -e "  ${BOLD}Milestone:${RESET} ${TAG} (closed)"
echo -e "  ${BOLD}Issues:${RESET}    ${ISSUE_COUNT} issues included"
if [[ -f ".github/workflows/release.yml" ]]; then
  echo -e "  ${BOLD}Release:${RESET}   building via release.yml"
fi
echo ""
echo -e "${BOLD}Changelog:${RESET}"
echo "$CHANGELOG_SECTION"
