#!/usr/bin/env bash
#
# Manual cleanup of Rust build artifacts that bloat the maestro repo.
# Targets:
#   - target/debug       (per-test rebuild artifacts; biggest offender)
#   - target/release     (release binaries + deps)
#   - target/tmp         (cargo scratch space)
#   - target/**/incremental  (rustc incremental cache)
#   - **/.snap.new       (orphan insta snapshot drafts left from `cargo test`)
#   - **/*.profraw       (coverage instrumentation output)
#
# Default: interactive — shows per-section sizes, prompts before each rm.
# Flags:
#   --all            wipe target/ entirely (equivalent to `cargo clean`)
#   --debug          wipe target/debug only
#   --release        wipe target/release only
#   --incremental    wipe target/**/incremental only (preserves built deps)
#   --tmp            wipe target/tmp only
#   --snap-new       remove orphan *.snap.new files repo-wide
#   --profraw        remove coverage *.profraw files repo-wide
#   --yes            skip confirmation prompts (use with a specific flag)
#   --dry-run        show what would be removed, do nothing
#   -h | --help      print this header
#
# Examples:
#   bash scripts/sanitize-build-artifacts.sh                 # interactive
#   bash scripts/sanitize-build-artifacts.sh --debug --yes   # nuke debug, no prompt
#   bash scripts/sanitize-build-artifacts.sh --all --yes     # cargo clean equivalent
#   bash scripts/sanitize-build-artifacts.sh --dry-run --all # preview only

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" \
    || { echo "FAIL: must run from inside the maestro repo" >&2; exit 1; }
cd "$REPO_ROOT"

ALL=0; DEBUG=0; RELEASE=0; INCR=0; TMP=0; SNAP=0; PROF=0
YES=0; DRY=0

if [[ $# -eq 0 ]]; then
  # No flags → interactive menu mode.
  ALL=1; DEBUG=1; RELEASE=1; INCR=1; TMP=1; SNAP=1; PROF=1
fi

for arg in "$@"; do
  case "$arg" in
    --all)         ALL=1 ;;
    --debug)       DEBUG=1 ;;
    --release)     RELEASE=1 ;;
    --incremental) INCR=1 ;;
    --tmp)         TMP=1 ;;
    --snap-new)    SNAP=1 ;;
    --profraw)     PROF=1 ;;
    --yes)         YES=1 ;;
    --dry-run)     DRY=1 ;;
    -h|--help)
      sed -n '2,/^set -e/p' "$0" | sed -e 's/^# \{0,1\}//' -e '/^set -e/d'
      exit 0
      ;;
    *) echo "FAIL: unknown flag '$arg' (try --help)" >&2; exit 2 ;;
  esac
done

human() {
  local bytes
  if [[ -e "$1" ]]; then
    bytes=$(du -sh "$1" 2>/dev/null | awk '{print $1}')
    echo "$bytes"
  else
    echo "0B"
  fi
}

confirm() {
  local prompt="$1"
  if [[ $YES -eq 1 ]]; then
    return 0
  fi
  read -r -p "$prompt [y/N] " ans
  [[ "$ans" == "y" || "$ans" == "Y" ]]
}

nuke_path() {
  local path="$1"
  local label="$2"
  if [[ ! -e "$path" ]]; then
    echo "skip $label — already absent"
    return 0
  fi
  local size; size=$(human "$path")
  if [[ $DRY -eq 1 ]]; then
    echo "DRY-RUN: would rm -rf $path ($size, $label)"
    return 0
  fi
  if confirm "Remove $label at $path ($size)?"; then
    rm -rf "$path"
    echo "  -> removed $label"
  else
    echo "  -> skipped $label"
  fi
}

find_and_nuke() {
  local pattern="$1"
  local label="$2"
  local hits
  hits=$(find . -type f -name "$pattern" -not -path './target/*' 2>/dev/null || true)
  if [[ -z "$hits" ]]; then
    echo "skip $label — no matches"
    return 0
  fi
  local count; count=$(printf '%s\n' "$hits" | wc -l | tr -d ' ')
  if [[ $DRY -eq 1 ]]; then
    echo "DRY-RUN: would remove $count $label file(s):"
    printf '  %s\n' "$hits" | head -20
    [[ $count -gt 20 ]] && echo "  ... (+$((count - 20)) more)"
    return 0
  fi
  if confirm "Remove $count $label file(s)?"; then
    printf '%s\n' "$hits" | xargs rm -f
    echo "  -> removed $count files"
  else
    echo "  -> skipped"
  fi
}

# --- Report current footprint ---------------------------------------------

echo "===================================================================="
echo "Current footprint:"
echo "  target/             $(human target)"
echo "  target/debug        $(human target/debug)"
echo "  target/release      $(human target/release)"
echo "  target/tmp          $(human target/tmp)"
incr_total=0
if [[ -d target ]]; then
  while IFS= read -r d; do
    s=$(du -sk "$d" 2>/dev/null | awk '{print $1}')
    incr_total=$((incr_total + s))
  done < <(find target -type d -name incremental 2>/dev/null)
fi
echo "  target/**/incremental   $((incr_total / 1024)) MB (aggregate)"
echo "  *.snap.new (orphans)    $(find . -type f -name '*.snap.new' -not -path './target/*' 2>/dev/null | wc -l | tr -d ' ') files"
echo "  *.profraw              $(find . -type f -name '*.profraw' -not -path './target/*' 2>/dev/null | wc -l | tr -d ' ') files"
echo "===================================================================="
echo

# --- Act on each selected target ------------------------------------------

if [[ $ALL -eq 1 ]]; then
  nuke_path target "entire target/ tree"
  # If --all wins, the rest is moot.
  [[ -e target ]] || { echo; echo "Done."; exit 0; }
fi

if [[ $DEBUG    -eq 1 ]]; then nuke_path target/debug   "target/debug";   fi
if [[ $RELEASE  -eq 1 ]]; then nuke_path target/release "target/release"; fi
if [[ $TMP      -eq 1 ]]; then nuke_path target/tmp     "target/tmp";     fi

if [[ $INCR -eq 1 ]]; then
  if [[ -d target ]]; then
    incr_dirs=$(find target -type d -name incremental 2>/dev/null || true)
    if [[ -z "$incr_dirs" ]]; then
      echo "skip incremental — none present"
    else
      count=$(printf '%s\n' "$incr_dirs" | wc -l | tr -d ' ')
      if [[ $DRY -eq 1 ]]; then
        echo "DRY-RUN: would remove $count incremental dir(s) under target/"
      elif confirm "Remove $count incremental dir(s) under target/?"; then
        printf '%s\n' "$incr_dirs" | xargs rm -rf
        echo "  -> removed $count incremental dirs"
      fi
    fi
  fi
fi

if [[ $SNAP -eq 1 ]]; then find_and_nuke '*.snap.new' 'orphan insta snapshot draft'; fi
if [[ $PROF -eq 1 ]]; then find_and_nuke '*.profraw'  'coverage instrumentation';    fi

# --- Final footprint -------------------------------------------------------

echo
echo "===================================================================="
echo "Post-cleanup footprint:"
echo "  target/             $(human target)"
echo "===================================================================="
