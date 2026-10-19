#!/usr/bin/env python3
"""Summarize official Fourslash outcomes and check reviewed differences and coverage."""

import argparse
from collections import Counter
import difflib
import hashlib
import json
import os
from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[2]


def fingerprint(values):
    values = sorted(values)
    return {"count": len(values), "sha256": hashlib.sha256(
        json.dumps(values, separators=(",", ":")).encode()).hexdigest()}


def inspect(results, baselines=None):
    outcomes, output = {}, {}
    package_result = None
    package_ends = 0
    for line in results.read_text().splitlines():
        event = json.loads(line)
        action = event["Action"]
        name = event.get("Test")
        if action == "output":
            text = event["Output"]
            if "panic:" in text or "fatal error:" in text:
                raise ValueError("runner panic; cannot accept a known baseline mismatch")
            if name:
                output.setdefault(name, []).append(text)
        if action not in ("pass", "fail", "skip"):
            continue
        if name:
            if name in outcomes:
                raise ValueError(f"duplicate outcome: {name}")
            outcomes[name] = action
        else:
            package_result = action
            package_ends += 1
    if package_ends != 1:
        raise ValueError("incomplete results: expected one final package outcome")
    groups = {name: outcome for name, outcome in outcomes.items() if "/" not in name}
    if not groups:
        raise ValueError("empty selection")
    children = {}
    for name in outcomes:
        parent, separator, _ = name.rpartition("/")
        if separator:
            children.setdefault(parent, []).append(name)
    effective = dict(outcomes)
    for name in sorted(outcomes, key=lambda name: name.count("/"), reverse=True):
        if outcomes[name] == "pass" and name in children and all(effective[child] == "skip" for child in children[name]):
            effective[name] = "skip"
    skipped_parents = [name for name in groups if groups[name] == "pass" and effective[name] == "skip"]
    counts = Counter(effective[name] for name in groups)
    print("Go top-level outcomes:", dict(Counter(groups.values())))
    print("Coverage (parents with only skipped subtests count as skipped):", dict(counts))
    print("Parents with only skipped subtests:", len(skipped_parents))
    failed = sorted(name for name, outcome in outcomes.items() if outcome == "fail")
    if package_result != ("fail" if failed else "pass"):
        raise ValueError("package outcome does not match case outcomes")
    mismatches = []
    if baselines is not None:
        for name in failed:
            # Only baseline assertions may be recorded. A new assertion, panic, or parent failure is never waived.
            messages = [line for line in "".join(output.get(name, [])).splitlines()
                        if line and not line.startswith(("=== ", "--- FAIL:"))]
            if len(messages) != 1:
                raise ValueError(f"non-baseline failure: {name}")
            match = re.fullmatch(r"\s*baseline\.go:\d+: the baseline file (.+) has changed\. "
                                 r"\(Run `hereby baseline-accept` if the new baseline is correct\.\)", messages[0])
            if not match:
                raise ValueError(f"non-baseline failure: {name}: {messages[0]}")
            reference = Path(match[1])
            relative = reference.resolve().relative_to((baselines / "reference").resolve())
            actual = baselines / "local" / relative
            before, after = reference.read_bytes(), actual.read_bytes()
            if before == after:
                raise ValueError(f"missing baseline difference: {name}")
            difference = "".join(difflib.unified_diff(before.decode().splitlines(True), after.decode().splitlines(True),
                                                     fromfile="reference", tofile="actual"))
            diff_path = Path(str(actual) + ".diff")
            diff_path.write_text(difference)
            mismatches.append({"test": name, "baseline": relative.as_posix(),
                               "reference_sha256": hashlib.sha256(before).hexdigest(),
                               "actual_sha256": hashlib.sha256(after).hexdigest(),
                               "diff_sha256": hashlib.sha256(difference.encode()).hexdigest()})
    snapshot = {"coverage": {"selected": fingerprint(outcomes),
                              "skipped": fingerprint(name for name in effective if effective[name] == "skip")},
                "mismatches": mismatches}
    return snapshot, counts, failed


def publish(results, text):
    print(text, end="")
    (results.parent / "report.md").write_text(text)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as output:
            output.write(text)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", type=Path)
    parser.add_argument("--min-pass", type=int, default=0)
    parser.add_argument("--known", type=Path)
    parser.add_argument("--baselines", type=Path)
    parser.add_argument("--runner-exit", type=int, default=0)
    args = parser.parse_args()
    if args.min_pass < 0:
        parser.error("--min-pass must be nonnegative")
    try:
        if args.runner_exit not in (0, 1):
            raise ValueError(f"runner failed with exit {args.runner_exit}")
        if args.known and not args.baselines:
            raise ValueError("--known requires --baselines")
        snapshot, counts, failed = inspect(args.results, args.baselines if args.known else None)
        problems, details = [], []
        if args.known:
            known = json.loads(args.known.read_text())
            commit = re.search(r'(?m)^commit = "([a-f0-9]{40})"$', (ROOT / "Cargo.toml").read_text())[1]
            if known.get("schema_version") != 1 or known.get("reference_commit") != commit:
                raise ValueError("obsolete inventory schema or different upstream commit")
            if snapshot["coverage"] != known["coverage"]:
                problems.append("selected or skipped coverage changed")
            if snapshot["mismatches"] != known["mismatches"]:
                problems.append("new, changed or resolved mismatches; review the full baseline diffs")
            for entry in snapshot["mismatches"]:
                status = "known" if entry in known["mismatches"] else "new or changed"
                details.append(f"- {status} conformance gap: `{entry['test']}` (`{entry['baseline']}`)")
            if args.runner_exit != (1 if failed else 0):
                problems.append("runner exit does not match case outcomes")
        elif failed or args.runner_exit:
            problems.append("unrecorded failures: " + ", ".join(failed))
        if counts["pass"] < args.min_pass:
            problems.append(f"pass={counts['pass']} (minimum {args.min_pass})")
        text = "Fourslash gate: " + ("FAIL: " + "; ".join(problems) if problems else "PASS") + "\n"
        report = ("# Fourslash conformance\n\n" +
                  f"Passing: {counts['pass']}; failing: {counts['fail']}; skipped: {counts['skip']}.\n\n" +
                  "\n".join(details) + "\n\n" + text)
        publish(args.results, report)
        (args.results.parent / "coverage.json").write_text(json.dumps(snapshot, indent=2) + "\n")
        return 1 if problems else 0
    except (OSError, ValueError, KeyError, TypeError) as error:
        publish(args.results, f"# Fourslash conformance\n\nFourslash gate: FAIL: {error}\n")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
