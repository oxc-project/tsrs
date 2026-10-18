#!/usr/bin/env python3
"""Summarize go test -json without counting parents with only skipped subtests as coverage."""

import argparse
from collections import Counter
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", type=Path)
    parser.add_argument("--rust-results", type=Path, help="optional tsrs-fourslash results directory to compare")
    args = parser.parse_args()
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
    skipped_parents = []
    for name, outcome in groups.items():
        children = [action for child, action in outcomes.items() if child.startswith(name + "/")]
        if outcome == "pass" and children and all(action == "skip" for action in children):
            groups[name] = "skip"
            skipped_parents.append(name)
    print("Coverage (parents with only skipped subtests count as skipped):", dict(Counter(groups.values())))
    print("Parents with only skipped subtests:", len(skipped_parents))
    if args.rust_results:
        rust_pass = set((args.rust_results / "pass.txt").read_text().splitlines())
        go_pass = {name for name, outcome in groups.items() if outcome == "pass"}
        print("Rust passes not exercised by this runner:", len(rust_pass - go_pass))
        for name in sorted(rust_pass - go_pass):
            print(" ", name)
        print("Additional Go-runner passes:", len(go_pass - rust_pass))
        for name in sorted(go_pass - rust_pass):
            print(" ", name)
    raise SystemExit(1 if package_result == "fail" else 0)


if __name__ == "__main__":
    main()
