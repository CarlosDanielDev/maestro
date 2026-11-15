#!/usr/bin/env bats
# Every manifest scripts/release.sh bumps must also be staged for the
# release commit, or the tag ships the old version pins (#1305).

setup() {
  REPO_ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$REPO_ROOT/scripts/release.sh"
}

# Manifest paths named as `sed -i` targets on the version-bump lines
# (a bump line mentions ${VERSION}; targets may sit on a `\` continuation).
bumped_manifests() {
  awk '
    /sed -i/ && /\$\{VERSION\}/ { bump = 1 }
    bump {
      for (i = 1; i <= NF; i++) if ($i ~ /Cargo\.toml$/) print $i
      if ($0 !~ /\\$/) bump = 0
    }
  ' "$SCRIPT" | sort -u
}

staged_paths() {
  grep -E '^[[:space:]]*git add ' "$SCRIPT" | tr ' ' '\n' | sort -u
}

@test "release.sh bumps at least the root Cargo.toml" {
  run bumped_manifests
  [[ "$output" == *"Cargo.toml"* ]]
}

@test "every manifest release.sh bumps is staged for the release commit" {
  staged="$(staged_paths)"
  missing=""
  while IFS= read -r f; do
    [ -z "$f" ] && continue
    grep -qxF -- "$f" <<<"$staged" || missing="$missing $f"
  done < <(bumped_manifests)
  [ -z "$missing" ] || { echo "bumped but not staged:$missing"; false; }
}
