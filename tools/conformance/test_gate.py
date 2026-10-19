import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import gate


class ConformanceGateTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.results = self.root / "results"
        self.results.mkdir()
        self.known = self.root / "known.json"
        self.records = [self.passing("compiler/good"), self.passing("conformance/example")]
        self.env = {key: value for key, value in os.environ.items() if key not in ("CI", "GITHUB_STEP_SUMMARY")}

    @staticmethod
    def passing(name):
        return {"id": name, "class": "pass", **{baseline: "pass" for baseline in gate.BASELINES[1:]}}

    def write_results(self):
        gate.write_json(self.results / "invocation.json", {
            "mode": "go", "reference_commit": gate.reference_commit(), "exit_code": 0,
        })
        gate.write_json(self.results / "run.json", {
            "complete": True, "mode": "go", "baselines": list(gate.BASELINES),
            "selected": [record["id"] for record in self.records],
            "excluded": {"compiler/APILibCheck.ts": "requires built typescript.d.ts"},
        })
        summary = {"tests": self.records}
        for baseline in gate.BASELINES:
            field = "class" if baseline == "errors" else baseline
            counts = {}
            for kind in gate.CLASSES:
                names = sorted(record["id"] for record in self.records if record.get(field) == kind)
                counts[kind] = len(names)
                filename = f"{kind}.txt" if baseline == "errors" else f"{baseline}-{kind}.txt"
                (self.results / filename).write_text("".join(name + "\n" for name in names))
            summary["totals" if baseline == "errors" else f"{baseline}_totals"] = {"all": counts}
        gate.write_json(self.results / "summary.json", summary)

    def mismatch(self, index=1, baseline="errors", kind="fail", diff="-expected\n+actual\n"):
        record = self.records[index]
        record["class" if baseline == "errors" else baseline] = kind
        record["diff" if baseline == "errors" else f"{baseline}_diff"] = "L1: -expected +actual"
        stem = self.results / record["id"]
        stem.parent.mkdir(parents=True, exist_ok=True)
        Path(f"{stem}.diff" if baseline == "errors" else f"{stem}.{baseline}.diff").write_text(diff)
        Path(f"{stem}.actual.txt" if baseline == "errors" else f"{stem}.{baseline}.actual").write_text("actual\n")

    def invoke(self, command="check", **env):
        return subprocess.run(
            [sys.executable, str(Path(gate.__file__)), command, "--results", str(self.results), "--known", str(self.known)],
            capture_output=True, text=True, env={**self.env, **env},
        )

    def seed(self):
        self.write_results()
        result = self.invoke("refresh")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def assert_rejected(self, text):
        result = self.invoke()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(text, result.stdout)

    def test_all_baselines_pass_and_exclusions_are_reported(self):
        self.seed()
        before = self.known.read_bytes()
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("compiler/APILibCheck.ts", result.stdout)
        self.assertEqual(self.known.read_bytes(), before)

    def test_known_mismatches_are_visible_for_every_baseline(self):
        for baseline in gate.BASELINES:
            self.mismatch(baseline=baseline, kind="codes" if baseline == "errors" else "fail")
        self.seed()
        summary = self.root / "step-summary.md"
        result = self.invoke(GITHUB_STEP_SUMMARY=str(summary))
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertEqual(result.stdout.count("**known**"), 6)
        self.assertIn("js.map", summary.read_text())
        self.assertIn("sourcemap.txt", summary.read_text())
        self.assertIn("conformance gaps", summary.read_text())

    def test_a_new_failure_cannot_be_offset_by_a_new_pass(self):
        self.mismatch()
        self.seed()
        self.records[1] = self.passing("conformance/example")
        self.mismatch(index=0)
        self.write_results()
        self.assert_rejected("new: compiler/good")

    def test_full_diff_changes_are_detected_with_the_same_first_difference(self):
        self.mismatch(baseline="js")
        self.seed()
        self.mismatch(baseline="js", diff="-expected\n+actual\n-another line\n+new regression\n")
        self.write_results()
        self.assert_rejected("changed: conformance/example")

    def test_resolved_entry_requires_removal(self):
        self.mismatch()
        self.seed()
        self.records[1] = self.passing("conformance/example")
        self.write_results()
        self.assert_rejected("resolved or no longer exercised")
        self.assertEqual(self.invoke("refresh").returncode, 0)
        self.assertEqual(self.invoke().returncode, 0)
        self.assertEqual(gate.read_json(self.known)["modes"]["go"]["mismatches"], [])

    def test_new_skip_and_missing_case_fail_coverage(self):
        self.seed()
        self.records[0]["js.map"] = "skip"
        self.records[0]["js.map_diff"] = "no source maps"
        self.write_results()
        self.assert_rejected("coverage changed: exercised")
        self.records.pop(0)
        self.write_results()
        self.assert_rejected("coverage changed: selected")

    def test_equal_case_counts_do_not_hide_replaced_cases(self):
        self.seed()
        self.records[0] = self.passing("compiler/replacement")
        self.write_results()
        self.assert_rejected("coverage changed: selected")

    def test_skip_reasons_and_fixture_exclusions_are_tracked(self):
        self.records[0]["js.map"] = "skip"
        self.records[0]["js.map_diff"] = "no source maps"
        self.seed()
        self.records[0]["js.map_diff"] = "new unsupported case"
        self.write_results()
        self.assert_rejected("coverage changed: skipped")
        manifest = gate.read_json(self.results / "run.json")
        manifest["excluded"]["compiler/new.ts"] = "unsupported"
        gate.write_json(self.results / "run.json", manifest)
        self.assert_rejected("coverage changed: excluded")

    def test_crashes_and_timeouts_cannot_be_recorded(self):
        self.seed()
        for baseline in gate.BASELINES:
            for kind in ("crash", "timeout"):
                with self.subTest(baseline=baseline, kind=kind):
                    self.records[1] = self.passing("conformance/example")
                    self.records[1]["class" if baseline == "errors" else baseline] = kind
                    self.write_results()
                    self.assert_rejected(f"{kind}: conformance/example")
                    before = self.known.read_bytes()
                    self.assertNotEqual(self.invoke("refresh").returncode, 0)
                    self.assertEqual(self.known.read_bytes(), before)

    def test_missing_artifacts_or_lists_fail(self):
        self.mismatch(baseline="symbols")
        self.seed()
        for name in ("conformance/example.symbols.actual", "conformance/example.symbols.diff", "js.map-pass.txt"):
            path = self.results / name
            contents = path.read_bytes()
            path.unlink()
            self.assertNotEqual(self.invoke().returncode, 0, name)
            path.write_bytes(contents)

    def test_incomplete_or_wrong_run_cannot_satisfy_gate(self):
        self.seed()
        manifest = gate.read_json(self.results / "run.json")
        for change in ({"complete": False}, {"mode": "default"}, {"baselines": ["errors", "types", "symbols"]},
                       {"selected": []}, {"selected": ["compiler/good"]},
                       {"selected": ["compiler/good", "compiler/good"]}):
            with self.subTest(change=change):
                gate.write_json(self.results / "run.json", {**manifest, **change})
                self.assertNotEqual(self.invoke().returncode, 0)
        gate.write_json(self.results / "run.json", manifest)
        (self.results / "run.json").unlink()
        self.assertNotEqual(self.invoke().returncode, 0)

    def test_missing_baseline_and_inconsistent_lists_fail(self):
        self.seed()
        del self.records[0]["sourcemap.txt"]
        self.write_results()
        self.assert_rejected("missing or invalid sourcemap.txt result")
        self.records[0]["sourcemap.txt"] = "pass"
        self.write_results()
        (self.results / "types-pass.txt").write_text("")
        self.assert_rejected("inconsistent case list")

    def test_failed_runner_and_wrong_reference_fail(self):
        self.seed()
        invocation = gate.read_json(self.results / "invocation.json")
        for change in ({"exit_code": 1}, {"reference_commit": "0" * 40}):
            gate.write_json(self.results / "invocation.json", {**invocation, **change})
            self.assert_rejected("runner failed")

    def test_invalid_inventory_is_rejected(self):
        self.mismatch()
        self.seed()
        original = gate.read_json(self.known)
        known = copy.deepcopy(original)
        known["modes"]["go"]["mismatches"] *= 2
        gate.write_json(self.known, known)
        self.assert_rejected("duplicate mismatch")
        known = copy.deepcopy(original)
        known["reference_commit"] = "0" * 40
        gate.write_json(self.known, known)
        self.assert_rejected("different upstream commit")
        self.known.write_text('{"modes": {}, "modes": {}}')
        self.assert_rejected("duplicate JSON key")

    def test_ci_cannot_refresh_the_inventory(self):
        self.seed()
        before = self.known.read_bytes()
        result = self.invoke("refresh", CI="true")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("CI cannot update", result.stdout)
        self.assertEqual(self.known.read_bytes(), before)

    def test_run_selects_full_baselines_and_overrides_ambient_mode(self):
        self.seed()
        source = self.root / "source"
        source.mkdir()
        for path in self.results.iterdir():
            if path.is_file():
                (source / path.name).write_bytes(path.read_bytes())
        fake_runner = self.root / "runner"
        fake_runner.write_text(
            f"#!{sys.executable}\n"
            "import os, pathlib, shutil, sys\n"
            "assert sys.argv[1:] == ['run', '--suite', 'all', '--baselines', "
            f"{gate.FLAGS!r}, '--timeout', '121']\n"
            "assert os.environ['TSRS_HISTORY'] == 'go'\n"
            "assert os.environ['TS_TEST_PROGRAM_SINGLE_THREADED'] == 'true'\n"
            "assert 'TSRS_CHECKER_ASSIGNMENT' not in os.environ\n"
            f"shutil.copytree({str(source)!r}, os.environ['TSRS_TEST_RESULTS'], dirs_exist_ok=True)\n"
        )
        fake_runner.chmod(0o755)
        result = self.invoke("run", TSRS_TEST=str(fake_runner), TSRS_HISTORY="canonical",
                             TSRS_CHECKER_ASSIGNMENT="locality", TEST_TIMEOUT="121")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_successful_runner_without_results_cannot_reuse_stale_output(self):
        self.seed()
        fake_runner = self.root / "empty-runner"
        fake_runner.write_text(f"#!{sys.executable}\n")
        fake_runner.chmod(0o755)
        result = self.invoke("run", TSRS_TEST=str(fake_runner))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("run.json", result.stdout)


if __name__ == "__main__":
    unittest.main()
