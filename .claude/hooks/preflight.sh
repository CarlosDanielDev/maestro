#!/usr/bin/env bash
# Pre-flight CI rehearsal for /implement.
#
# Runs the fast per-PR gates locally before a branch is created, so
# obvious regressions fail in ~10 seconds instead of ~10 minutes on
# GitHub Actions.
#
# Gates included:
#   - cargo fmt --check
#   - cargo clippy -- -D warnings -A dead_code
#   - scripts/check-file-size.sh (includes deadline enforcement from Wave 1)
#
# Gates NOT included (CI-only):
#   - cargo test            (takes too long for a pre-flight)
#   - coverage              (Wave 2.1; same reason)
#   - layer check           (Wave 2.3)
#   - cargo deny            (networked; CI-only)
#
# See docs/superpowers/specs/2026-04-22-ci-quality-gates-design.md for
# the broader gate design.

set -e

echo "preflight: cargo fmt --check"
cargo fmt -- --check

echo "preflight: cargo clippy -- -D warnings -A dead_code"
cargo clippy -- -D warnings -A dead_code

echo "preflight: scripts/check-file-size.sh"
bash scripts/check-file-size.sh

echo "preflight: all clear"
