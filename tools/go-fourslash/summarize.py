#!/usr/bin/env python3
"""Summarize go test -json without counting parents with only skipped subtests as coverage."""

import argparse
from collections import Counter
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", type=Path)
    parser.add_argument("--min-pass", type=int, default=0, help="fail if fewer groups are exercised successfully")
    args = parser.parse_args()
    if args.min_pass < 0:
        parser.error("--min-pass must be nonnegative")
    outcomes = {}
    package_result = None
    for line in args.results.read_text().splitlines():
        event = json.loads(line)
        action = event["Action"]
        if action not in ("pass", "fail", "skip"):
            continue
        if "Test" in event:
            outcomes[event["Test"]] = action
        else:
            package_result = action
    if package_result is None:
        parser.error("incomplete results: no final package outcome")
    groups = {name: outcome for name, outcome in outcomes.items() if "/" not in name}
    print("Go top-level outcomes:", dict(Counter(groups.values())))
    children = {}
    for name in outcomes:
        parent, separator, _ = name.rpartition("/")
        if separator:
            children.setdefault(parent, []).append(name)
    effective = dict(outcomes)
    # Collapse deepest parents first so nested skipped-only groups cannot inflate the gate.
    for name in sorted(outcomes, key=lambda name: name.count("/"), reverse=True):
        if outcomes[name] == "pass" and name in children and all(effective[child] == "skip" for child in children[name]):
            effective[name] = "skip"
    skipped_parents = [name for name in groups if groups[name] == "pass" and effective[name] == "skip"]
    groups = {name: effective[name] for name in groups}
    counts = Counter(groups.values())
    print("Coverage (parents with only skipped subtests count as skipped):", dict(counts))
    print("Parents with only skipped subtests:", len(skipped_parents))
    failed = package_result != "pass" or "fail" in outcomes.values() or counts["pass"] < args.min_pass
    if failed:
        print(f"fourslash gate failed: package={package_result}, pass={counts['pass']} "
              f"(minimum {args.min_pass}), fail={counts['fail']}")
    raise SystemExit(1 if failed else 0)


if __name__ == "__main__":
    main()
