"""The CI gate must avoid skipping benchmarks when evidence is missing."""

import argparse
import io
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import wallclock


class WallclockTests(unittest.TestCase):
    def event(self, root):
        path = Path(root) / "event.json"
        path.write_text(json.dumps({"pull_request": {"number": 42, "title": "docs change", "body": "",
                                                       "head": {"sha": "a" * 40}}}))
        return path

    def test_clef_skips_only_for_confident_no_and_fails_open(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            env = {"GITHUB_EVENT_PATH": str(self.event(directory)), "GITHUB_OUTPUT": str(output),
                   "GITHUB_REPOSITORY": "oxc-project/tsrs", "GH_TOKEN": "fake",
                   "CLOUDFLARE_ACCOUNT_ID": "account", "CLOUDFLARE_API_KEY": "token"}
            files = [{"filename": "crates/tsrs_checker/src/checker.rs", "status": "modified",
                      "additions": 1, "deletions": 1, "patch": "@@ -1 +1 @@\n-old\n+new"},
                     {"filename": "docs/README.md", "status": "modified", "additions": 1, "deletions": 1}]
            with patch.dict(os.environ, env, clear=True), patch.object(wallclock, "pr_files", return_value=files), \
                    patch.object(wallclock, "clef_probabilities", return_value=(0.02, 0.04)) as clef:
                wallclock.decide()
            self.assertIn("run=false", output.read_text())
            self.assertEqual(clef.call_args.args[0]["files"], [
                {"path": files[0]["filename"], "status": "modified", "additions": 1, "deletions": 1,
                 "patch": files[0]["patch"]},
                {"path": "docs/README.md", "status": "modified", "additions": 1, "deletions": 1},
            ])
            self.assertIn("--noEmit", clef.call_args.args[0]["workload"]["command"])
            self.assertNotIn("title", clef.call_args.args[0])
            self.assertNotIn("body", clef.call_args.args[0])
            output.unlink()
            with patch.dict(os.environ, env, clear=True), patch.object(wallclock, "pr_files", return_value=files), \
                    patch.object(wallclock, "clef_probabilities", return_value=(0.02, 0.7)):
                wallclock.decide()
            self.assertIn("run=true", output.read_text())
            self.assertIn("70.0% peak-RSS", output.read_text())
            output.unlink()
            with patch.dict(os.environ, env, clear=True), patch.object(wallclock, "pr_files", return_value=files), \
                    patch.object(wallclock, "clef_probabilities", side_effect=TimeoutError):
                wallclock.decide()
            self.assertIn("run=true", output.read_text())

    def test_clef_request_and_noul_response(self):
        response = {"success": True, "result": {"answers": {
            "wall_time_impact": {"type": "noul", "noul": 0.8},
            "rss_impact": {"type": "noul", "noul": 0.3}}}}
        with patch.dict(os.environ, {"CLOUDFLARE_ACCOUNT_ID": "account", "CLOUDFLARE_API_KEY": "token"},
                        clear=True), patch.object(wallclock.urllib.request, "urlopen",
                                                  return_value=io.BytesIO(json.dumps(response).encode())) as call:
            self.assertEqual(wallclock.clef_probabilities({"files": []}), (0.8, 0.3))
        request = call.call_args.args[0]
        self.assertEqual(request.get_header("Authorization"), "Bearer token")
        questions = json.loads(request.data)["questions"]
        self.assertEqual(set(questions), {"wall_time_impact", "rss_impact"})
        self.assertTrue(all(question["type"] == "noul" for question in questions.values()))

    def test_incomplete_or_oversized_patch_runs_benchmarks(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            env = {"GITHUB_EVENT_PATH": str(self.event(directory)), "GITHUB_OUTPUT": str(output),
                   "GITHUB_REPOSITORY": "oxc-project/tsrs", "GH_TOKEN": "fake",
                   "CLOUDFLARE_ACCOUNT_ID": "account", "CLOUDFLARE_API_KEY": "token"}
            base = {"filename": "crates/tsrs_checker/src/checker.rs", "status": "modified",
                    "additions": 1, "deletions": 1}
            for patch_text in (None, "@@ -1 +1 @@\n-old", "@@ -1 +1 @@\n-old\n+" + "x" * 4096):
                with self.subTest(patch=patch_text is not None):
                    file = {**base, "patch": patch_text}
                    with patch.dict(os.environ, env, clear=True), \
                            patch.object(wallclock, "pr_files", return_value=[file]), \
                            patch.object(wallclock, "clef_probabilities") as clef:
                        wallclock.decide()
                    self.assertIn("run=true", output.read_text())
                    clef.assert_not_called()
                    output.unlink()

    def test_report_marks_slowdown_and_missing_projects(self):
        with tempfile.TemporaryDirectory() as directory:
            results = Path(directory)
            (results / "vscode.json").write_text(json.dumps({
                "project": "vscode",
                "main": {"median_s": 10.0, "peak_rss_bytes": 100 * 1048576},
                "pr": {"median_s": 11.0, "peak_rss_bytes": 110 * 1048576},
                "pair_ratios": [1.08, 1.1, 1.12], "timing_pairs": 3, "rss_runs": 3,
                "main_exit": 0, "pr_exit": 1, "diagnostics_match": False}))
            args = argparse.Namespace(run_url="https://github.com/oxc-project/tsrs/actions/runs/1",
                                      base_sha="a" * 40, head_sha="b" * 40, merge_sha="c" * 40,
                                      decision="true", identical="false", reason="",
                                      results=results)
            body = wallclock.render_report(args)
            self.assertIn("| vscode | 10.000 s | 11.000 s | +10.0% | +8.0% to +12.0% | "
                          "100.0 MiB | 110.0 MiB | +10.0% |", body)
            self.assertIn("Diagnostics or exit status differed on: vscode", body)
            self.assertIn("Measurements missing or invalid on:", body)

    def test_identical_binaries_skip_measurements(self):
        args = argparse.Namespace(run_url="https://github.com/oxc-project/tsrs/actions/runs/1",
                                  base_sha="a" * 40, head_sha="b" * 40, merge_sha="c" * 40,
                                  decision="true", identical="true", reason="Clef estimated 12.8% probability",
                                  results=Path("/nonexistent"))
        body = wallclock.render_report(args)
        self.assertIn("byte-for-byte identical", body)
        self.assertNotIn("Measurements missing", body)

    def test_timing_pairs_alternate_order_and_validate_exits(self):
        with tempfile.TemporaryDirectory() as directory:
            raw = Path(directory) / "hyperfine.json"
            calls = []

            def fake_hyperfine(argv, **kwargs):
                calls.append(argv)
                raw.write_text(json.dumps({"results": [{"times": [1.0]}, {"times": [2.0]}]}))

            with patch.object(wallclock.subprocess, "run", side_effect=fake_hyperfine):
                pairs = wallclock.timed_pairs(["/tmp/main", "-p", "project"], ["/tmp/pr", "-p", "project"],
                                               Path(directory), raw, 2, 1, 2)
            self.assertEqual(pairs, [{"main": 1.0, "pr": 2.0}, {"pr": 1.0, "main": 2.0}])
            self.assertIn('code=$?; [ "$code" -eq 1 ]', calls[0][-2])
            self.assertIn('code=$?; [ "$code" -eq 2 ]', calls[0][-1])
            self.assertIn("/tmp/pr", calls[1][-2])
            self.assertIn("/tmp/main", calls[1][-1])

    def test_peak_rss_reports_compiler_exit_status(self):
        with tempfile.TemporaryDirectory() as directory:
            raw = Path(directory) / "rss.json"
            rss = wallclock.peak_rss(["/bin/sh", "-c", "exit 2"], Path(directory), raw, 2)
            self.assertGreater(rss, 0)
            self.assertFalse(raw.exists())
            with self.assertRaises(RuntimeError):
                wallclock.peak_rss(["/bin/sh", "-c", "exit 3"], Path(directory), raw, 2)

    def test_comment_does_not_replace_newer_pr_results(self):
        args = argparse.Namespace(pr_number=42, head_sha="a" * 40)
        with patch.dict(os.environ, {"GH_TOKEN": "fake", "GITHUB_REPOSITORY": "oxc-project/tsrs"}), \
                patch.object(wallclock, "request_json", return_value={"head": {"sha": "b" * 40}}) as request:
            wallclock.comment(args)
        request.assert_called_once()

    def test_comment_updates_existing_bot_comment(self):
        args = argparse.Namespace(pr_number=42, head_sha="a" * 40, base_sha="b" * 40, merge_sha="c" * 40,
                                  run_url="https://github.com/oxc-project/tsrs/actions/runs/1",
                                  decision="false", identical="", reason="Clef estimated 2.0% probability of a wall-clock impact",
                                  results=Path("/tmp/unused"))
        replies = [{"head": {"sha": args.head_sha}},
                   [{"id": 17, "body": wallclock.MARKER + "\nold", "user": {"login": "github-actions[bot]"}}],
                   {}]
        with patch.dict(os.environ, {"GH_TOKEN": "fake", "GITHUB_REPOSITORY": "oxc-project/tsrs"}), \
                patch.object(wallclock, "request_json", side_effect=replies) as request:
            wallclock.comment(args)
        self.assertEqual(request.call_count, 3)
        self.assertEqual(request.call_args.kwargs["method"], "PATCH")
        self.assertIn("Clef gate: Clef estimated 2.0%", request.call_args.kwargs["data"]["body"])
        self.assertIn("Benchmarks skipped.", request.call_args.kwargs["data"]["body"])


if __name__ == "__main__":
    unittest.main()
