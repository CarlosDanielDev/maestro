#!/usr/bin/env bash
# Test fixture: create a scratch git repo in a temp dir.
# Prints the temp dir path to stdout.

set -euo pipefail

tmp=$(mktemp -d -t maestro-gate-test-XXXXXX)
cd "$tmp"
git init -q
git config user.email "test@example.com"
git config user.name "Test User"
# Seed with an initial commit so `git status` behaves like a real repo.
touch README.md
git add README.md
git commit -q -m "init"
echo "$tmp"
