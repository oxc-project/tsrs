#!/usr/bin/env python3
"""Run full conformance and compare exact failures and coverage with the reviewed inventory."""

import argparse
from collections import Counter
import hashlib
import html
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
KNOWN = Path(__file__).with_name("known-mismatches.json")
BASELINES = ("errors", "types", "symbols", "js", "js.map", "sourcemap.txt")
CLASSES = ("pass", "codes", "fail", "crash", "timeout", "skip")
FLAGS = "types,symbols,js,jsmap,sourcemap"


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object)


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def reference_commit():
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    section = re.search(r"(?ms)^\[workspace\.metadata\.typescript\]\s*\n(.*?)(?=^\[|\Z)", text)
    commit = re.search(r'^commit\s*=\s*"([0-9a-f]{40})"\s*$', section[1], re.M) if section else None
    if not commit:
        raise ValueError("missing pinned TypeScript commit in Cargo.toml")
    return commit[1]


def fingerprint(values):
    values = sorted(values)
    encoded = json.dumps(values, ensure_ascii=False, separators=(",", ":")).encode()
    return {"count": len(values), "sha256": hashlib.sha256(encoded).hexdigest()}


def validate_id(name):
    if not isinstance(name, str):
        raise ValueError("test ID must be a string")
    parts = PurePosixPath(name).parts
    if len(parts) < 2 or parts[0] not in ("compiler", "conformance") or ".." in parts or "\\" in name:
        raise ValueError(f"invalid test ID: {name}")
    return name


def mismatch_index(entries):
    index = {}
    for entry in entries:
        key = (validate_id(entry["test"]), entry["baseline"])
        if key[1] not in BASELINES or entry["class"] not in ("codes", "fail"):
            raise ValueError(f"invalid mismatch: {key}")
        if key[1] != "errors" and entry["class"] == "codes":
            raise ValueError(f"invalid diagnostic classification: {key}")
        if key in index:
            raise ValueError(f"duplicate mismatch: {key}")
        if not re.fullmatch(r"[0-9a-f]{64}", entry["diff_sha256"]):
            raise ValueError(f"invalid diff fingerprint: {key}")
        if not isinstance(entry["first_difference"], str):
            raise ValueError(f"invalid first difference: {key}")
        index[key] = entry
    return index


def inspect_results(directory, mode, commit):
    invocation = read_json(directory / "invocation.json")
    if invocation != {"mode": mode, "reference_commit": commit, "exit_code": 0}:
        raise ValueError("runner failed, or results were produced for a different mode/reference")
    manifest = read_json(directory / "run.json")
    if manifest["complete"] is not True or manifest["mode"] != mode or manifest["baselines"] != list(BASELINES):
        raise ValueError("results are not a complete full-baseline run in the requested mode")
    selected = manifest["selected"]
    if not selected or len(set(selected)) != len(selected):
        raise ValueError("empty or duplicate test selection")
    for name in selected:
        validate_id(name)
    excluded = manifest["excluded"]
    if not isinstance(excluded, dict):
        raise ValueError("missing fixture exclusion inventory")
    for name, reason in excluded.items():
        validate_id(name)
        if not isinstance(reason, str) or not reason:
            raise ValueError(f"missing exclusion reason: {name}")
    summary = read_json(directory / "summary.json")
    records = summary["tests"]
    if len(records) != len(selected) or {test["id"] for test in records} != set(selected):
        raise ValueError("results do not match the selected test inventory")

    counts, skipped, exercised, mismatches, problems = {}, {}, {}, [], []
    for baseline in BASELINES:
        field = "class" if baseline == "errors" else baseline
        prefix = "" if baseline == "errors" else f"{baseline}-"
        classes = {kind: [] for kind in CLASSES}
        skipped[baseline] = {}
        for test in records:
            name, kind = test["id"], test.get(field)
            if kind not in CLASSES or (kind == "codes" and baseline != "errors"):
                raise ValueError(f"missing or invalid {baseline} result: {name}")
            classes[kind].append(name)
            detail = test.get("diff" if baseline == "errors" else f"{baseline}_diff", "")
            if kind == "skip":
                reason = test.get("skip", "") if baseline == "errors" else detail
                if not reason:
                    raise ValueError(f"missing {baseline} skip reason: {name}")
                skipped[baseline][name] = reason
            elif kind in ("crash", "timeout"):
                problems.append(f"{kind}: {name} [{baseline}] {test.get('panic', '') or detail}")
            elif kind in ("codes", "fail"):
                # Fingerprint the entire diff, not just its first line. A known failure may regress further.
                stem = directory / name
                actual = Path(f"{stem}.actual.txt" if baseline == "errors" else f"{stem}.{baseline}.actual")
                diff = Path(f"{stem}.diff" if baseline == "errors" else f"{stem}.{baseline}.diff")
                if not actual.is_file() or not diff.is_file() or not diff.stat().st_size:
                    raise ValueError(f"missing mismatch artifacts (or harness failure): {name} [{baseline}]")
                mismatches.append({
                    "test": name, "baseline": baseline, "class": kind,
                    "first_difference": detail,
                    "diff_sha256": hashlib.sha256(diff.read_bytes()).hexdigest(),
                })
        for kind, names in classes.items():
            listed = (directory / f"{prefix}{kind}.txt").read_text(encoding="utf-8").splitlines()
            if sorted(listed) != sorted(names):
                raise ValueError(f"missing or inconsistent case list: {prefix}{kind}.txt")
        counts[baseline] = {kind: len(names) for kind, names in classes.items()}
        totals_key = "totals" if baseline == "errors" else f"{baseline}_totals"
        if summary[totals_key]["all"] != counts[baseline]:
            raise ValueError(f"inconsistent totals: {baseline}")
        exercised[baseline] = sorted(name for kind, names in classes.items() if kind != "skip" for name in names)

    coverage = {
        "selected": fingerprint(selected),
        "excluded": dict(sorted(excluded.items())),
        "exercised": {kind: fingerprint(exercised[kind]) for kind in BASELINES},
        "skipped": {kind: fingerprint(list(skipped[kind].items())) for kind in BASELINES},
    }
    # Keep the full IDs and reasons in the artifact, while the reviewed file has compact coverage fingerprints.
    write_json(directory / "coverage.json", {"selected": sorted(selected), "excluded": excluded,
                                           "exercised": exercised, "skipped": skipped})
    snapshot = {"coverage": coverage, "mismatches": sorted(mismatches, key=lambda m: (m["test"], m["baseline"]))}
    return snapshot, counts, skipped, problems


def compare(snapshot, expected):
    actual_index = mismatch_index(snapshot["mismatches"])
    expected_index = mismatch_index(expected["mismatches"])
    changes = []
    for key in sorted(actual_index.keys() | expected_index.keys()):
        if key not in expected_index:
            status = "new"
        elif key not in actual_index:
            status = "resolved or no longer exercised"
        elif actual_index[key] != expected_index[key]:
            status = "changed"
        else:
            status = "known"
        changes.append((status, actual_index.get(key, expected_index.get(key))))
    problems = [f"{status}: {entry['test']} [{entry['baseline']}]" for status, entry in changes if status != "known"]
    before, after = expected["coverage"], snapshot["coverage"]
    for field in ("selected", "excluded", "exercised", "skipped"):
        if before[field] != after[field]:
            problems.append(f"coverage changed: {field}; inspect coverage.json and refresh only after review")
    return changes, problems


def report(directory, mode, counts, skipped, excluded, changes, problems):
    lines = [f"## Full conformance: {mode}", "", "| Baseline | Pass | Codes | Fail | Crash | Timeout | Skip |",
             "| --- | ---: | ---: | ---: | ---: | ---: | ---: |"]
    for baseline in BASELINES:
        lines.append(f"| {baseline} | " + " | ".join(str(counts[baseline][c]) for c in CLASSES) + " |")
    lines.extend(["", f"Recorded mismatches remain conformance gaps. Fixture exclusions: {len(excluded)}.", ""])
    if changes:
        lines.extend(["### Mismatches", ""])
        for status, entry in changes:
            lines.append(f"- **{status}**: `{entry['test']}` / `{entry['baseline']}` ({entry['class']})")
            lines.append(f"  <pre>{html.escape(entry['first_difference'])}</pre>")
    else:
        lines.extend(["No mismatches.", ""])
    lines.extend(["<details><summary>Skipped coverage and non-applicable baselines</summary>", "",
                  "A skipped case is not a pass. `noTypesAndSymbols`, `no js output`, and `no source maps`/",
                  "`no source map baseline` identify baselines the harness does not produce; compiler-option",
                  "and content-mapper skips are unsupported coverage. Upstream emit exclusions are listed separately.",
                  "Full case IDs and reasons are in `coverage.json`.", "",
                  "| Baseline | Reason | Cases |", "| --- | --- | ---: |"])
    for baseline in BASELINES:
        for reason, count in sorted(Counter(skipped[baseline].values()).items()):
            lines.append(f"| {baseline} | {html.escape(reason).replace('|', '&#124;')} | {count} |")
    lines.extend(["", "</details>", "", "<details><summary>Excluded fixtures</summary>", ""])
    lines.extend(f"- `{name}`: {reason}" for name, reason in sorted(excluded.items()))
    lines.extend(["", "</details>", ""])
    if problems:
        lines.extend(["### Gate failures", ""])
        lines.extend(f"- {html.escape(problem)}" for problem in problems)
    lines.extend(["", f"Conformance gate: {'FAIL' if problems else 'PASS'}; {len(changes)} mismatch entries.", ""])
    publish_report(directory, "\n".join(lines))


def publish_report(directory, text):
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "report.md").write_text(text, encoding="utf-8")
    print(text, end="", flush=True)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as output:
            output.write(text)


def run(directory, mode, commit):
    directory.mkdir(parents=True, exist_ok=True)
    # Only remove run-level metadata. Never recursively delete a caller-supplied results directory.
    for filename in ("summary.json", "run.json", "invocation.json", "coverage.json", "report.md"):
        (directory / filename).unlink(missing_ok=True)
    env = os.environ.copy()
    env.pop("TSRS_CHECKER_ASSIGNMENT", None)
    env["TSRS_HISTORY"] = "canonical" if mode == "default" else "go"
    env["TS_TEST_PROGRAM_SINGLE_THREADED"] = "false" if mode == "default" else "true"
    env["TSRS_TEST_RESULTS"] = str(directory.resolve())
    command = [env.get("TSRS_TEST", "target/release/tsrs-test"), "run", "--suite", "all",
               "--baselines", FLAGS, "--timeout", env.get("TEST_TIMEOUT", "120")]
    with (directory / "runner.log").open("w", encoding="utf-8") as log:
        with subprocess.Popen(command, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                              text=True, encoding="utf-8", errors="replace") as process:
            for line in process.stdout:
                log.write(line)
                print(line, end="", flush=True)
            status = process.wait()
    write_json(directory / "invocation.json", {"mode": mode, "reference_commit": commit, "exit_code": status})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("run", "check", "refresh"))
    parser.add_argument("--mode", choices=("go", "default"), default="go")
    parser.add_argument("--results", type=Path)
    parser.add_argument("--known", type=Path, default=KNOWN)
    args = parser.parse_args()
    directory = args.results or Path(os.environ.get("TSRS_TEST_RESULTS", ROOT / "target" /
                                                   ("test-results" if args.mode == "go" else "test-results-default")))
    try:
        if args.command == "refresh" and os.environ.get("CI"):
            raise ValueError("refresh is a local review operation; CI cannot update known mismatches")
        commit = reference_commit()
        if args.command == "run":
            run(directory, args.mode, commit)
        snapshot, counts, skipped, problems = inspect_results(directory, args.mode, commit)
        if args.known.exists():
            known = read_json(args.known)
            if known["reference_commit"] != commit:
                raise ValueError("known mismatches target a different upstream commit; regenerate both modes after review")
        elif args.command == "refresh":
            known = {"reference_commit": commit, "modes": {}}
        else:
            raise ValueError(f"missing reviewed inventory: {args.known}")
        if args.command == "refresh":
            if problems:
                raise ValueError("cannot record crashes/timeouts: " + "; ".join(problems))
            mismatch_index(snapshot["mismatches"])
            known["modes"][args.mode] = snapshot
            args.known.parent.mkdir(parents=True, exist_ok=True)
            write_json(args.known, known)
            changes = [("known", entry) for entry in snapshot["mismatches"]]
            print(f"Updated {args.known} ({args.mode}); review the diff before committing.")
        else:
            changes, differences = compare(snapshot, known["modes"][args.mode])
            problems.extend(differences)
        report(directory, args.mode, counts, skipped, snapshot["coverage"]["excluded"], changes, problems)
        return 1 if problems else 0
    except (OSError, ValueError, KeyError, TypeError) as error:
        publish_report(directory, f"## Full conformance: {args.mode}\n\nConformance gate: FAIL\n\n"
                                  f"Incomplete or invalid results/inventory: {html.escape(str(error))}\n")
        return 1


if __name__ == "__main__":
    sys.exit(main())
