"""Unit tests for .claude/hooks/parse_gatekeeper_report.py."""
import importlib.util
import unittest
from pathlib import Path

HOOK_PATH = Path(__file__).resolve().parents[2] / ".claude" / "hooks" / "parse_gatekeeper_report.py"

def _load_parser():
    spec = importlib.util.spec_from_file_location("parse_gatekeeper_report", HOOK_PATH)
    if spec is None or spec.loader is None:
        raise FileNotFoundError(f"Parser not found at {HOOK_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

class ParserTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.parser = _load_parser()

    def test_parses_valid_pass_report(self):
        text = """
Prose above the fence.

```json gatekeeper
{
  "report_version": 1,
  "status": "PASS",
  "task_type": "implementation",
  "dor": {"passed": true, "missing_sections": [], "weak_sections": []},
  "blockers": {"passed": true, "open": []},
  "contracts": {"passed": true, "missing": []},
  "remediation": {"comment_body": "", "labels_to_add": []},
  "reasons": []
}
```

Prose below the fence.
"""
        report = self.parser.extract_report(text)
        self.assertEqual(report["status"], "PASS")
        self.assertEqual(report["task_type"], "implementation")
        self.assertTrue(report["dor"]["passed"])

    def test_parses_valid_fail_report_with_blockers(self):
        text = """
```json gatekeeper
{
  "report_version": 1,
  "status": "FAIL",
  "task_type": "implementation",
  "dor": {"passed": true, "missing_sections": [], "weak_sections": []},
  "blockers": {
    "passed": false,
    "open": [
      {"number": 42, "title": "upstream scaffolding", "state": "OPEN"},
      {"number": 43, "title": "prerequisite api", "state": "OPEN"}
    ]
  },
  "contracts": {"passed": true, "missing": []},
  "remediation": {"comment_body": "", "labels_to_add": []},
  "reasons": ["Blocker #42 is OPEN", "Blocker #43 is OPEN"]
}
```
"""
        report = self.parser.extract_report(text)
        self.assertEqual(report["status"], "FAIL")
        self.assertFalse(report["blockers"]["passed"])
        self.assertEqual(len(report["blockers"]["open"]), 2)
        self.assertEqual(report["blockers"]["open"][0]["number"], 42)

    def test_rejects_unclosed_fence(self):
        text = "Prose.\n```json gatekeeper\n{\"report_version\": 1}\n"  # no closing ```
        with self.assertRaises(self.parser.ParseError) as ctx:
            self.parser.extract_report(text)
        self.assertIn("no ```json gatekeeper fenced block found", str(ctx.exception))

    def test_rejects_malformed_json(self):
        text = """
```json gatekeeper
{"report_version": 1, "status": "PASS", "trailing": }
```
"""
        with self.assertRaises(self.parser.ParseError) as ctx:
            self.parser.extract_report(text)
        self.assertIn("malformed JSON", str(ctx.exception))

    def test_uses_first_fence_when_multiple(self):
        text = """
```json gatekeeper
{"report_version": 1, "status": "PASS", "task_type": "implementation",
 "dor": {"passed": true}, "blockers": {"passed": true},
 "contracts": {"passed": true}, "remediation": {}, "reasons": []}
```

Some prose.

```json gatekeeper
{"report_version": 1, "status": "FAIL", "task_type": "docs"}
```
"""
        report = self.parser.extract_report(text)
        self.assertEqual(report["status"], "PASS")  # first fence wins

    def test_rejects_missing_report_version(self):
        text = """
```json gatekeeper
{"status": "PASS", "task_type": "implementation"}
```
"""
        with self.assertRaises(self.parser.ParseError) as ctx:
            self.parser.extract_report(text)
        self.assertIn("missing required field: report_version", str(ctx.exception))

if __name__ == "__main__":
    unittest.main()
