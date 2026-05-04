#!/usr/bin/env bash
# Emit a deterministic markdown issue summary from cached gh issue JSON.
#
# Usage:
#   scripts/condense-issue.sh /path/to/issue.json > /path/to/issue-summary.md

set -euo pipefail

issue_json="${1:-}"
if [ -z "$issue_json" ]; then
  echo "condense-issue: issue JSON path required" >&2
  exit 1
fi

python3 - "$issue_json" <<'PY'
import json
import re
import sys
from pathlib import Path

ORDER = [
    "Overview",
    "Expected Behavior",
    "Acceptance Criteria",
    "Blocked By",
    "Files to Modify",
]


def section_map(body):
    matches = list(
        re.finditer(r"(?m)^##[ \t]+(.+?)[ \t]*#*[ \t]*$", body or "")
    )
    sections = {}
    for index, match in enumerate(matches):
        title = match.group(1).strip()
        start = match.end()
        end = matches[index + 1].start() if index + 1 < len(matches) else len(body)
        sections[title] = body[start:end].strip()
    return sections


issue = json.loads(Path(sys.argv[1]).read_text())
title = str(issue.get("title") or "").strip()
sections = section_map(issue.get("body") or "")

parts = [f"# {title}"]
for name in ORDER:
    content = sections.get(name)
    if content is None:
        continue
    parts.append(f"## {name}\n\n{content.rstrip()}")

sys.stdout.write("\n\n".join(parts).rstrip() + "\n")
PY
