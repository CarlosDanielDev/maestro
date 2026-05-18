#!/usr/bin/env python3
"""Extract and validate idea-triager JSON reports from subagent responses.

Usage:
    python3 parse_idea_triager_report.py < input.txt
    echo "<text>" | python3 parse_idea_triager_report.py

Exit codes:
    0 — valid report extracted, re-emitted as compact JSON on stdout
    1 — parse error (no fence, malformed JSON, wrong version, schema violation)
"""
import json
import re
import sys

FENCE_PATTERN = re.compile(
    r"```json\s+idea-triager\s*\n(.*?)\n```",
    re.DOTALL,
)

SUPPORTED_VERSION = 1
RECOMMENDATIONS = {"promote", "park", "archive"}
RECOMMENDATIONS_REQUIRING_REMEDIATION = frozenset({"park", "archive"})
VERDICTS = {"pass", "weak", "fail"}
CHECK_KEYS = (
    "whose_problem",
    "smallest_proof",
    "success_signal",
    "cost_of_skipping",
    "vision_alignment",
)


class ParseError(Exception):
    """Raised when the input cannot be parsed as a valid triager report."""


def extract_report(text: str) -> dict:
    match = FENCE_PATTERN.search(text)
    if not match:
        raise ParseError("no ```json idea-triager fenced block found in input")

    try:
        report = json.loads(match.group(1))
    except json.JSONDecodeError as exc:
        raise ParseError(f"malformed JSON in idea-triager fence: {exc}") from exc

    if not isinstance(report, dict):
        raise ParseError("idea-triager report must be a JSON object")

    version = report.get("report_version")
    if version != SUPPORTED_VERSION:
        raise ParseError(
            f"unsupported report_version: {version!r} "
            f"(this parser supports {SUPPORTED_VERSION})"
        )

    recommendation = report.get("recommendation")
    if recommendation not in RECOMMENDATIONS:
        raise ParseError(
            f"recommendation must be one of {sorted(RECOMMENDATIONS)}, "
            f"got {recommendation!r}"
        )

    checks = report.get("checks")
    if not isinstance(checks, dict):
        raise ParseError("checks must be a JSON object")
    for key in CHECK_KEYS:
        check = checks.get(key)
        if not isinstance(check, dict):
            raise ParseError(f"checks.{key} missing or not an object")
        verdict = check.get("verdict")
        if verdict not in VERDICTS:
            raise ParseError(
                f"checks.{key}.verdict must be one of {sorted(VERDICTS)}, "
                f"got {verdict!r}"
            )

    if recommendation == "promote" and "spike_proposal" not in report:
        raise ParseError("recommendation 'promote' requires spike_proposal")

    if recommendation in RECOMMENDATIONS_REQUIRING_REMEDIATION:
        remediation = report.get("remediation") or {}
        if not remediation.get("comment_body"):
            raise ParseError(
                f"recommendation '{recommendation}' requires "
                "remediation.comment_body"
            )

    return report


def main() -> int:
    text = sys.stdin.read()
    try:
        report = extract_report(text)
    except ParseError as exc:
        print(f"parse-idea-triager-report: {exc}", file=sys.stderr)
        return 1

    json.dump(report, sys.stdout)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
