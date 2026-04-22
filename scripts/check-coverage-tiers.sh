#!/usr/bin/env bash
# Enforce coverage floors per tier from scripts/coverage-tiers.yml
#
# Usage: check-coverage-tiers.sh <coverage.lcov>
#
# Exit codes:
#   0 — all tier floors satisfied
#   1 — some tier below its floor
#   2 — invalid input (no lcov, missing manifest, yq not installed)

set -euo pipefail

LCOV_FILE="${1:-}"
MANIFEST="${MANIFEST_OVERRIDE:-scripts/coverage-tiers.yml}"

if [[ -z "$LCOV_FILE" ]]; then
  echo "usage: $0 <coverage.lcov>" >&2
  exit 2
fi

if [[ ! -f "$LCOV_FILE" ]]; then
  echo "error: lcov file not found: $LCOV_FILE" >&2
  exit 2
fi

if [[ ! -f "$MANIFEST" ]]; then
  echo "error: manifest not found: $MANIFEST" >&2
  exit 2
fi

if ! command -v yq >/dev/null 2>&1; then
  echo "error: yq required for YAML parsing; install via: brew install yq" >&2
  exit 2
fi

# Sanity-check yq flavor. Mike Farah's Go-based yq supports `eval` as a
# subcommand and returns a known version banner. Python's yq (ubuntu apt)
# treats everything after the binary name as files, which breaks the
# script in confusing ways. Detect and fail early with a clear message.
if ! yq --version 2>&1 | grep -qi "mikefarah\|https://github.com/mikefarah/yq"; then
  echo "error: detected Python-based yq (jq wrapper). check-coverage-tiers.sh" >&2
  echo "       requires Mike Farah's Go-based yq. Install via:" >&2
  echo "         brew install yq                                           # macOS" >&2
  echo "         sudo wget -qO /usr/local/bin/yq https://github.com/mikefarah/yq/releases/latest/download/yq_linux_amd64 && sudo chmod +x /usr/local/bin/yq   # Linux" >&2
  exit 2
fi

# ---------------------------------------------------------------------------
# Read tier metadata from manifest into parallel indexed arrays (bash 3.2 safe)
# ---------------------------------------------------------------------------

tier_names=()
tier_floors=()
tier_is_excluded=()

while IFS= read -r name; do
  tier_names+=("$name")
  floor=$(yq eval ".tiers.${name}.floor // 0" "$MANIFEST")
  tier_floors+=("$floor")
  if [[ "$name" == "excluded" ]]; then
    tier_is_excluded+=("true")
  else
    tier_is_excluded+=("false")
  fi
done < <(yq eval '.tiers | keys | .[]' "$MANIFEST")

n_tiers=${#tier_names[@]}

# Guard: if the manifest produced zero tiers, fail loudly instead of
# letting the downstream array accesses crash with "unbound variable"
# under set -u.
if (( n_tiers == 0 )); then
  echo "error: no tiers found in manifest $MANIFEST" >&2
  echo "       manifest must define tiers.<name> entries with paths/floor/aspiration" >&2
  exit 2
fi

# ---------------------------------------------------------------------------
# Build a flat "tier_index:glob" list for all non-trivial path lookups.
# Format:  <tier_index> <glob_pattern>
# We'll use this in bash case-pattern matching.
# ---------------------------------------------------------------------------

# tier_path_list is a temp file: lines of "<tier_index> <pattern>"
TMPDIR_WORK=$(mktemp -d)
trap 'rm -rf "$TMPDIR_WORK"' EXIT

tier_path_file="$TMPDIR_WORK/tier_paths.txt"
: > "$tier_path_file"

for i in $(seq 0 $((n_tiers - 1))); do
  name="${tier_names[$i]}"
  while IFS= read -r pattern; do
    echo "$i $pattern" >> "$tier_path_file"
  done < <(yq eval ".tiers.${name}.paths[] // \"\"" "$MANIFEST" 2>/dev/null || true)
done

# ---------------------------------------------------------------------------
# Match a file path to a tier index (first match wins).
# Returns the index via stdout; returns n_tiers (=no match → default core)
# if nothing matched.
# ---------------------------------------------------------------------------
match_tier() {
  local file="$1"
  while read -r idx pattern; do
    case "$file" in
      $pattern)
        echo "$idx"
        return
        ;;
    esac
  done < "$tier_path_file"
  # Default to core (index 0)
  echo "0"
}

# ---------------------------------------------------------------------------
# Initialise per-tier accumulators (indexed arrays, bash 3.2 safe)
# ---------------------------------------------------------------------------
tier_lf=()
tier_lh=()
for i in $(seq 0 $((n_tiers - 1))); do
  tier_lf+=("0")
  tier_lh+=("0")
done

# ---------------------------------------------------------------------------
# Parse lcov and accumulate
# ---------------------------------------------------------------------------
current_file=""
current_lf=""
current_lh=""
in_record=false

while IFS= read -r line; do
  case "$line" in
    "SF:"*)
      current_file="${line#SF:}"
      in_record=true
      ;;
    "LF:"*)
      current_lf="${line#LF:}"
      ;;
    "LH:"*)
      current_lh="${line#LH:}"
      ;;
    "end_of_record")
      if $in_record; then
        tidx=$(match_tier "$current_file")
        is_exc="${tier_is_excluded[$tidx]}"
        if [[ "$is_exc" != "true" ]]; then
          old_lf="${tier_lf[$tidx]}"
          old_lh="${tier_lh[$tidx]}"
          tier_lf[$tidx]=$(( old_lf + current_lf ))
          tier_lh[$tidx]=$(( old_lh + current_lh ))
        fi
      fi
      in_record=false
      current_file=""
      current_lf=""
      current_lh=""
      ;;
  esac
done < "$LCOV_FILE"

# ---------------------------------------------------------------------------
# Compute coverage per tier, check floors, report
# ---------------------------------------------------------------------------
violations=0

for i in $(seq 0 $((n_tiers - 1))); do
  name="${tier_names[$i]}"
  is_exc="${tier_is_excluded[$i]}"
  [[ "$is_exc" == "true" ]] && continue

  lf="${tier_lf[$i]}"
  lh="${tier_lh[$i]}"
  floor="${tier_floors[$i]}"

  if [[ "$lf" -eq 0 ]]; then
    echo "$name: no files measured"
    continue
  fi

  pct=$(awk "BEGIN { printf \"%.1f\", ($lh / $lf) * 100 }")
  printf "%s: %s%% (floor: %s%%)\n" "$name" "$pct" "$floor"

  if awk "BEGIN { exit !($pct < $floor) }"; then
    echo "  VIOLATION: below floor"
    violations=$(( violations + 1 ))
  fi
done

if (( violations > 0 )); then
  echo ""
  echo "$violations tier(s) below floor."
  exit 1
fi

exit 0
