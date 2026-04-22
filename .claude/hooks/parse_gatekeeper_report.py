#!/usr/bin/env python3
"""Extract and validate gatekeeper JSON reports from subagent responses.

Usage:
    python3 parse_gatekeeper_report.py < input.txt
    echo "<text>" | python3 parse_gatekeeper_report.py

Exit codes:
    0 — valid report extracted, re-emitted as compact JSON on stdout
    1 — parse error (no fence, malformed JSON, wrong version)
"""
import json
import re
import sys

FENCE_PATTERN = re.compile(
    r"```json\s+gatekeeper\s*\n(.*?)\n```",
    re.DOTALL,
)

SUPPORTED_VERSION = 1


class ParseError(Exception):
    """Raised when the input cannot be parsed as a valid gatekeeper report."""


def extract_report(text: str) -> dict:
    """Extract the first ```json gatekeeper fenced block and parse as JSON."""
    match = FENCE_PATTERN.search(text)
    if not match:
        raise ParseError("no ```json gatekeeper fenced block found in input")

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
