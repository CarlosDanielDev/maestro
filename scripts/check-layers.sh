#!/usr/bin/env bash
# Enforce architecture layer rules from scripts/architecture-layers.yml.
#
# Usage: check-layers.sh [<manifest>]
#   Default manifest: scripts/architecture-layers.yml
#
# Scans every .rs file under src/, extracts `use crate::...` statements,
# checks layer ordering and forbidden pairs.
#
# Exit codes:
#   0 — no violations
#   1 — one or more violations
#   2 — invalid input (missing manifest, missing yq)
#
# Known v1 limitations (intentional — "simple enough not to need a full
# Rust parser" per spec):
#   - Brace-group imports (`use crate::mod::{TypeA, TypeB};`) are not
#     expanded. The extracted path becomes "{TypeA, TypeB}" which
#     use_to_path can't resolve; the import is silently ignored.
#     Such imports exist in this codebase. Addressable in a future
#     version by pre-expanding braces or switching to syn-based parsing.
#   - Nested `use` blocks and `pub use` re-exports are treated the same
#     as plain `use`; re-exports from TUI into domain layers would be
#     flagged incorrectly if they occur.

set -euo pipefail

MANIFEST="${1:-scripts/architecture-layers.yml}"
DEBT_FILE="${DEBT_FILE_OVERRIDE:-docs/layers-debt.txt}"

if [[ ! -f "$MANIFEST" ]]; then
  echo "error: manifest not found: $MANIFEST" >&2
  exit 2
fi

if ! command -v yq >/dev/null 2>&1; then
  echo "error: yq required for YAML parsing; install via: brew install yq" >&2
  exit 2
fi

# ---------------------------------------------------------------------------
# Glob-match helper (bash 3.2 safe — uses case statement).
# ---------------------------------------------------------------------------
matches_any_glob() {
  local file="$1"
  shift
  local pattern
  for pattern in "$@"; do
    case "$file" in
      $pattern) return 0 ;;
    esac
  done
  return 1
}

# ---------------------------------------------------------------------------
# Read layer metadata into parallel indexed arrays (bash 3.2 safe).
# No declare -A, no mapfile.
# ---------------------------------------------------------------------------
layer_nums=()
layer_path_counts=()
# We'll build a flat list of all paths per layer as layer_paths_N_I variables
# by storing them in named variables via eval, because bash 3.2 has no arrays
# of arrays. Instead we use a flat encoded approach:
#   layer_paths_<layer_index>_<path_index> = path
# We track how many paths each layer has in layer_path_counts.

layer_count=$(yq eval '.layers | length' "$MANIFEST")

for (( i=0; i<layer_count; i++ )); do
  num=$(yq eval ".layers[$i].number" "$MANIFEST")
  layer_nums+=("$num")
  pcount=$(yq eval ".layers[$i].paths | length" "$MANIFEST")
  layer_path_counts+=("$pcount")
  for (( p=0; p<pcount; p++ )); do
    pval=$(yq eval ".layers[$i].paths[$p]" "$MANIFEST")
    # Strip surrounding quotes if yq outputs them (it shouldn't for scalars,
    # but be defensive).
    pval="${pval#\"}"
    pval="${pval%\"}"
    # Store as a named variable: layer_path_I_J
    eval "layer_path_${i}_${p}=\"\$pval\""
  done
done

# ---------------------------------------------------------------------------
# Given a relative file path, return its layer number (empty = unclassified).
# ---------------------------------------------------------------------------
file_to_layer_num() {
  local file="$1"
  for (( i=0; i<layer_count; i++ )); do
    local num="${layer_nums[$i]}"
    local pcount="${layer_path_counts[$i]}"
    for (( p=0; p<pcount; p++ )); do
      eval "local pval=\"\${layer_path_${i}_${p}}\""
      case "$file" in
        $pval) echo "$num"; return 0 ;;
      esac
    done
  done
  echo ""
}

# ---------------------------------------------------------------------------
# Read forbidden pairs into parallel indexed arrays.
# ---------------------------------------------------------------------------
forbidden_count=$(yq eval '.forbidden | length' "$MANIFEST")
forbidden_from=()
forbidden_to=()
forbidden_reason=()

for (( i=0; i<forbidden_count; i++ )); do
  fval=$(yq eval ".forbidden[$i].from" "$MANIFEST")
  tval=$(yq eval ".forbidden[$i].to" "$MANIFEST")
  rval=$(yq eval ".forbidden[$i].reason" "$MANIFEST")
  forbidden_from+=("$fval")
  forbidden_to+=("$tval")
  forbidden_reason+=("$rval")
done

# ---------------------------------------------------------------------------
# Load debt file (known violations that are tolerated until deadline).
# Parallel arrays: debt_importers, debt_targets, debt_deadlines.
# ---------------------------------------------------------------------------
debt_importers=()
debt_targets=()
debt_deadlines=()

if [[ -f "$DEBT_FILE" ]]; then
  while IFS= read -r line; do
    # Skip comment lines and blank lines.
    [[ "$line" =~ ^[[:space:]]*# ]] && continue
    [[ -z "${line// }" ]] && continue
    # Strip trailing comment: everything after " # "
    local_stripped="${line%% # *}"
    importer="${local_stripped% → *}"
    # Handle both ASCII arrow ' → ' and the actual unicode arrow
    # The debt file uses the unicode arrow character →
    if [[ "$local_stripped" == *" → "* ]]; then
      importer="${local_stripped%% → *}"
      target="${local_stripped##* → }"
    else
      # Fallback: split on " -> "
      importer="${local_stripped%% -> *}"
      target="${local_stripped##* -> }"
    fi
    importer="${importer## }"
    importer="${importer%% }"
    target="${target## }"
    target="${target%% }"
    # Extract deadline from original line.
    deadline=""
    if [[ "$line" == *"deadline:"* ]]; then
      deadline="${line##*deadline: }"
      deadline="${deadline%%,*}"
      deadline="${deadline%% *}"
    fi
    debt_importers+=("$importer")
    debt_targets+=("$target")
    debt_deadlines+=("$deadline")
  done < "$DEBT_FILE"
fi

# Check if a pair is in the debt file. If deadline is past, return 2.
# Returns: 0 = in debt (future), 1 = not in debt, 2 = in debt but past deadline
# NOTE: uses 'di' as loop variable (not 'i') to avoid clobbering the caller's $i.
is_in_debt() {
  local importer="$1"
  local target="$2"
  local today
  today=$(date +%Y-%m-%d)
  local di
  for (( di=0; di<${#debt_importers[@]}; di++ )); do
    if [[ "${debt_importers[$di]}" == "$importer" && "${debt_targets[$di]}" == "$target" ]]; then
      local dl="${debt_deadlines[$di]}"
      if [[ -n "$dl" && "$dl" < "$today" ]]; then
        return 2
      fi
      return 0
    fi
  done
  return 1
}

# ---------------------------------------------------------------------------
# For a `use crate::X::...` line, return the first-matching source file path
# (best-effort — maps `crate::session::manager` to `src/session/manager.rs`
# or `src/session/manager/mod.rs`).
# Brace-group imports (`{TypeA, TypeB}`) can't be resolved; returns empty.
# ---------------------------------------------------------------------------
use_to_path() {
  local use_line="$1"
  # Extract everything after `use crate::` and before the terminating `;`
  # or end of line. Strip leading whitespace first.
  local stripped
  stripped=$(printf '%s' "$use_line" | sed -E 's/^[[:space:]]*use crate:://')
  stripped=$(printf '%s' "$stripped" | sed -E 's/;[[:space:]]*$//')

  # If this contains a brace group, we can't resolve it — skip.
  if [[ "$stripped" == *"{"* ]]; then
    echo ""
    return 0
  fi

  # Convert `::` separators to `/` path separators.
  local path
  path=$(printf '%s' "$stripped" | sed 's/::/\//g')

  # Try direct file path.
  if [[ -f "src/${path}.rs" ]]; then
    echo "src/${path}.rs"
    return 0
  fi
  # Try mod.rs directory form.
  if [[ -f "src/${path}/mod.rs" ]]; then
    echo "src/${path}/mod.rs"
    return 0
  fi
  # Strip last segment — handles `session::manager::SessionManager` where
  # `SessionManager` is a type exported from `session/manager.rs`.
  local stripped_path
  stripped_path=$(printf '%s' "$path" | rev | cut -d/ -f2- | rev)
  if [[ -n "$stripped_path" ]]; then
    if [[ -f "src/${stripped_path}.rs" ]]; then
      echo "src/${stripped_path}.rs"
      return 0
    fi
    if [[ -f "src/${stripped_path}/mod.rs" ]]; then
      echo "src/${stripped_path}/mod.rs"
      return 0
    fi
  fi

  echo ""  # can't resolve; silently ignored
}

# ---------------------------------------------------------------------------
# Main scan loop.
# ---------------------------------------------------------------------------
violations=0

while IFS= read -r -d '' src_file; do
  importer_rel="${src_file#./}"
  importer_layer=$(file_to_layer_num "$importer_rel")
  [[ -z "$importer_layer" ]] && continue

  while IFS= read -r use_line; do
    [[ -z "$use_line" ]] && continue
    target=$(use_to_path "$use_line")
    [[ -z "$target" ]] && continue
    target_layer=$(file_to_layer_num "$target")
    [[ -z "$target_layer" ]] && continue

    # Forbidden pair check takes precedence over generic layer ordering —
    # emit the more-specific FORBIDDEN message when both would apply.
    forbidden_matched=false
    for (( i=0; i<forbidden_count; i++ )); do
      from_glob="${forbidden_from[$i]}"
      to_glob="${forbidden_to[$i]}"
      if matches_any_glob "$importer_rel" "$from_glob" && matches_any_glob "$target" "$to_glob"; then
        forbidden_matched=true
        debt_status=1
        is_in_debt "$importer_rel" "$target" && debt_status=$? || debt_status=$?
        if (( debt_status == 0 )); then
          # Tolerated — future deadline.
          break
        elif (( debt_status == 2 )); then
          reason="${forbidden_reason[$i]}"
          echo "FORBIDDEN (DEADLINE PAST): $importer_rel → $target ($reason)"
          violations=$((violations + 1))
          break
        else
          reason="${forbidden_reason[$i]}"
          echo "FORBIDDEN: $importer_rel → $target ($reason)"
          violations=$((violations + 1))
          break
        fi
      fi
    done

    # If a forbidden pair already fired, skip the generic layer ordering check.
    $forbidden_matched && continue

    # Layer ordering check: importer (lower number = lower layer) MUST NOT
    # import from a higher layer number.
    if (( importer_layer < target_layer )); then
      debt_status=1
      is_in_debt "$importer_rel" "$target" && debt_status=$? || debt_status=$?
      if (( debt_status == 0 )); then
        # Tolerated — future deadline.
        continue
      elif (( debt_status == 2 )); then
        echo "VIOLATION (DEADLINE PAST): $importer_rel (layer $importer_layer) imports $target (layer $target_layer)"
        violations=$((violations + 1))
      else
        echo "VIOLATION: $importer_rel (layer $importer_layer) imports $target (layer $target_layer)"
        violations=$((violations + 1))
      fi
    fi
  done < <(grep -E '^[[:space:]]*use crate::' "$src_file" 2>/dev/null || true)

done < <(find src -name '*.rs' -print0 2>/dev/null)

if (( violations > 0 )); then
  echo ""
  echo "$violations violation(s) found."
  exit 1
fi

exit 0
