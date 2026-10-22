#!/usr/bin/env python3
"""Compare the Rust headless binary with exported, pinned tsgolint cases."""

import argparse
import json
import pathlib
import os
import re
import shutil
import struct
import subprocess
import tempfile


def frames(data):
    result = []
    offset = 0
    while offset < len(data):
        if len(data) - offset < 5:
            raise AssertionError("truncated frame header")
        length, kind = struct.unpack_from("<IB", data, offset)
        offset += 5
        if len(data) - offset < length:
            raise AssertionError("truncated frame payload")
        payload = json.loads(data[offset:offset + length])
        offset += length
        if kind == 0:
            raise AssertionError(payload["error"])
        if kind == 1:
            result.append(payload)
        elif kind != 2:
            raise AssertionError(f"unknown frame type {kind}")
    return result


def apply_fixes(code, diagnostics):
    groups = [sorted(d["fixes"], key=lambda f: (f["range"]["pos"], f["range"]["end"])) for d in diagnostics if d.get("fixes")]
    groups.sort(key=lambda group: (group[0]["range"]["pos"], group[-1]["range"]["end"]))
    data = code.encode()
    output = bytearray()
    last = 0
    fixed = False
    for group in groups:
        if last > group[0]["range"]["pos"]:
            continue
        for fix in group:
            start, end = fix["range"]["pos"], fix["range"]["end"]
            assert last <= start <= end <= len(data), "invalid fix range"
            output.extend(data[last:start])
            output.extend(fix["text"].encode())
            last = end
            fixed = True
    output.extend(data[last:])
    return output.decode(), fixed


def select_config(root, selected):
    root = root.resolve()
    base = root / "tsconfig.json"
    preserved = root / "tsconfig.tsrs-original.json"
    base.rename(preserved)
    for config in root.rglob("tsconfig*.json"):
        def redirect(match):
            target = (config.parent / match.group(1)).resolve()
            if target == base:
                relative = pathlib.Path(os.path.relpath(preserved, config.parent)).as_posix()
                return '"extends": ' + json.dumps("./" + relative)
            return match.group(0)
        config.write_text(re.sub(r'"extends"\s*:\s*"([^"\n]+)"', redirect, config.read_text()))
    if selected == "tsconfig.json":
        selected = preserved.name
    selected_path = root / selected
    assert selected_path.parent == root, "baseline configuration must be in fixture root"
    base.write_text(selected_path.read_text())


def run_case(binary, fixtures, suite, case, expected):
    with tempfile.TemporaryDirectory(prefix="tsrs-lint-case-") as temporary:
        root = pathlib.Path(temporary).resolve()
        shutil.copytree(fixtures, root, dirs_exist_ok=True)
        dependencies = fixtures.parent / "dependencies"
        if dependencies.exists():
            shutil.copytree(dependencies, root / "node_modules", dirs_exist_ok=True)
        filename = case["FileName"] or ("react.tsx" if case["Tsx"] else "file.ts")
        file = root / filename
        selected_config = case["TSConfig"] or suite["TSConfig"]
        select_config(root, selected_config)
        extras = case.get("Files") or {}
        overrides = {str(root / name): text for name, text in extras.items()}
        payload = {"version": 2, "configs": [{"file_paths": [str(file)], "rules": [{"name": suite["Name"], "options": case["Options"]}]}], "source_overrides": overrides}

        def lint(code):
            payload["source_overrides"][str(file)] = code
            process = subprocess.run([str(binary), "headless", "-fix", "-fix-suggestions"], input=json.dumps(payload).encode(), capture_output=True, cwd=root)
            diagnostics = frames(process.stdout)
            assert process.returncode == 0, process.stderr.decode(errors="replace")
            for diagnostic in diagnostics:
                path = diagnostic.get("file_path")
                if path is not None:
                    diagnostic["file_path"] = pathlib.Path(path).relative_to(root).as_posix()
            return diagnostics

        diagnostics = lint(case["Code"])
        assert diagnostics == expected, f"diagnostics differ\nexpected {expected!r}\nactual {diagnostics!r}"
        outputs = []
        code = case["Code"]
        for _ in range(10):
            code, fixed = apply_fixes(code, diagnostics)
            if not fixed:
                break
            outputs.append(code)
            diagnostics = lint(code)
        assert outputs == (case.get("Output") or []), f"fix outputs differ: {outputs!r}"
        initial = expected
        for diagnostic, error in zip(initial, case.get("Errors") or [], strict=True):
            suggestions = diagnostic.get("suggestions") or []
            for suggestion, expected_suggestion in zip(suggestions, error.get("Suggestions") or [], strict=True):
                output, _ = apply_fixes(case["Code"], [suggestion])
                assert output == expected_suggestion["Output"], "suggestion output differs"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("baselines", type=pathlib.Path)
    parser.add_argument("--rule")
    args = parser.parse_args()
    root = args.baselines.resolve()
    binary = args.binary.resolve()
    suites = sorted(root.glob(f"{args.rule or '*' }/*.cases.json"))
    if not suites:
        parser.error("no baseline suites found")
    passed = skipped = 0
    for source in suites:
        suite = json.loads(source.read_text())
        only = any(case["Only"] for group in ["Valid", "Invalid"] for case in suite[group])
        for group in ["Valid", "Invalid"]:
            for index, case in enumerate(suite[group]):
                name = f"{source.stem.removesuffix('.cases')}_{group.lower()}-{index}"
                if case["Skip"] or (only and not case["Only"]):
                    skipped += 1
                    continue
                expected = json.loads((source.parent / (name + ".diagnostics.json")).read_text())
                try:
                    run_case(binary, root / "fixtures", suite, case, expected)
                except (AssertionError, ValueError) as error:
                    raise AssertionError(f"{suite['Name']}/{name}: {error}") from error
                passed += 1
        print(f"{suite['Name']}: suite {source.name} passed", flush=True)
    print(f"{passed} cases passed; {skipped} upstream cases skipped")


if __name__ == "__main__":
    main()
