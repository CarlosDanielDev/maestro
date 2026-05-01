#!/usr/bin/env python3
"""Tests for parse_gatekeeper_report.py — both legacy and new fence formats.

Run with: python3 -m unittest .claude/hooks/tests/test_parse_gatekeeper_report.py
"""
import json
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from parse_gatekeeper_report import extract_report, ParseError  # noqa: E402

MINIMAL_REPORT = {"report_version": 1, "status": "PASS"}


def _legacy_fence(payload: dict) -> str:
    return f"```json gatekeeper\n{json.dumps(payload)}\n```"


def _headered_fence(payload: dict) -> str:
    return f"## Gatekeeper\n\nSome prose.\n\n```json\n{json.dumps(payload)}\n```"


class TestLegacyFence(unittest.TestCase):
    def test_parses_cleanly(self):
        report = extract_report(_legacy_fence(MINIMAL_REPORT))
        self.assertEqual(report["status"], "PASS")
        self.assertEqual(report["report_version"], 1)

    def test_with_surrounding_prose(self):
        text = "Analysis follows.\n\n" + _legacy_fence(MINIMAL_REPORT) + "\n\nDone."
        report = extract_report(text)
        self.assertEqual(report["status"], "PASS")


class TestHeaderedFence(unittest.TestCase):
    def test_parses_cleanly(self):
        report = extract_report(_headered_fence(MINIMAL_REPORT))
        self.assertEqual(report["status"], "PASS")
        self.assertEqual(report["report_version"], 1)

    def test_with_surrounding_prose(self):
        text = "Pre-text.\n\n" + _headered_fence(MINIMAL_REPORT) + "\n\nPost-text."
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
        bad = dict(MINIMAL_REPORT, report_version=99)
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(bad))

    def test_missing_version_raises(self):
        no_version = {"status": "PASS"}
        with self.assertRaises(ParseError):
            extract_report(_legacy_fence(no_version))


if __name__ == "__main__":
    unittest.main()
