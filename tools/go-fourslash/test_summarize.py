import contextlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import summarize


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
        self.assertEqual(result.returncode, 1)
        self.assertIn("incomplete results", result.stdout)


class KnownMismatchTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.results = self.root / "results.jsonl"
        self.known = self.root / "known.json"
        self.baselines = self.root / "baselines"
        self.reference = self.baselines / "reference" / "fourslash" / "example.baseline.jsonc"
        self.actual = self.baselines / "local" / "fourslash" / "example.baseline.jsonc"
        for path, text in ((self.reference, "expected\n"), (self.actual, "actual\n")):
            path.parent.mkdir(parents=True)
            path.write_text(text)
        self.events = [event("pass", "TestGood"), event("skip", "TestSkipped"),
                       dict(event("output", "TestKnown"), Output=f"    baseline.go:79: the baseline file {self.reference} "
                            "has changed. (Run `hereby baseline-accept` if the new baseline is correct.)\n"),
                       event("fail", "TestKnown"), event("fail")]
        self.write_results()
        with contextlib.redirect_stdout(io.StringIO()):
            snapshot, _, _ = summarize.inspect(self.results, self.baselines)
        commit = summarize.re.search(r'(?m)^commit = "([a-f0-9]{40})"$',
                                     (summarize.ROOT / "Cargo.toml").read_text())[1]
        self.inventory = {"schema_version": 1, "reference_commit": commit, **snapshot}
        self.known.write_text(json.dumps(self.inventory))

    def write_results(self):
        self.results.write_text("".join(json.dumps(item) + "\n" for item in self.events))

    def run_gate(self, status=1):
        return subprocess.run([sys.executable, str(SCRIPT), str(self.results), "--known", str(self.known),
                               "--baselines", str(self.baselines), "--runner-exit", str(status)],
                              capture_output=True, text=True)

    def test_known_failure_stays_visible_without_becoming_a_pass(self):
        result = self.run_gate()
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("Passing: 1; failing: 1; skipped: 1", result.stdout)
        self.assertIn("known conformance gap", (self.root / "report.md").read_text())
        self.assertTrue(Path(str(self.actual) + ".diff").is_file())

    def test_changed_or_missing_baseline_is_rejected(self):
        self.actual.write_text("actual\nnew difference\n")
        self.assertEqual(self.run_gate().returncode, 1)
        self.actual.unlink()
        self.assertEqual(self.run_gate().returncode, 1)

    def test_fixed_entry_requires_reviewed_removal(self):
        self.events = [event("pass", "TestGood"), event("skip", "TestSkipped"),
                       event("pass", "TestKnown"), event("pass")]
        self.write_results()
        self.assertEqual(self.run_gate(status=0).returncode, 1)
        self.inventory["mismatches"] = []
        self.known.write_text(json.dumps(self.inventory))
        self.assertEqual(self.run_gate(status=0).returncode, 0)

    def test_new_failure_cannot_be_hidden_by_a_known_failure(self):
        self.events[0] = event("fail", "TestGood")
        self.write_results()
        self.assertEqual(self.run_gate().returncode, 1)

    def test_unknown_assertions_and_panics_in_a_known_case_fail(self):
        for text in ("    assertion.go:1: new failure\n", "panic: new failure\n"):
            with self.subTest(text=text):
                self.events.insert(3, dict(event("output", "TestKnown"), Output=text))
                self.write_results()
                self.assertEqual(self.run_gate().returncode, 1)
                self.events.pop(3)

    def test_missing_replaced_or_skipped_coverage_fails(self):
        for first in (event("skip", "TestGood"), event("pass", "TestReplacement")):
            self.events[0] = first
            self.write_results()
            self.assertEqual(self.run_gate().returncode, 1)
        self.events.pop(0)
        self.write_results()
        self.assertEqual(self.run_gate().returncode, 1)

    def test_incomplete_duplicate_or_wrong_exit_fails(self):
        for status in (0, 2, 101):
            self.assertEqual(self.run_gate(status=status).returncode, 1)
        self.events.insert(0, event("pass", "TestGood"))
        self.write_results()
        self.assertEqual(self.run_gate().returncode, 1)
        self.events.pop(0)
        self.events.pop()
        self.write_results()
        self.assertEqual(self.run_gate().returncode, 1)

    def test_changed_pin_is_rejected(self):
        self.inventory["reference_commit"] = "0" * 40
        self.known.write_text(json.dumps(self.inventory))
        self.assertEqual(self.run_gate().returncode, 1)


if __name__ == "__main__":
    unittest.main()
