import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("summarize.py")


def event(action, test=None):
    result = {"Action": action, "Package": "fourslash"}
    if test is not None:
        result["Test"] = test
    return result


class CoverageGateTests(unittest.TestCase):
    def summarize(self, events, minimum=1):
        with tempfile.TemporaryDirectory() as directory:
            results = Path(directory, "results.jsonl")
            results.write_text("".join(json.dumps(item) + "\n" for item in events))
            return subprocess.run(
                [sys.executable, str(SCRIPT), str(results), "--min-pass", str(minimum)],
                capture_output=True, text=True,
            )

    def test_pass_floor(self):
        events = [event("pass", "TestA"), event("skip", "TestB"), event("pass")]
        self.assertEqual(self.summarize(events, minimum=1).returncode, 0)
        result = self.summarize(events, minimum=2)
        self.assertEqual(result.returncode, 1)
        self.assertIn("minimum 2", result.stdout)

    def test_skipped_only_parents_do_not_satisfy_floor(self):
        for child in ("TestA/child", "TestA/nested/child"):
            with self.subTest(child=child):
                events = [event("skip", child)]
                if child.count("/") == 2:
                    events.append(event("pass", "TestA/nested"))
                events.extend([event("pass", "TestA"), event("pass")])
                result = self.summarize(events)
                self.assertEqual(result.returncode, 1)
                self.assertIn("pass=0", result.stdout)

    def test_mixed_subtests_count_as_one_exercised_group(self):
        events = [event("pass", "TestA/a"), event("skip", "TestA/b"), event("pass", "TestA"), event("pass")]
        self.assertEqual(self.summarize(events).returncode, 0)
        self.assertEqual(self.summarize(events, minimum=2).returncode, 1)

    def test_failures_cannot_be_offset_by_passing_groups(self):
        for failure in ([event("fail", "TestB")], [event("fail", "TestB/child"), event("fail", "TestB")]):
            with self.subTest(failure=failure):
                events = [event("pass", "TestA"), *failure, event("fail")]
                self.assertEqual(self.summarize(events).returncode, 1)

    def test_package_failure_and_skip_fail_even_with_zero_floor(self):
        for action in ("fail", "skip"):
            with self.subTest(action=action):
                self.assertEqual(self.summarize([event(action)], minimum=0).returncode, 1)

    def test_incomplete_results_fail(self):
        result = self.summarize([event("pass", "TestA")])
        self.assertEqual(result.returncode, 2)
        self.assertIn("incomplete results", result.stderr)


if __name__ == "__main__":
    unittest.main()
