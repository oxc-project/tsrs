#!/usr/bin/env python3
"""Work-stealing benchmark: wall time of `tsrs -p <proj> --noEmit` with stealing (default) vs static assignments.

usage: stealbench.py --tsrs BIN --work DIR --out results.jsonl --checkers 2,4,6,8 --rounds R [--projects a,b]
Each round runs every (checkers, arm) cell of a project once, in a fresh random order (interleaved, so drift hits all
arms alike). Every run is pinned to CPUS with taskset. One untimed warm-up run per project first.
"""
import argparse, hashlib, json, os, random, re, subprocess, sys, time
from pathlib import Path

ARMS = {"steal": None, "locality": "locality", "go": "go"}
CPUS = os.environ.get("BENCH_CPUS", "8-15")
FLAGS = ["--noEmit", "--incremental", "false", "--pretty", "false"]
ERROR_RE = re.compile(r"^((?:\S.*\(\d+,\d+\): )?error TS\d+):")


def project_path(cfg, p, work):
    root = work / "suite" / p["suite_dir"] if "suite_dir" in p else work / "solutions" / p["name"]
    return root, root / p["project"]


def run(tsrs, cwd, proj, n, arm):
    env = dict(os.environ)
    env.pop("TSRS_CHECKER_ASSIGNMENT", None)
    if ARMS[arm]:
        env["TSRS_CHECKER_ASSIGNMENT"] = ARMS[arm]
    argv = ["taskset", "-c", CPUS, str(tsrs), "-p", str(proj), *FLAGS, "--checkers", str(n)]
    t0 = time.perf_counter()
    r = subprocess.run(argv, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    wall = time.perf_counter() - t0
    out = r.stdout.decode(errors="replace")
    return {"wall_s": wall, "exit": r.returncode, "errors": sum(1 for l in out.splitlines() if ERROR_RE.match(l)),
            "stdout_sha": hashlib.sha256(r.stdout).hexdigest()[:16], "stderr_tail": r.stderr.decode(errors="replace")[-300:]}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tsrs", type=Path, required=True)
    ap.add_argument("--work", type=Path, required=True)
    ap.add_argument("--projects-json", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--checkers", default="2,4,6,8")
    ap.add_argument("--arms", default="steal,locality,go")
    ap.add_argument("--rounds", type=int, default=10)
    ap.add_argument("--projects")
    ap.add_argument("--seed", type=int, default=1)
    a = ap.parse_args()
    cfg = json.loads(a.projects_json.read_text())
    projects = [p for p in cfg["projects"] if not a.projects or p["name"] in a.projects.split(",")]
    counts = [int(x) for x in a.checkers.split(",")]
    arms = a.arms.split(",")
    rng = random.Random(a.seed)
    with a.out.open("a") as f:
        for p in projects:
            cwd, proj = project_path(cfg, p, a.work)
            if not proj.exists():
                print(f"skip {p['name']}: {proj} missing", file=sys.stderr, flush=True)
                continue
            run(a.tsrs, cwd, proj, counts[-1], "steal")  # warm-up: page cache, lazily built state
            for rnd in range(a.rounds):
                cells = [(n, arm) for n in counts for arm in arms]
                rng.shuffle(cells)
                for n, arm in cells:
                    res = run(a.tsrs, cwd, proj, n, arm)
                    rec = {"project": p["name"], "checkers": n, "arm": arm, "round": rnd, **res}
                    f.write(json.dumps(rec) + "\n")
                    f.flush()
            print(f"done {p['name']}", file=sys.stderr, flush=True)


if __name__ == "__main__":
    main()
