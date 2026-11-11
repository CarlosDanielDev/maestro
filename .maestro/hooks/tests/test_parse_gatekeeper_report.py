#!/usr/bin/env python3
"""Tests for parse_gatekeeper_report.py — fence formats and strict schema.

Run with: python3 -m unittest .maestro/hooks/tests/test_parse_gatekeeper_report.py
"""
import copy
import json
import os
import subprocess
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from parse_gatekeeper_report import extract_report, ParseError  # noqa: E402

HOOK_PATH = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "parse_gatekeeper_report.py",
)

VALID_REPORT = {
    "report_version": 1,
    "status": "PASS",
    "task_type": "implementation",
    "dor": {"passed": True},
    "blockers": {"passed": True, "open": []},
    "contracts": {"passed": True, "missing": []},
    "remediation": {"comment_body": "", "labels_to_add": []},
    "reasons": [],
}


def _report(**overrides):
    report = copy.deepcopy(VALID_REPORT)
    report.update(overrides)
    return report


def _legacy_fence(payload: dict) -> str:
    return f"```json gatekeeper\n{json.dumps(payload)}\n```"


def _headered_fence(payload: dict) -> str:
    return f"## Gatekeeper\n\nSome prose.\n\n```json\n{json.dumps(payload)}\n```"


class TestLegacyFence(unittest.TestCase):
    def test_parses_cleanly(self):
        report = extract_report(_legacy_fence(_report()))
        self.assertEqual(report["status"], "PASS")
        self.assertEqual(report["report_version"], 1)

    def test_with_surrounding_prose(self):
        text = "Analysis follows.\n\n" + _legacy_fence(_report()) + "\n\nDone."
        report = extract_report(text)
        self.assertEqual(report["status"], "PASS")


class TestHeaderedFence(unittest.TestCase):
    def test_parses_cleanly(self):
        report = extract_report(_headered_fence(_report()))
        self.assertEqual(report["status"], "PASS")
        self.assertEqual(report["report_version"], 1)

    def test_with_surrounding_prose(self):
        text = "Pre-text.\n\n" + _headered_fence(_report()) + "\n\nPost-text."
        report = extract_report(text)
        self.assertEqual(report["status"], "PASS")


class TestErrorCases(unittest.TestCase):
    def test_no_fence_raises(self):
        with self.assertRaises(ParseError):
            extract_report("no fence here at all")

    def test_malformed_json_raises(self):
        with self.assertRaises(ParseError):
            extract_report("```json gatekeeper\nnot-valid-json\n```")

    def test_unsupported_version_raises(self):
        bad = _report(report_version=99)
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(bad))

    def test_missing_version_raises(self):
        no_version = _report()
        del no_version["report_version"]
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(no_version))


class TestStrictSchema(unittest.TestCase):
    """status/task_type/dor/labels/comment_body are schema-checked (issue #1038)."""

    def test_rejects_status_outside_allowlist(self):
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(_report(status="SKIPPED")))

    def test_rejects_missing_status(self):
        payload = _report()
        del payload["status"]
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(payload))

    def test_rejects_unknown_task_type(self):
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(_report(task_type="chore")))

    def test_rejects_missing_task_type(self):
        payload = _report()
        del payload["task_type"]
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(payload))

    def test_rejects_missing_dor(self):
        payload = _report()
        del payload["dor"]
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(payload))

    def test_rejects_non_bool_dor_passed(self):
        payload = _report(dor={"passed": "true"})
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(payload))

    def test_rejects_flag_like_label(self):
        payload = _report(remediation={"comment_body": "", "labels_to_add": ["--add-assignee"]})
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(payload))

    def test_rejects_label_with_word_split_injection(self):
        payload = _report(
            remediation={"comment_body": "", "labels_to_add": ["needs-info --add-assignee evil"]}
        )
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(payload))

    def test_rejects_label_matching_regex_but_outside_allowlist(self):
        payload = _report(remediation={"comment_body": "", "labels_to_add": ["wip"]})
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(payload))

    def test_rejects_non_string_label(self):
        payload = _report(remediation={"comment_body": "", "labels_to_add": [3]})
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(payload))

    def test_rejects_non_list_labels_to_add(self):
        payload = _report(remediation={"comment_body": "", "labels_to_add": "needs-info"})
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(payload))

    def test_accepts_allowlisted_labels(self):
        payload = _report(
            remediation={"comment_body": "please fix", "labels_to_add": ["needs-info"]}
        )
        report = extract_report(_legacy_fence(payload))
        self.assertEqual(report["remediation"]["labels_to_add"], ["needs-info"])

    def test_rejects_oversized_comment_body(self):
        payload = _report(remediation={"comment_body": "x" * 10001, "labels_to_add": []})
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(payload))

    def test_accepts_comment_body_at_cap(self):
        payload = _report(remediation={"comment_body": "x" * 10000, "labels_to_add": []})
        report = extract_report(_legacy_fence(payload))
        self.assertEqual(len(report["remediation"]["comment_body"]), 10000)

    def test_rejects_fail_with_dor_false_and_empty_comment_body(self):
        payload = _report(
            status="FAIL",
            dor={"passed": False},
            remediation={"comment_body": "", "labels_to_add": []},
        )
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(payload))

    def test_accepts_fail_with_dor_false_and_remediation_comment(self):
        payload = _report(
            status="FAIL",
            dor={"passed": False},
            remediation={"comment_body": "missing Overview section", "labels_to_add": ["needs-info"]},
            reasons=["Acceptance Criteria missing"],
        )
        report = extract_report(_legacy_fence(payload))
        self.assertEqual(report["status"], "FAIL")


class CliTests(unittest.TestCase):
    def _run(self, stdin_text):
        return subprocess.run(
            [sys.executable, HOOK_PATH],
            input=stdin_text,
            capture_output=True,
            text=True,
        )

    def test_cli_roundtrips_valid_report(self):
        result = self._run(_legacy_fence(_report()))
        self.assertEqual(result.returncode, 0)
        parsed = json.loads(result.stdout)
        self.assertEqual(parsed["status"], "PASS")

    def test_cli_rejects_injection_labels(self):
        # Exact fixture from issue #1038: flag-like and non-allowlisted labels
        # must make the parser exit non-zero, not pass through unchanged.
        result = self._run(
            _legacy_fence({"report_version": 1, "remediation": {"labels_to_add": ["--add-assignee", "x"]}})
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("parse-gatekeeper-report", result.stderr)

    def test_cli_reports_error_on_malformed_input(self):
        result = self._run("no fence here")
        self.assertEqual(result.returncode, 1)
        self.assertIn("no gatekeeper fence found", result.stderr)


if __name__ == "__main__":
    unittest.main()
