"""Small Python adapter for performance probes that still use Python's wait4.

Project preparation and machine identification belong to the JavaScript bench CLI.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
BENCH = REPO / "bench"
TIMED_FLAGS = ["--noEmit", "--incremental", "false", "--pretty", "false"]
CRASH_RE = re.compile(r"^(?:panic: |fatal error: |thread '.*' panicked at )")


def setup_project(cfg, project, work):
    subprocess.run(["node", str(BENCH / "bench.mjs"), "prepare", "--projects", project["name"],
                    "--work-dir", str(work)], check=True)


def project_path(cfg, project, work):
    root = work / "suite" / project["suite_dir"] if "suite_dir" in project else work / "solutions" / project["name"]
    return root, root / project["project"]


def machine_info(local=False, label=None):
    info = json.loads(subprocess.check_output(["node", str(BENCH / "bench.mjs"), "machine"], text=True))
    if label:
        info["label"] = label
    elif local:
        info["label"] = f"local machine ({info['cpu']}, {info['cpus']} cores, {info['memory_gb']} GB)"
    return info


def fmt_mem(value):
    return "n/a" if value is None else f"{value / 2**30:.2f} GiB"


def log(message):
    print(f"[bench] {message}", file=sys.stderr, flush=True)
