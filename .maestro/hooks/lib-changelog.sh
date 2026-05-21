#!/usr/bin/env bash
# CHANGELOG-drift detection helpers for implement-gates.sh.
#
# The landing screen embeds CHANGELOG.md via include_str! (src/changelog/mod.rs)
# and renders trend bars from .entries.iter().take(24) in release_ref_trend()
# (src/tui/screens/landing/draw.rs). Any edit to CHANGELOG.md can shift those
# bars and break the landing snapshots. These helpers let the pre-check hook
# surface drift as a fast, scoped, actionable signal scoped to CHANGELOG edits.
#
# Sourced from implement-gates.sh and tests/test-changelog-gate.sh.

# Echoes "main" or "origin/main" or empty string if neither ref is reachable.
resolve_changelog_diff_base() {
  if git rev-parse --verify --quiet main >/dev/null 2>&1; then
    echo "main"
  elif git rev-parse --verify --quiet origin/main >/dev/null 2>&1; then
    echo "origin/main"
  else
    echo ""
  fi
}

# Returns 0 when CHANGELOG.md (exact, anchored) appears in the diff between
# HEAD and the given base ref; returns 1 otherwise.
#
# Anchored grep (`-x`) prevents false positives from paths like
# docs/CHANGELOG-archive.md or scripts/CHANGELOG.md.tmpl that contain the
# string CHANGELOG.md.
changelog_changed_vs_base() {
  local base="${1:?base ref required}"
  git diff "${base}...HEAD" --name-only -z \
    | tr '\0' '\n' \
    | grep -qx 'CHANGELOG.md'
}
