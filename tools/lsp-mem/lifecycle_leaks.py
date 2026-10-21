#!/usr/bin/env python3
"""Memory of tsrs across repeated program lifecycles (issue #646: track observed leaks).

Each scenario drives one long-lived tsrs process through N identical cycles that create and release a program (or a
file version, or a snapshot) and samples the process's resident set size (`ps -o rss`) after every cycle. The first
--warmup cycles are discarded (caches, thread arenas, allocator pools); over the rest a least-squares line gives
bytes per cycle, and the median of the per-cycle differences gives a robust second estimate. Steady state is a slope
near zero; a leak is a positive slope that holds over the whole tail (the summary also prints the slope of the last
half of the tail alone, which separates one-time growth from growth that continues).

Scenarios (--scenario, repeatable; default: all):
  watch            `tsrs --watch -p <project> --noEmit`: a leaf file is edited each cycle (append a comment, or
                   with --edit flip: alternate a type error on and off, which re-checks the file); each program is
                   freed by tsrs_execute watcher.rs (`free_program`).
  watch-root       the same, editing a module every other file imports, so every dependent is re-checked.
  lsp-edit         `tsrs --lsp -stdio`: open a file, then per cycle one didChange + diagnostic + hover (lsp_mem.py).
  lsp-open-close   per cycle: didOpen a file, request diagnostics, didClose it (snapshot per open/close).
  lsp-reload       per cycle: write tsconfig.json (a comment toggles) and send workspace/didChangeWatchedFiles,
                   which reloads the project; then diagnostics of the open file.
  api-snapshot     `tsrs --api --async`: per cycle createSnapshot(openProjects) + getSemanticDiagnostics + release.
  api-update       keep one base snapshot; per cycle write the leaf file, updateSnapshot(fileNotifications.changed,
                   ensurePrograms), getSemanticDiagnostics, release the previous snapshot.
  api-build        per cycle createBuildOrchestrator + build + disposeBuildOrchestrator (`tsc -b` in one process).

Project: --project <dir with tsconfig.json> and --file <leaf, relative>, or (default) a synthetic project of
--modules chained modules written under --work. The synthetic project's src/leaf.ts imports nothing and nothing
imports it; src/m0.ts is imported by every other module.

    python3 tools/lsp-mem/lifecycle_leaks.py --cycles 60 --warmup 15
    python3 tools/lsp-mem/lifecycle_leaks.py --scenario api-update --cycles 200 --json out.json
    TSRS_ARENA_POISON=1 TSRS_REGION_LOG=1 python3 tools/lsp-mem/lifecycle_leaks.py --poison   # same, labelled

--poison sets TSRS_ARENA_POISON=1 TSRS_REGION_LOG=1 for the servers. Poison keeps freed arena memory mapped, so RSS
then grows by what each cycle frees; that run is for crashes, wrong answers (the per-cycle check of the response
contents), and the region log's created/freed balance (printed from the server's stderr when present).

RSS counts memory the allocator has freed but not yet returned (and on macOS, pages it marked reusable), so a
slope alone does not say what grows. Two readings separate that from retained data:
  --vmmap   macOS: `vmmap` at the end of the warmup and at the last cycle (the raw output is kept in the log
            directory) attributes the growth to the arena reservation, other anonymous memory (mimalloc), system
            malloc and stacks, and reports the physical footprint, which excludes reusable pages.
  --split   runs each scenario twice, for a quarter of the cycles and for all of them, with TSRS_MEM_SPLIT=1, and
            divides the difference of the servers' exit reports (`tsrs_core::memsplit`) by the extra cycles: bytes
            per cycle that stay *used* in the thread arenas (never reclaimed: an allocation made outside any
            region) and *live* in the heap (reachable malloc blocks), against what is merely resident.
The watch and LSP processes are never asked to emit; the synthetic project is written under --work only, and a
--project of your own gets its leaf file and tsconfig.json restored at the end.
"""
import argparse
import copy
import json
import os
import queue
import re
import shutil
import signal
import subprocess
import sys
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, os.path.join(REPO, "tools", "oracle", "lsp"))
from lsp_oracle import Server, file_uri, language_id, offset_to_position  # noqa: E402

TSRS = os.path.join(REPO, "target", "release", "tsrs")
SCENARIOS = ["watch", "watch-root", "lsp-edit", "lsp-open-close", "lsp-reload", "api-snapshot", "api-update", "api-build"]
WATCHING_RE = re.compile(r"Watching for file changes")
FOUND_RE = re.compile(r"Found (\d+) errors?")
REGION_RE = re.compile(r"^regions: (program \S+: (created|freed)|checker .*)")
TYPED = "let zz = 1; "
TYPE_ERROR = "\nexport const lifecycleLeakError: number = 'not a number';\n"


# ---------------------------------------------------------------- measurement

def rss_bytes(pid):
    out = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
    return int(out) * 1024 if out else None


VMMAP_ROW = re.compile(r"^(\S[^\[]*?)\s+([0-9a-f]+)-([0-9a-f]+)\s+\[\s*(\S+)\s+(\S+)\s+(\S+)\s+(\S+)\]")


def vmmap_size(s):
    unit = {"K": 1 << 10, "M": 1 << 20, "G": 1 << 30, "T": 1 << 40}.get(s[-1])
    return int(float(s[:-1]) * unit) if unit else int(s)


def vmmap_split(pid, save=None):
    """macOS: resident bytes of the process by class, from `vmmap` (raw output written to `save`). The arena
    reservation (one address range of tens of GiB, `tsrs_core::reserve`) is the longest contiguous run of
    VM_ALLOCATE regions; the rest of VM_ALLOCATE is mimalloc's segments and other anonymous memory; MALLOC_* is the
    system allocator. `footprint` is the physical footprint, which leaves out pages marked reusable."""
    if sys.platform != "darwin":
        return None
    out = subprocess.run(["vmmap", str(pid)], capture_output=True, text=True).stdout
    if save:
        with open(save, "w") as f:
            f.write(out)
    footprint = re.search(r"^Physical footprint:\s+(\S+)", out, re.M)
    rows = []
    for line in out.splitlines():
        m = VMMAP_ROW.match(line)
        if not m:
            continue
        kind = m.group(1).strip()
        rows.append((kind, int(m.group(2), 16), int(m.group(3), 16), vmmap_size(m.group(4)), vmmap_size(m.group(5)),
                     vmmap_size(m.group(6))))
    # vmmap prints each region twice (non-writable and writable tables); keep one copy per start address.
    rows = list({(r[1], r[0]): r for r in rows}.values())
    rows.sort(key=lambda r: r[1])
    runs, cur = [], []
    for r in rows:
        if r[0] == "VM_ALLOCATE" and cur and cur[-1][2] == r[1]:
            cur.append(r)
        elif r[0] == "VM_ALLOCATE":
            cur = [r]
            runs.append(cur)
    arena = max(runs, key=lambda run: sum(r[3] for r in run), default=[])
    arena_ids = {(r[1], r[0]) for r in arena}
    split = {"arena_resident": 0, "arena_dirty": 0, "anon_resident": 0, "anon_dirty": 0, "malloc_resident": 0,
             "stack_resident": 0, "other_resident": 0, "arena_vsize": sum(r[3] for r in arena),
             "footprint": vmmap_size(footprint.group(1)) if footprint else 0}
    for r in rows:
        kind, _, _, _, resident, dirty = r
        if (r[1], kind) in arena_ids:
            split["arena_resident"] += resident
            split["arena_dirty"] += dirty
        elif kind == "VM_ALLOCATE":
            split["anon_resident"] += resident
            split["anon_dirty"] += dirty
        elif kind.startswith("MALLOC"):
            split["malloc_resident"] += resident
        elif kind.startswith("Stack"):
            split["stack_resident"] += resident
        else:
            split["other_resident"] += resident
    return split


def fit(samples, warmup):
    """samples: [(cycle, rss_bytes)]. Returns slope statistics over the cycles after `warmup`."""
    tail = [(c, r) for c, r in samples if c > warmup and r is not None]
    if len(tail) < 3:
        return {"n": len(tail)}

    def slope(points):
        n = len(points)
        mx = sum(c for c, _ in points) / n
        my = sum(r for _, r in points) / n
        sxx = sum((c - mx) ** 2 for c, _ in points)
        if sxx == 0:
            return 0.0, 0.0
        s = sum((c - mx) * (r - my) for c, r in points) / sxx
        ss_tot = sum((r - my) ** 2 for _, r in points)
        ss_res = sum((r - (my + s * (c - mx))) ** 2 for c, r in points)
        r2 = 1 - ss_res / ss_tot if ss_tot else 1.0
        return s, r2

    s, r2 = slope(tail)
    half = tail[len(tail) // 2:]
    s_last, _ = slope(half)
    diffs = sorted(b - a for (_, a), (_, b) in zip(tail, tail[1:]))
    med = diffs[len(diffs) // 2]
    return {
        "n": len(tail),
        "first_rss": tail[0][1],
        "last_rss": tail[-1][1],
        "total_growth": tail[-1][1] - tail[0][1],
        "slope": s,
        "r2": r2,
        "slope_last_half": s_last,
        "median_diff": med,
    }


def classify(f):
    """A first reading of the fit; the note refines it with the owning code path."""
    if "slope" not in f:
        return "too few samples"
    noise = 4 * 16384  # a few pages per cycle: allocator and sampling jitter
    if f["median_diff"] <= 0 and f["slope"] <= noise:
        return "steady"
    if f["slope"] <= noise and f["slope_last_half"] <= noise:
        return "steady"
    if f["slope_last_half"] <= noise / 4:
        return "one-time growth (settles)"
    if f["r2"] >= 0.8 and f["median_diff"] > 0:
        return "linear growth: leak candidate"
    return "growth, noisy: needs investigation"


def fmt_bytes(b):
    if b is None:
        return "-"
    a = abs(b)
    for unit, div in (("GiB", 1 << 30), ("MiB", 1 << 20), ("KiB", 1 << 10)):
        if a >= div:
            return f"{b / div:.1f} {unit}"
    return f"{round(b)} B"


# ---------------------------------------------------------------- synthetic project

def write_project(root, modules):
    src = os.path.join(root, "src")
    os.makedirs(src, exist_ok=True)
    with open(os.path.join(root, "tsconfig.json"), "w") as f:
        json.dump({"compilerOptions": {"strict": True, "target": "es2022", "module": "esnext",
                                       "moduleResolution": "bundler", "noEmit": True, "types": []},
                   "include": ["src"]}, f, indent=2)
    with open(os.path.join(src, "m0.ts"), "w") as f:
        f.write("export interface Base<T> { value: T; next?: Base<T>; tag: string }\n"
                "export type Pick1<T> = T extends Base<infer U> ? U : never;\n"
                "export function base<T>(value: T): Base<T> { return { value, tag: 'base' }; }\n"
                "export const basis: number = 1;\n")
    for i in range(1, modules):
        prev = i - 1
        with open(os.path.join(src, f"m{i}.ts"), "w") as f:
            f.write(f"import {{ Base, base, Pick1 }} from './m0';\nimport * as p from './m{prev}';\n")
            f.write(f"export interface I{i}<T> extends Base<T> {{ field{i}: T[]; prev: typeof p }}\n")
            f.write(f"export type U{i} = " + " | ".join(f"I{i}<{t}>" for t in ("number", "string", "boolean", "Date")) + ";\n")
            f.write(f"export function f{i}<T extends string>(x: T): Pick1<Base<T>> {{ return base(x).value; }}\n")
            f.write(f"export const c{i} = f{i}('m{i}').length + p.{'basis' if prev == 0 else f'c{prev}'};\n")
    with open(os.path.join(src, "hub.ts"), "w") as f:
        f.write("".join(f"import {{ c{i} }} from './m{i}';\n" for i in range(1, modules)))
        f.write("export const total = " + (" + ".join(f"c{i}" for i in range(1, modules)) or "0") + ";\n")
    with open(os.path.join(src, "leaf.ts"), "w") as f:
        f.write("export function leaf(n: number): string {\n  const parts: string[] = [];\n"
                "  for (let i = 0; i < n; i++) {\n    parts.push(String(i));\n  }\n  return parts.join(',');\n}\n")


# ---------------------------------------------------------------- watch

class Watch:
    def __init__(self, cmd, cwd, env, log):
        self.lines = queue.Queue()
        self.log = open(log, "w")
        self.proc = subprocess.Popen(cmd, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                     text=True, bufsize=1, errors="replace")
        threading.Thread(target=self._pump, daemon=True).start()

    def _pump(self):
        for line in self.proc.stdout:
            self.lines.put(line)
            self.log.write(line)
        self.lines.put(None)

    def settle(self, quiet, timeout):
        """Waits for a 'Watching for file changes' report, then `quiet` seconds without output."""
        deadline = time.time() + timeout
        found = None
        reports = 0
        while True:
            wait = quiet if reports else max(0.0, deadline - time.time())
            try:
                line = self.lines.get(timeout=wait)
            except queue.Empty:
                if not reports:
                    raise TimeoutError(f"no watch report within {timeout:g} s")
                return found, reports
            if line is None:
                raise RuntimeError(f"the watch exited (status {self.proc.poll()})")
            if WATCHING_RE.search(line):
                m = FOUND_RE.search(line)
                found = int(m.group(1)) if m else found
                reports += 1

    def stop(self):
        self.proc.send_signal(signal.SIGINT)
        try:
            self.proc.wait(timeout=15)
        except subprocess.TimeoutExpired:
            self.proc.kill()
        self.log.close()


def run_watch(args, root, env, log_dir, edit_root):
    rel = "src/m0.ts" if edit_root else args.file
    path = os.path.join(root, rel)
    original = open(path, encoding="utf-8").read()
    w = Watch([args.tsrs, "--watch", "-p", root, "--noEmit", "--preserveWatchOutput", "--pretty", "false"],
              root, env, os.path.join(log_dir, "watch.log"))
    samples, errors, checks, splits = [], 0, 0, {}
    try:
        base_found, _ = w.settle(args.quiet, args.timeout)
        samples.append((0, rss_bytes(w.proc.pid)))
        for i in range(1, args.cycles + 1):
            if args.edit == "flip" and i % 2 == 1:
                text, expect = original + TYPE_ERROR, (base_found or 0) + 1
            elif args.edit == "flip":
                text, expect = original, base_found
            else:
                text, expect = original + f"\n// lifecycle edit {i}\n", base_found
            with open(path, "w") as f:
                f.write(text)
            found, _ = w.settle(args.quiet, args.timeout)
            checks += 1
            if expect is not None and found != expect:
                errors += 1
            samples.append((i, rss_bytes(w.proc.pid)))
            progress(args, i, samples[-1][1], w.proc.pid, splits)
    finally:
        with open(path, "w") as f:
            f.write(original)
        w.stop()
    log = os.path.join(log_dir, "watch.log")
    return samples, errors, checks, region_balance(log), splits, exit_split(log)


# ---------------------------------------------------------------- lsp

def lsp_init(server, root, timeout):
    r = server.request("initialize", {
        "processId": None,
        "rootUri": file_uri(root),
        "workspaceFolders": [{"uri": file_uri(root), "name": os.path.basename(root)}],
        "capabilities": {
            "textDocument": {"diagnostic": {"dynamicRegistration": False},
                             "hover": {"contentFormat": ["markdown", "plaintext"]}},
            "workspace": {"configuration": True, "workspaceFolders": True,
                          "didChangeWatchedFiles": {"dynamicRegistration": True}},
            "general": {"positionEncodings": ["utf-16"]},
        },
    }, timeout)
    if "error" in r:
        raise RuntimeError(f"initialize failed: {r['error']}")
    server.notify("initialized", {})


def diag_count(resp):
    return len(((resp.get("result") or {}).get("items")) or [])


def run_lsp(args, root, env, log_dir, mode):
    path = os.path.join(root, args.file)
    text = open(path, encoding="utf-8").read()
    uri = file_uri(path)
    cfg = os.path.join(root, "tsconfig.json")
    cfg_text = open(cfg, encoding="utf-8").read()
    for k, v in env.items():
        os.environ[k] = v
    log = os.path.join(log_dir, f"{mode}.log")
    server = Server("tsrs", [args.tsrs, "--lsp", "-stdio"], root, log)
    pid = server.proc.pid
    samples, errors, checks, splits = [], 0, 0, {}
    try:
        lsp_init(server, root, args.timeout)
        open_doc = {"textDocument": {"uri": uri, "languageId": language_id(path), "version": 1, "text": text}}
        if mode != "lsp-open-close":
            server.notify("textDocument/didOpen", open_doc)
        first = server.request("textDocument/diagnostic", {"textDocument": {"uri": uri}}, args.timeout) \
            if mode != "lsp-open-close" else {"result": {"items": []}}
        if "error" in first:
            raise RuntimeError(f"first diagnostic failed: {first['error']}")
        base = diag_count(first)
        samples.append((0, rss_bytes(pid)))
        at = text.rfind("return")
        cur, typed, direction = text, 0, 1
        version = 1
        for i in range(1, args.cycles + 1):
            if mode == "lsp-edit":
                if direction > 0:
                    ch = TYPED[typed]
                    pos = offset_to_position(cur, at + typed)
                    change = {"range": {"start": pos, "end": pos}, "text": ch}
                    cur = cur[:at + typed] + ch + cur[at + typed:]
                    typed += 1
                    if typed == len(TYPED):
                        direction = -1
                else:
                    start = offset_to_position(cur, at + typed - 1)
                    end = offset_to_position(cur, at + typed)
                    change = {"range": {"start": start, "end": end}, "text": ""}
                    cur = cur[:at + typed - 1] + cur[at + typed:]
                    typed -= 1
                    if typed == 0:
                        direction = 1
                version += 1
                server.notify("textDocument/didChange", {"textDocument": {"uri": uri, "version": version},
                                                         "contentChanges": [change]})
                d = server.request("textDocument/diagnostic", {"textDocument": {"uri": uri}}, args.timeout)
                h = server.request("textDocument/hover", {"textDocument": {"uri": uri},
                                                          "position": offset_to_position(cur, at)}, args.timeout)
                checks += 1
                # Half-typed statements have their own errors; the count is checked when the text is back.
                errors += ("error" in d) + ("error" in h) + (typed == 0 and diag_count(d) != base)
            elif mode == "lsp-open-close":
                server.notify("textDocument/didOpen", open_doc)
                d = server.request("textDocument/diagnostic", {"textDocument": {"uri": uri}}, args.timeout)
                server.notify("textDocument/didClose", {"textDocument": {"uri": uri}})
                checks += 1
                errors += ("error" in d)
                if i == 1:
                    base = diag_count(d)
                elif diag_count(d) != base:
                    errors += 1
            elif mode == "lsp-reload":
                with open(cfg, "w") as f:
                    f.write(cfg_text.rstrip() + (f"\n// reload {i}\n" if i % 2 else "\n"))
                server.notify("workspace/didChangeWatchedFiles",
                              {"changes": [{"uri": file_uri(cfg), "type": 2}]})
                d = server.request("textDocument/diagnostic", {"textDocument": {"uri": uri}}, args.timeout)
                checks += 1
                errors += ("error" in d) + (diag_count(d) != base)
            server.drain(args.settle)
            samples.append((i, rss_bytes(pid)))
            progress(args, i, samples[-1][1], pid, splits)
    finally:
        with open(cfg, "w") as f:
            f.write(cfg_text)
        try:
            server.request("shutdown", None, 30)
            server.send({"jsonrpc": "2.0", "method": "exit"})
            server.proc.stdin.close()
            server.proc.wait(timeout=10)
        except Exception:
            server.proc.kill()
    return samples, errors, checks, region_balance(log + ".stderr"), splits, exit_split(log + ".stderr")


# ---------------------------------------------------------------- api

class Api:
    def __init__(self, tsrs, root, env, log, timeout):
        for k, v in env.items():
            os.environ[k] = v
        self.server = Server("tsrs-api", [tsrs, "--api", "--async", "--cwd", root], root, log)
        self.timeout = timeout
        self.log = log

    def call(self, method, params):
        r = self.server.request(method, params, self.timeout)
        if "error" in r:
            raise RuntimeError(f"{method}: {r['error']}")
        return r.get("result")

    def close(self):
        try:
            self.server.proc.stdin.close()
            self.server.proc.wait(timeout=10)
        except Exception:
            self.server.proc.kill()


def run_api(args, root, env, log_dir, mode):
    cfg = os.path.join(root, "tsconfig.json")
    leaf = os.path.join(root, args.file)
    original = open(leaf, encoding="utf-8").read()
    log = os.path.join(log_dir, f"{mode}.log")
    api = Api(args.tsrs, root, env, log, args.timeout)
    pid = api.server.proc.pid
    samples, errors, checks, splits = [], 0, 0, {}
    try:
        api.call("initialize", None)
        samples.append((0, rss_bytes(pid)))
        base_snap = None
        base_diags = None
        for i in range(1, args.cycles + 1):
            if mode == "api-build":
                r = api.call("createBuildOrchestrator", {"rootNames": [cfg], "cwd": root, "buildOptions": {}})
                bid = r.get("buildOrchestratorID") if isinstance(r, dict) else r
                b = api.call("build", {"buildOrchestratorID": bid, "project": cfg})
                api.call("disposeBuildOrchestrator", {"buildOrchestratorID": bid})
                checks += 1
                if not isinstance(b, dict):
                    errors += 1
            else:
                if mode == "api-update" and base_snap is not None:
                    with open(leaf, "w") as f:
                        f.write(original + f"\n// update {i}\n")
                    r = api.call("updateSnapshot", {"snapshot": base_snap["snapshot"], "changes": {
                        "fileNotifications": {"changed": [leaf]}, "ensurePrograms": [base_snap["project"]]}})
                else:
                    r = api.call("createSnapshot", {"openProjects": [cfg]})
                snap = r["snapshot"]
                projects = r.get("projects") or []
                project = projects[0]["id"] if projects else (base_snap or {}).get("project")
                if args.no_check:
                    d, n = [], 0
                else:
                    d = api.call("getSemanticDiagnostics", {"snapshot": snap, "project": project, "files": [leaf]})
                    n = len(d) if isinstance(d, list) else -1
                checks += 1
                if base_diags is None:
                    base_diags = n
                elif n != base_diags:
                    errors += 1
                if mode == "api-update":
                    if base_snap is not None:
                        api.call("release", {"snapshot": base_snap["snapshot"]})
                    base_snap = {"snapshot": snap, "project": project}
                else:
                    api.call("release", {"snapshot": snap})
            samples.append((i, rss_bytes(pid)))
            progress(args, i, samples[-1][1], pid, splits)
        if base_snap is not None:
            api.call("release", {"snapshot": base_snap["snapshot"]})
    finally:
        with open(leaf, "w") as f:
            f.write(original)
        api.close()
    return samples, errors, checks, region_balance(log + ".stderr"), splits, exit_split(log + ".stderr")


# ---------------------------------------------------------------- driver

def region_balance(stderr_path):
    """Counts `TSRS_REGION_LOG=1` lines: programs created and freed, checkers freed and parked."""
    counts = {"program_created": 0, "program_freed": 0, "checker_freed": 0, "checker_parked": 0}
    try:
        with open(stderr_path, errors="replace") as f:
            for line in f:
                if not line.startswith("regions: "):
                    continue
                if "program" in line and "created" in line:
                    counts["program_created"] += 1
                elif "program" in line and "freed" in line:
                    counts["program_freed"] += 1
                elif "checker" in line and "freed" in line:
                    counts["checker_freed"] += 1
                elif "checker" in line and ("parked" in line or "pinned" in line or "escaped" in line):
                    counts["checker_parked"] += 1
    except OSError:
        return None
    return counts if any(counts.values()) else None


EXIT_ARENAS = re.compile(r"thread arenas: (\d+) arenas, ([\d.]+) MiB capacity, ([\d.]+) MiB used")
EXIT_HEAP = re.compile(r"heap \(allocator walk\): (\d+) pages, ([\d.]+) MiB initialized blocks, ([\d.]+) MiB live blocks, "
                       r"([\d.]+) MiB resident")


def exit_split(path):
    """The server's `tsrs mem split: exit` report (TSRS_MEM_SPLIT=1, `tsrs_core::memsplit`), in bytes: what the
    thread arenas hold at exit (`arena_used`; nothing frees it before the process ends), the heap's reachable
    blocks (`heap_live`) and the heap's resident pages (`heap_resident`). None when the log has no report."""
    try:
        text = open(path, errors="replace").read()
    except OSError:
        return None
    block = text.rsplit("tsrs mem split: exit", 1)
    if len(block) < 2:
        return None
    arenas, heap = EXIT_ARENAS.search(block[1]), EXIT_HEAP.search(block[1])
    if not arenas or not heap:
        return None
    mib = 1 << 20
    return {"arena_capacity": float(arenas.group(2)) * mib, "arena_used": float(arenas.group(3)) * mib,
            "heap_live": float(heap.group(3)) * mib, "heap_resident": float(heap.group(4)) * mib}


def progress(args, i, rss, pid=None, splits=None):
    if args.every and i % args.every == 0:
        print(f"    cycle {i:4d}  rss {fmt_bytes(rss):>10}", flush=True)
    if args.vmmap and splits is not None and i in (args.warmup, args.cycles):
        save = os.path.join(args.log_dir, f"{args.scenario_now}-vmmap-{i}.txt") if getattr(args, "log_dir", None) else None
        splits["warmup" if i == args.warmup else "end"] = vmmap_split(pid, save)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--scenario", action="append", choices=SCENARIOS, help="default: all")
    ap.add_argument("--tsrs", default=TSRS)
    ap.add_argument("--project", help="project directory (default: a synthetic one under --work)")
    ap.add_argument("--file", default="src/leaf.ts", help="leaf file to edit / open, relative to the project")
    ap.add_argument("--modules", type=int, default=40, help="modules of the synthetic project")
    ap.add_argument("--work", default=os.path.join(REPO, "target", "scratch", "mem", "lifecycle"))
    ap.add_argument("--cycles", type=int, default=60)
    ap.add_argument("--warmup", type=int, default=15, help="cycles excluded from the slope")
    ap.add_argument("--every", type=int, default=10, help="print a sample every N cycles (0: none)")
    ap.add_argument("--edit", choices=["comment", "flip"], default="comment", help="watch: kind of leaf edit")
    ap.add_argument("--quiet", type=float, default=0.5, help="watch: seconds without output that end a cycle")
    ap.add_argument("--settle", type=float, default=0.05, help="lsp: seconds to drain notifications per cycle")
    ap.add_argument("--timeout", type=float, default=300)
    ap.add_argument("--poison", action="store_true", help="TSRS_ARENA_POISON=1 TSRS_REGION_LOG=1 for the servers")
    ap.add_argument("--no-check", action="store_true", help="api: skip getSemanticDiagnostics (create/release only)")
    ap.add_argument("--vmmap", action="store_true",
                    help="macOS: attribute the growth between warmup and end to the arena reservation, other anonymous "
                         "memory (mimalloc), system malloc and stacks with `vmmap`, and report the physical footprint")
    ap.add_argument("--split", action="store_true",
                    help="run each scenario for a quarter of the cycles and for all of them with TSRS_MEM_SPLIT=1 and "
                         "report, per extra cycle, the bytes that stay used in the thread arenas and live in the heap")
    ap.add_argument("--env", action="append", default=[], help="KEY=VALUE for the server environment")
    ap.add_argument("--json")
    args = ap.parse_args()

    scenarios = args.scenario or SCENARIOS
    args.work = os.path.abspath(args.work)
    args.tsrs = os.path.abspath(args.tsrs)
    os.makedirs(args.work, exist_ok=True)
    if args.project:
        root = os.path.abspath(args.project)
    else:
        root = os.path.join(args.work, f"synthetic-{args.modules}")
        if os.path.isdir(root):
            shutil.rmtree(root)
        write_project(root, args.modules)
    env = dict(os.environ)
    if args.poison:
        env["TSRS_ARENA_POISON"] = "1"
        env["TSRS_REGION_LOG"] = "1"
    if args.split:
        env["TSRS_MEM_SPLIT"] = "1"
    for kv in args.env:
        k, _, v = kv.partition("=")
        env[k] = v
    log_dir = os.path.join(args.work, "logs", time.strftime("%Y%m%d-%H%M%S") + ("-poison" if args.poison else ""))
    os.makedirs(log_dir, exist_ok=True)
    version = subprocess.run([args.tsrs, "--version"], capture_output=True, text=True).stdout.strip()
    print(f"{version}\nproject {root} ({'synthetic, ' + str(args.modules) + ' modules' if not args.project else 'yours'}), "
          f"{args.cycles} cycles, warmup {args.warmup}{', poison' if args.poison else ''}", flush=True)

    def run_scenario(sc, run_args, run_log_dir):
        os.makedirs(run_log_dir, exist_ok=True)
        run_args.log_dir, run_args.scenario_now = run_log_dir, sc
        if sc.startswith("watch"):
            return run_watch(run_args, root, env, run_log_dir, sc == "watch-root")
        if sc.startswith("lsp"):
            return run_lsp(run_args, root, env, run_log_dir, sc)
        return run_api(run_args, root, env, run_log_dir, sc)

    results = []
    for sc in scenarios:
        print(f"\n== {sc}", flush=True)
        t0 = time.time()
        short, short_cycles = None, max(args.cycles // 4, 2)
        try:
            if args.split:
                # The short run first: its exit report is the baseline the full run's report is compared with.
                short_args = copy.copy(args)
                short_args.cycles, short_args.warmup, short_args.every = short_cycles, min(args.warmup, short_cycles // 2), 0
                short = run_scenario(sc, short_args, os.path.join(log_dir, f"short-{short_cycles}"))[5]
                if short is None:
                    raise RuntimeError("the short run printed no `tsrs mem split: exit` report (TSRS_MEM_SPLIT=1)")
            samples, errors, checks, regions, splits, at_exit = run_scenario(sc, args, log_dir)
            failure = None
        except Exception as e:  # noqa: BLE001 - one scenario's failure is a result, not the end of the run
            samples, errors, checks, regions, splits, at_exit = [], 0, 0, None, {}, None
            failure = f"{type(e).__name__}: {e}"
        f = fit(samples, args.warmup)
        r = {"scenario": sc, "samples": samples, "fit": f, "errors": errors, "checks": checks,
             "regions": regions, "splits": splits, "exit": at_exit, "failure": failure,
             "seconds": round(time.time() - t0, 1), "class": "failed" if failure else classify(f)}
        results.append(r)
        if short and at_exit:
            n = args.cycles - short_cycles
            r["exit_per_cycle"] = {k: (at_exit[k] - short[k]) / n for k in ("arena_used", "heap_live", "heap_resident")}
            print(f"   exit split per cycle ({short_cycles} -> {args.cycles} cycles): thread arenas used "
                  f"{fmt_bytes(r['exit_per_cycle']['arena_used'])}, heap live {fmt_bytes(r['exit_per_cycle']['heap_live'])}, "
                  f"heap resident {fmt_bytes(r['exit_per_cycle']['heap_resident'])}")
        if splits.get("warmup") and splits.get("end"):
            n = max(1, args.cycles - args.warmup)
            delta = {k: (splits["end"][k] - splits["warmup"][k]) for k in splits["end"] if k != "arena_vsize"}
            r["split_per_cycle"] = {k: v / n for k, v in delta.items()}
            print("   vmmap growth per cycle (warmup -> end): " + ", ".join(
                f"{k.replace('_resident', '').replace('_dirty', ' dirty')} {fmt_bytes(v / n)}"
                for k, v in delta.items()))
            r["footprint"] = {"warmup": splits["warmup"]["footprint"], "end": splits["end"]["footprint"]}
        if failure:
            print(f"   FAILED: {failure}")
        else:
            print(f"   start {fmt_bytes(samples[0][1])}, end {fmt_bytes(samples[-1][1])}, "
                  f"{'slope ' + fmt_bytes(f['slope']) + '/cycle (last half ' + fmt_bytes(f['slope_last_half']) + ', median diff ' + fmt_bytes(f['median_diff']) + ', r2 ' + format(f['r2'], '.2f') + ')' if 'slope' in f else 'too few samples'}; "
                  f"{errors} wrong answers of {checks}; {r['seconds']} s -> {r['class']}"
                  + (f"; regions {regions}" if regions else ""))

    extra = " arena used/cycle | heap live/cycle |" if args.split else ""
    print(f"\n| scenario | start | end | bytes/cycle (tail) | last half | median diff | r2 | wrong answers |{extra} reading |")
    print("| --- | --- | --- | --- | --- | --- | --- | --- |" + " --- | --- |" * bool(args.split) + " --- |")
    for r in results:
        f = r["fit"]
        if r["failure"] or "slope" not in f:
            print(f"| {r['scenario']} | - | - | - | - | - | - | - |{' - | - |' if args.split else ''} {r['failure'] or r['class']} |")
            continue
        e = r.get("exit_per_cycle")
        cols = f" {fmt_bytes(e['arena_used']) if e else '-'} | {fmt_bytes(e['heap_live']) if e else '-'} |" if args.split else ""
        print(f"| {r['scenario']} | {fmt_bytes(r['samples'][0][1])} | {fmt_bytes(r['samples'][-1][1])} | "
              f"{fmt_bytes(f['slope'])} | {fmt_bytes(f['slope_last_half'])} | {fmt_bytes(f['median_diff'])} | "
              f"{f['r2']:.2f} | {r['errors']}/{r['checks']} |{cols} {r['class']} |")
    if args.json:
        with open(args.json, "w") as out:
            json.dump({"tsrs": version, "project": root, "cycles": args.cycles, "warmup": args.warmup,
                       "poison": args.poison, "results": results}, out, indent=1)


if __name__ == "__main__":
    main()
