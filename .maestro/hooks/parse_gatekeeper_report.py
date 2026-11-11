#!/usr/bin/env python3
"""Extract and validate gatekeeper JSON reports from subagent responses.

Usage:
    python3 parse_gatekeeper_report.py < input.txt
    echo "<text>" | python3 parse_gatekeeper_report.py

Exit codes:
    0 — valid report extracted, re-emitted as compact JSON on stdout
    1 — parse error (no fence, malformed JSON, wrong version, schema violation)
"""
import json
import re
import sys

# Two acceptable fence formats:
#   1. legacy:    ```json gatekeeper   (the original, kept for back-compat)
#   2. headered:  ## Gatekeeper        (Markdown header) followed by a plain
#                 ```json fence anywhere later in the same response.
FENCE_LEGACY = re.compile(r"```json\s+gatekeeper\s*\n(.*?)\n```", re.DOTALL)
FENCE_HEADERED = re.compile(
    r"##\s*Gatekeeper\b[^\n]*\n.*?```json\s*\n(.*?)\n```",
    re.DOTALL,
)

SUPPORTED_VERSION = 1

# Strict schema (issue #1038): the report drives orchestrator side effects
# (gh comments, issue labels, RED/GREEN skip logic), so every field that a
# downstream step reads is validated here. The parser is the first line of
# defense; scripts/gatekeeper-remediation.sh re-checks labels at the sink.
ALLOWED_STATUSES = frozenset({"PASS", "FAIL"})
ALLOWED_TASK_TYPES = frozenset({"implementation", "docs", "refactor"})
ALLOWED_LABELS = frozenset({"needs-info", "blocked", "needs-contract"})
LABEL_PATTERN = re.compile(r"^[a-z0-9][a-z0-9._-]{0,48}$")
COMMENT_BODY_MAX_LEN = 10000


class ParseError(Exception):
    """Raised when the input cannot be parsed as a valid gatekeeper report."""


def _validate_remediation_labels(remediation: dict) -> None:
    labels = remediation.get("labels_to_add")
    if labels is None:
        return
    if not isinstance(labels, list):
        raise ParseError("remediation.labels_to_add must be a list of strings")
    for label in labels:
        if not isinstance(label, str) or not LABEL_PATTERN.match(label):
            raise ParseError(
                f"invalid label {label!r}: must match {LABEL_PATTERN.pattern} "
                f"and be a string"
            )
        if label not in ALLOWED_LABELS:
            raise ParseError(
                f"label {label!r} is not in the allowlist {sorted(ALLOWED_LABELS)}"
            )


def _validate_remediation(remediation: object) -> None:
    if remediation is None:
        return
    if not isinstance(remediation, dict):
        raise ParseError("remediation must be a JSON object")
    _validate_remediation_labels(remediation)
    comment_body = remediation.get("comment_body")
    if comment_body is None:
        return
    if not isinstance(comment_body, str):
        raise ParseError("remediation.comment_body must be a string")
    if len(comment_body) > COMMENT_BODY_MAX_LEN:
        raise ParseError(
            f"remediation.comment_body exceeds {COMMENT_BODY_MAX_LEN} characters"
        )


def extract_report(text: str) -> dict:
    """Extract the first matching gatekeeper fence, parse and validate it.

    Accepts either the legacy ```json gatekeeper fence or a plain ```json
    fence preceded somewhere by a `## Gatekeeper` header.
    """
    match = FENCE_LEGACY.search(text) or FENCE_HEADERED.search(text)
    if not match:
        raise ParseError(
            "no gatekeeper fence found — expected either "
            "```json gatekeeper or `## Gatekeeper` header followed by ```json"
        )

    content = match.group(1)
    try:
        report = json.loads(content)
    except json.JSONDecodeError as exc:
        raise ParseError(f"malformed JSON in gatekeeper fence: {exc}") from exc

    if not isinstance(report, dict):
        raise ParseError("gatekeeper report must be a JSON object")

    version = report.get("report_version")
    if version is None:
        raise ParseError("gatekeeper report missing required field: report_version")
    if version != SUPPORTED_VERSION:
        raise ParseError(
            f"unsupported report_version: {version} "
            f"(this parser supports {SUPPORTED_VERSION})"
        )

    status = report.get("status")
    if status not in ALLOWED_STATUSES:
        raise ParseError(
            f"status must be one of {sorted(ALLOWED_STATUSES)}, got {status!r}"
        )

    task_type = report.get("task_type")
    if task_type not in ALLOWED_TASK_TYPES:
        raise ParseError(
            f"task_type must be one of {sorted(ALLOWED_TASK_TYPES)}, got {task_type!r}"
        )

    dor = report.get("dor")
    if not isinstance(dor, dict):
        raise ParseError("dor must be a JSON object")
    if not isinstance(dor.get("passed"), bool):
        raise ParseError("dor.passed must be a boolean")

    remediation = report.get("remediation")
    _validate_remediation(remediation)

    if status == "FAIL" and dor["passed"] is False:
        body = (remediation or {}).get("comment_body")
        if not body:
            raise ParseError(
                "status 'FAIL' with dor.passed false requires a non-empty "
                "remediation.comment_body"
            )

    return report


def main() -> int:
    text = sys.stdin.read()
    try:
        report = extract_report(text)
    except ParseError as exc:
        print(f"parse-gatekeeper-report: {exc}", file=sys.stderr)
        return 1

    json.dump(report, sys.stdout)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
