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
            files = [{"filename": "README.md", "status": "modified", "patch": "@@ -1 +1 @@\n-old\n+new"}]
            with patch.dict(os.environ, env, clear=True), patch.object(wallclock, "pr_files", return_value=files), \
                    patch.object(wallclock, "clef_probability", return_value=0.02):
                wallclock.decide()
            self.assertIn("run=false", output.read_text())
            output.unlink()
            with patch.dict(os.environ, env, clear=True), patch.object(wallclock, "pr_files", return_value=files), \
                    patch.object(wallclock, "clef_probability", side_effect=TimeoutError):
                wallclock.decide()
            self.assertIn("run=true", output.read_text())

    def test_clef_request_and_noul_response(self):
        response = {"success": True, "result": {"answers": {"run_wallclock":
                                                            {"type": "noul", "noul": 0.8}}}}
        with patch.dict(os.environ, {"CLOUDFLARE_ACCOUNT_ID": "account", "CLOUDFLARE_API_KEY": "token"},
                        clear=True), patch.object(wallclock.urllib.request, "urlopen",
                                                  return_value=io.BytesIO(json.dumps(response).encode())) as call:
            self.assertEqual(wallclock.clef_probability({"title": "perf"}), 0.8)
        request = call.call_args.args[0]
        self.assertEqual(request.get_header("Authorization"), "Bearer token")
        self.assertEqual(json.loads(request.data)["questions"]["run_wallclock"]["type"], "noul")

    def test_report_marks_slowdown_and_missing_projects(self):
        with tempfile.TemporaryDirectory() as directory:
            results = Path(directory)
            (results / "vscode.json").write_text(json.dumps({
                "project": "vscode", "main": {"mean": 10.0}, "pr": {"mean": 11.0},
                "main_exit": 0, "pr_exit": 1, "diagnostics_match": False}))
            args = argparse.Namespace(run_url="https://github.com/oxc-project/tsrs/actions/runs/1",
                                      base_sha="a" * 40, head_sha="b" * 40, merge_sha="c" * 40,
                                      decision="true", reason="",
                                      results=results)
            body = wallclock.render_report(args)
            self.assertIn("| vscode | 10.000 s | 11.000 s | +10.0% |", body)
            self.assertIn("Diagnostics or exit status differed on: vscode", body)
            self.assertIn("Measurements missing or invalid on:", body)

    def test_comment_does_not_replace_newer_pr_results(self):
        args = argparse.Namespace(pr_number=42, head_sha="a" * 40)
        with patch.dict(os.environ, {"GH_TOKEN": "fake", "GITHUB_REPOSITORY": "oxc-project/tsrs"}), \
                patch.object(wallclock, "request_json", return_value={"head": {"sha": "b" * 40}}) as request:
            wallclock.comment(args)
        request.assert_called_once()

    def test_comment_updates_existing_bot_comment(self):
        args = argparse.Namespace(pr_number=42, head_sha="a" * 40, base_sha="b" * 40, merge_sha="c" * 40,
                                  run_url="https://github.com/oxc-project/tsrs/actions/runs/1",
                                  decision="false", reason="Clef estimated 2.0% probability of a wall-clock impact",
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
