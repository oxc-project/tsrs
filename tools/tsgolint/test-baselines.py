#!/usr/bin/env python3
"""Regression checks for baseline fixture preparation and byte edits."""

import json
import pathlib
import runpy
import tempfile
import unittest

RUNNER = runpy.run_path(str(pathlib.Path(__file__).with_name("run-baselines.py")))


class BaselineTests(unittest.TestCase):
    def test_config_extending_original_root_is_not_circular(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            (root / "tsconfig.json").write_text('{"compilerOptions":{"strict":true}}')
            (root / "tsconfig.unstrict.json").write_text('{"extends":"./tsconfig.json","compilerOptions":{"strict":false}}')
            RUNNER["select_config"](root, "tsconfig.unstrict.json")
            config = json.loads((root / "tsconfig.json").read_text())
            base = (root / config["extends"]).resolve()
            self.assertNotEqual(base, root / "tsconfig.json")
            self.assertTrue(json.loads(base.read_text())["compilerOptions"]["strict"])
            self.assertFalse(config["compilerOptions"]["strict"])

    def test_fix_ranges_are_bytes_and_overlaps_skip_whole_groups(self):
        diagnostics = [
            {"fixes": [{"text": "x", "range": {"pos": 2, "end": 6}}]},
            {"fixes": [{"text": "y", "range": {"pos": 3, "end": 6}}]},
        ]
        self.assertEqual(RUNNER["apply_fixes"]("é😀z", diagnostics), ("éxz", True))

    def test_truncated_frames_and_error_frames_fail(self):
        for data in [b"\x00", b"\x01\x00\x00\x00\x01"]:
            with self.assertRaises(AssertionError):
                RUNNER["frames"](data)


if __name__ == "__main__":
    unittest.main()
