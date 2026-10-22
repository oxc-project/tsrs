#!/usr/bin/env python3
"""Require a normal compiler run and compare it with a normally finishing base.

The extraction matches tools/perf/verify.py: an `error TS` line and its
indented continuation lines form the diagnostic stream. Other output is not a
parity failure. A failing base may be repaired by the new compiler, but a new
compiler that fails abnormally never passes, even without a base.
"""

from __future__ import annotations

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


def finished_normally(output: str, code: int) -> bool:
    return code in (0, 1, 2) and not any(CRASH_RE.match(line) for line in output.splitlines())


def compare(base: str | None, base_exit: int | None, new: str, new_exit: int, label: str) -> bool:
    if not finished_normally(new, new_exit):
        print(f"FAIL {label}: new did not finish normally (exit {new_exit})")
        return False
    if base is None:
        print(f"pass {label}: {len(diagnostics(new))} ordered diagnostic lines, exit {new_exit}")
        return True
    if not finished_normally(base, base_exit):
        print(f"pass {label}: base did not finish normally (exit {base_exit}); "
              f"new finished normally (exit {new_exit}), skipping base diagnostic comparison")
        return True

    problems = []
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
    if None in (args.new, args.new_exit):
        parser.error("--new and --new-exit are required")
    if (args.base is None) != (args.base_exit is None):
        parser.error("--base and --base-exit must be supplied together")
    if not compare(args.base.read_text(errors="replace") if args.base is not None else None, args.base_exit,
                   args.new.read_text(errors="replace"), args.new_exit, args.label):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
