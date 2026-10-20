#!/usr/bin/env python3
"""Measure one compiler process for the PR wall-clock workflow.

The fresh interpreter keeps its resident size below the compiler's peak RSS.
"""

import json
import os
import sys
import time


def main() -> None:
    out, sep, argv = sys.argv[1], sys.argv[2], sys.argv[3:]
    if sep != "--" or not argv:
        sys.exit("usage: measure.py <out.json> -- <command> [args...]")
    t0 = time.perf_counter()
    pid = os.posix_spawnp(argv[0], argv, os.environ)
    _, status, ru = os.wait4(pid, 0)
    wall = time.perf_counter() - t0
    with open(out, "w") as output:
        json.dump({"wall_s": wall, "maxrss": ru.ru_maxrss, "status": status}, output)
    sys.exit(os.waitstatus_to_exitcode(status) % 256)


if __name__ == "__main__":
    main()
