#!/usr/bin/env python3
"""Compare ordered TypeScript diagnostics and exit codes from two compiler runs.

The extraction matches tools/perf/verify.py: an `error TS` line and its
indented continuation lines form the diagnostic stream. Other output is not a
parity failure.
"""

import argparse
import contextlib
import io
import re
from pathlib import Path


ERROR_LINE_RE = re.compile(r"(^|: )error TS\d+:")
CRASH_RE = re.compile(r"^(?:panic: |fatal error: |thread '.*' panicked at )")


def diagnostics(output: str) -> list[str]:
    lines = []
    inside = False
    for line in output.splitlines():
        if ERROR_LINE_RE.search(line):
            lines.append(line)
            inside = True
        elif inside and line[:1] in (" ", "\t") and line.strip():
            lines.append(line)
        else:
            inside = False
    return lines


def compare(base: str, base_exit: int, new: str, new_exit: int, label: str) -> bool:
    problems = []
    for side, output, code in (("base", base, base_exit), ("new", new, new_exit)):
        if code not in (0, 1, 2) or any(CRASH_RE.match(line) for line in output.splitlines()):
            problems.append(f"{side} did not finish normally (exit {code})")
    if base_exit != new_exit:
        problems.append(f"exit differs: base {base_exit}, new {new_exit}")

    before, after = diagnostics(base), diagnostics(new)
    if before != after:
        for number, (old, current) in enumerate(zip(before, after), start=1):
            if old != current:
                break
        else:
            number = min(len(before), len(after)) + 1
            old = before[number - 1] if number <= len(before) else "<end of diagnostics>"
            current = after[number - 1] if number <= len(after) else "<end of diagnostics>"
        problems.append(f"diagnostic line {number} differs: base {old!r}, new {current!r}")

    if problems:
        for problem in problems:
            print(f"FAIL {label}: {problem}")
        return False
    print(f"pass {label}: {len(before)} ordered diagnostic lines, exit {base_exit}")
    return True


def self_test() -> None:
    first = "a.ts(1,1): error TS1: first\n  detail\nb.ts(1,1): error TS2: second\n"
    reordered = "b.ts(1,1): error TS2: second\na.ts(1,1): error TS1: first\n  detail\n"
    assert diagnostics(first) != diagnostics(reordered)
    assert diagnostics(first) != diagnostics(first.replace("  detail", "  changed detail"))
    assert diagnostics("noise\n" + first) == diagnostics(first)
    with contextlib.redirect_stdout(io.StringIO()):
        assert not compare(first, 2, reordered, 2, "self-test order")
        assert not compare(first, 2, first, 0, "self-test exit")
        assert compare(first, 2, "noise\n" + first, 2, "self-test non-diagnostic output")
    print("self-test: ordered diagnostics and exit codes are checked")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--label", default="run")
    parser.add_argument("--base", type=Path)
    parser.add_argument("--base-exit", type=int)
    parser.add_argument("--new", type=Path)
    parser.add_argument("--new-exit", type=int)
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    if None in (args.base, args.base_exit, args.new, args.new_exit):
        parser.error("--base, --base-exit, --new, and --new-exit are required")
    if not compare(args.base.read_text(errors="replace"), args.base_exit,
                   args.new.read_text(errors="replace"), args.new_exit, args.label):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
