#!/usr/bin/env bash
# Clean large disposable Cargo artifacts from this repository's target folder.
#
# Dry-run by default:
#   scripts/sanitize-target.sh
#
# Remove debug/doc/cache artifacts, preserving target/release:
#   scripts/sanitize-target.sh --execute
#
# Also remove target/release:
#   scripts/sanitize-target.sh --execute --include-release

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/sanitize-target.sh [options]

Options:
  --execute          Actually delete files. Without this, prints a dry run.
  --include-release  Also delete target/release. Requires --execute.
  --target-dir DIR   Target directory to clean. Defaults to ./target.
  -h, --help         Show this help.

Default cleanup removes disposable Cargo build outputs:
  target/debug
  target/doc
  target/criterion
  target/tmp
  target/.rustc_info.json
  target/.rustdoc_fingerprint.json

target/release is preserved unless --include-release is passed.
EOF
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
target_dir="$repo_root/target"
execute=0
include_release=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --execute)
      execute=1
      ;;
    --include-release)
      include_release=1
      ;;
    --target-dir)
      shift
      if [ "$#" -eq 0 ]; then
        echo "sanitize-target: --target-dir requires a path" >&2
        exit 2
      fi
      target_dir="$1"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "sanitize-target: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if [ "$include_release" -eq 1 ] && [ "$execute" -ne 1 ]; then
  echo "sanitize-target: --include-release requires --execute" >&2
  exit 2
fi

if [ ! -d "$target_dir" ]; then
  echo "sanitize-target: target directory not found: $target_dir" >&2
  exit 1
fi

target_dir="$(cd "$target_dir" && pwd -P)"
expected_target="$repo_root/target"

if [ "$target_dir" != "$expected_target" ]; then
  case "$target_dir" in
    */target) ;;
    *)
      echo "sanitize-target: refusing to clean non-target directory: $target_dir" >&2
      exit 1
      ;;
  esac
fi

candidates=(
  "$target_dir/debug"
  "$target_dir/doc"
  "$target_dir/criterion"
  "$target_dir/tmp"
  "$target_dir/.rustc_info.json"
  "$target_dir/.rustdoc_fingerprint.json"
)

if [ "$include_release" -eq 1 ]; then
  candidates+=("$target_dir/release")
fi

print_size() {
  if [ -e "$1" ]; then
    du -sh "$1" 2>/dev/null | awk '{print $1}'
  else
    printf 'missing'
  fi
}

echo "Target: $target_dir"
echo "Mode: $([ "$execute" -eq 1 ] && echo execute || echo dry-run)"
echo

if [ -d "$target_dir" ]; then
  echo "Current target size: $(print_size "$target_dir")"
fi

echo "Cleanup candidates:"
found=0
for path in "${candidates[@]}"; do
  if [ -e "$path" ]; then
    found=1
    printf '  %8s  %s\n' "$(print_size "$path")" "$path"
  fi
done

if [ "$found" -eq 0 ]; then
  echo "  none"
  exit 0
fi

if [ "$execute" -ne 1 ]; then
  echo
  echo "Dry run only. Re-run with --execute to delete these artifacts."
  echo "Use --include-release only if you also want to delete release builds."
  exit 0
fi

echo
echo "Deleting cleanup candidates..."
for path in "${candidates[@]}"; do
  if [ -e "$path" ]; then
    rm -rf -- "$path"
    echo "  removed $path"
  fi
done

echo
echo "Remaining target size: $(print_size "$target_dir")"
