#!/usr/bin/env bash
# Spike #732 — nvim diff-viewer feasibility prototype harness (THROWAWAY).
#
# NOT wired into the build. Run by hand from the repo root:
#   bash scripts/spikes/nvim-diff-prototype.sh [diff-base]
#
# Demonstrates the three candidate diff-handoff paths and measures the
# parts that are observable without a human at an interactive terminal:
#   1. nvim presence + version + fallback path
#   2. Path A — two-file embedded diff:   nvim -d <a> <b>
#   3. Path B — whole-worktree diff:      git diff <base> | nvim -R -c 'set ft=diff' -
#   4. Headless open->quit timing as a proxy for the spawn/exit cost that
#      the suspend->resume cycle pays on top of crossterm mode toggles.
#
# The interactive suspend/resume latency (<300ms budget) and any keyboard
# raw-mode corruption on resume can ONLY be judged by a human running the
# real maestro binary. This harness measures process cost, not UX.

set -euo pipefail

BASE="${1:-main}"
note() { printf '\n=== %s ===\n' "$*"; }

note "1. nvim presence + fallback"
if command -v nvim >/dev/null 2>&1; then
  echo "nvim FOUND: $(command -v nvim)"
  nvim --version | head -1
  HAVE_NVIM=1
else
  echo "nvim MISSING — production fallback: render native ratatui diff widget"
  echo "                or print 'install nvim: https://neovim.io/doc/install/'"
  HAVE_NVIM=0
fi

note "2. Path A — two-file embedded (nvim -d a b)"
echo "Command maestro would spawn (blocks until :qa, like OsShellLauncher):"
echo "    nvim -d <file_before> <file_after>"
echo "Two-file ONLY. Wrong for multi-file worktree diffs (maestro median ~16 files)."

note "3. Path B — whole-worktree single read-only buffer (universal, zero-plugin)"
echo "Command:"
echo "    git diff ${BASE} | nvim -R -c 'set ft=diff' -"
DIFF_FILES=$(git diff "${BASE}" --name-only 2>/dev/null | wc -l | tr -d ' ')
DIFF_LINES=$(git diff "${BASE}" 2>/dev/null | wc -l | tr -d ' ')
echo "Current worktree diff vs ${BASE}: ${DIFF_FILES} files, ${DIFF_LINES} diff lines"

note "3b. Path B+ — diffview.nvim (richest, REQUIRES plugin installed)"
echo "    nvim -c 'DiffviewOpen ${BASE}'   # file-tree sidebar + <tab> cycling"
echo "Cannot assume installed → only an opportunistic upgrade over Path B."

note "4. Headless spawn->exit timing (proxy for resume cost, NOT interactive UX)"
if [ "${HAVE_NVIM}" = "1" ]; then
  # --headless +qa: pure startup+teardown cost.
  START=$(python3 -c 'import time;print(int(time.time()*1000))')
  nvim --headless +qa >/dev/null 2>&1 || true
  END=$(python3 -c 'import time;print(int(time.time()*1000))')
  echo "nvim --headless +qa startup/teardown: $((END-START)) ms"

  # Pipe the real worktree diff in, open ft=diff buffer, quit immediately.
  START=$(python3 -c 'import time;print(int(time.time()*1000))')
  git diff "${BASE}" 2>/dev/null | nvim -R -es -c 'set ft=diff' -c 'qa!' - >/dev/null 2>&1 || true
  END=$(python3 -c 'import time;print(int(time.time()*1000))')
  echo "git diff | nvim -R ft=diff open+quit:   $((END-START)) ms"
  echo "NOTE: interactive view time is unbounded (user reads); the 300ms"
  echo "      budget applies to the suspend->spawn and exit->resume edges only."
else
  echo "skipped — nvim missing"
fi

note "done"
