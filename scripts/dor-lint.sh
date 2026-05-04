#!/usr/bin/env bash
# Fast mechanical DOR lint for /implement.
#
# Usage:
#   scripts/dor-lint.sh /path/to/issue.json
#
# Always exits 0. The verdict is written to dor-lint.json beside issue.json
# and echoed to stdout.

set -uo pipefail

issue_json="${1:-}"

if [ -z "$issue_json" ]; then
  printf '%s\n' '{"passed":false,"missing":[],"blockers":[],"blocker_states":{},"task_type":"trivial","reasons":["issue JSON path required"]}'
  exit 0
fi

python3 - "$issue_json" <<'PY'
import json
import re
import subprocess
import sys
from pathlib import Path


def empty_report(reason):
    return {
        "passed": False,
        "missing": [],
        "blockers": [],
        "blocker_states": {},
        "task_type": "trivial",
        "reasons": [reason],
    }


def label_names(issue):
    labels = issue.get("labels") or []
    names = []
    for label in labels:
        if isinstance(label, dict):
            name = label.get("name")
        else:
            name = str(label)
        if name:
            names.append(str(name).lower())
    return names


def task_type(labels):
    label_set = set(labels)
    if "bug" in label_set or "type:bug" in label_set:
        return "bug"
    if "documentation" in label_set or "type:docs" in label_set:
        return "docs"
    if "tech-debt" in label_set or "refactor" in label_set or "type:refactor" in label_set:
        return "refactor"
    if "enhancement" in label_set or "type:feature" in label_set:
        return "feature"
    return "trivial"


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


def blocker_numbers(blocked_by):
    numbers = []
    seen = set()
    for match in re.finditer(r"(?<![\w/.-])#(\d+)\b", blocked_by or ""):
        number = int(match.group(1))
        if number not in seen:
            seen.add(number)
            numbers.append(number)
    return numbers


def blocker_state(number):
    try:
        completed = subprocess.run(
            ["gh", "issue", "view", str(number), "--json", "state"],
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
    except OSError:
        return "OPEN", f"could not resolve blocker: #{number}"

    if completed.returncode != 0:
        return "OPEN", f"could not resolve blocker: #{number}"

    try:
        state = json.loads(completed.stdout).get("state", "OPEN")
    except json.JSONDecodeError:
        state = "OPEN"
    state = str(state).upper()
    if state not in {"OPEN", "CLOSED"}:
        state = "OPEN"
    return state, None


path = Path(sys.argv[1])
try:
    issue = json.loads(path.read_text())
except Exception as exc:  # noqa: BLE001 - linter must report, not fail.
    report = empty_report(f"could not read issue JSON: {exc}")
else:
    body = issue.get("body") or ""
    labels = label_names(issue)
    sections = section_map(body)
    required = [
        "Overview",
        "Expected Behavior",
        "Acceptance Criteria",
        "Blocked By",
        "Definition of Done",
    ]
    if "bug" in set(labels) or "type:bug" in set(labels):
        required.extend(["Current Behavior", "Steps to Reproduce"])

    missing = [name for name in required if name not in sections]
    reasons = [f"missing section: {name}" for name in missing]

    blockers = blocker_numbers(sections.get("Blocked By", ""))
    if re.search(r"\b[\w.-]+/[\w.-]+#\d+\b", sections.get("Blocked By", "")):
        reasons.append("cross-repo blocker requires gatekeeper")

    blocker_states = {}
    for number in blockers:
        state, reason = blocker_state(number)
        blocker_states[str(number)] = state
        if reason:
            reasons.append(reason)
        if state != "CLOSED":
            reasons.append(f"open blocker: #{number}")

    if "docs/api-contracts/" in body:
        reasons.append("contract validation required")

    report = {
        "passed": not reasons,
        "missing": missing,
        "blockers": blockers,
        "blocker_states": blocker_states,
        "task_type": task_type(labels),
        "reasons": reasons,
    }

out_path = path.with_name("dor-lint.json")
out_path.write_text(json.dumps(report, sort_keys=True, separators=(",", ":")) + "\n")
print(json.dumps(report, sort_keys=True, separators=(",", ":")))
PY

exit 0
