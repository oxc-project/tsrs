#!/usr/bin/env bash
# Published macOS arm64 compilers: @tsrs/darwin-arm64@latest vs typescript@next.
# Usage: ./bench/laptop.sh [--runs 10] [--warmup 3]
# Raw samples only: Aligned and Maximum resources, four pinned public projects.
set -euo pipefail

bench_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
exec python3 - "$bench_dir" "$@" <<'PY'
import argparse
import csv
import datetime as dt
import hashlib
import io
import json
import os
import platform
import re
import shlex
import shutil
import subprocess
import sys
import tarfile
import urllib.request
from collections import Counter
from pathlib import Path

BENCH = Path(sys.argv.pop(1))
sys.path.insert(0, str(BENCH))
import run as setup

PROJECTS = ("vscode", "mui-docs", "webpack", "t3code-server")
REGISTRY = "https://registry.npmjs.org/"
METRICS = "time_wall_clock:s,instructions:B,memory_peak_resident:GiB"
HF_VERSION = "2.0.0"
HF_ARCHIVE_SHA256 = "590b839cfea386b384ce0b42d16daf2910277cf30950656194bb3a5f20dea81e"
HF_BINARY_SHA256 = "dc9f035feaa4f5e25985c0d36a9c1b60985ebd159652908f0ba65a31645ac8dc"


def log(message):
    print(message, file=sys.stderr, flush=True)


def fail(message):
    raise SystemExit(message)


def output(argv, **kwargs):
    return subprocess.check_output([str(a) for a in argv], text=True, **kwargs).strip()


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, data):
    path.write_text(json.dumps(data, indent=2) + "\n")


def download(url):
    with urllib.request.urlopen(url, timeout=120) as response:
        return response.read()


def release(package, tag):
    # Resolve each moving tag once; installation and every run use that exact version.
    data = json.loads(download(REGISTRY + package + "/" + tag))
    if data.get("name") != package or not data.get("version"):
        fail(f"Invalid npm metadata for {package}@{tag}")
    return data


def hyperfine(work, override):
    if override:
        exe = override.resolve()
    else:
        exe = work / "tools" / f"hyperfine-{HF_VERSION}"
        if not exe.exists() or sha256(exe) != HF_BINARY_SHA256:
            log(f"Downloading hyperfine {HF_VERSION}")
            archive = download("https://github.com/sharkdp/hyperfine/releases/download/"
                               f"v{HF_VERSION}/hyperfine-v{HF_VERSION}-aarch64-apple-darwin.tar.gz")
            if hashlib.sha256(archive).hexdigest() != HF_ARCHIVE_SHA256:
                fail("Hyperfine archive checksum mismatch")
            with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as tar:
                member = next(m for m in tar.getmembers() if m.isfile() and Path(m.name).name == "hyperfine")
                binary = tar.extractfile(member).read()
            if hashlib.sha256(binary).hexdigest() != HF_BINARY_SHA256:
                fail("Hyperfine binary checksum mismatch")
            exe.parent.mkdir(parents=True, exist_ok=True)
            exe.write_bytes(binary)
            exe.chmod(0o755)
    if "--metrics" not in output([exe, "--help"]):
        fail("Hyperfine 2 or newer is required for instructions and peak RSS")
    return exe


def install_compilers(work, result_dir):
    tsrs = release("@tsrs/darwin-arm64", "latest")
    typescript = release("typescript", "next")
    native_name = "@typescript/typescript-darwin-arm64"
    native_version = typescript.get("optionalDependencies", {}).get(native_name)
    if not native_version:
        fail("typescript@next does not declare the native macOS arm64 compiler")
    native = release(native_name, native_version)
    packages = [tsrs, typescript, native]
    for package in packages:
        log(f"Using {package['name']}@{package['version']}")
    install = work / "packages" / f"tsrs-{tsrs['version']}_typescript-{typescript['version']}"
    install.mkdir(parents=True, exist_ok=True)
    if not (install / "package.json").exists():
        write_json(install / "package.json", {"name": "laptop-benchmark", "private": True})
    subprocess.run(["npm", "install", "--prefix", str(install), "--registry", REGISTRY,
                    "--cache", str(work / "npm-cache"), "--ignore-scripts", "--no-audit", "--no-fund",
                    "--save-exact", *[p["name"] + "@" + p["version"] for p in packages]], check=True)
    shutil.copyfile(install / "package-lock.json", result_dir / "package-lock.json")
    executables = {
        "typescript": install / "node_modules" / native_name / "lib" / "tsc",
        "tsrs": install / "node_modules" / tsrs["name"] / "tsrs",
    }
    metadata = {}
    for compiler, package in (("typescript", native), ("tsrs", tsrs)):
        exe = executables[compiler]
        installed = json.loads((install / "node_modules" / package["name"] / "package.json").read_text())
        if installed["version"] != package["version"]:
            fail(f"Installed {compiler} version does not match the resolved release")
        with exe.open("rb") as binary:
            if binary.read(4) not in (b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe"):
                fail(f"{exe} is not a native Mach-O executable")
        metadata[compiler] = {"package": package["name"], "version": package["version"],
                              "requested": "typescript@next" if compiler == "typescript" else "@tsrs/darwin-arm64@latest",
                              "dist": package["dist"], "binary": str(exe), "sha256": sha256(exe),
                              "version_output": output([exe, "--version"])}
    write_json(result_dir / "npm-packages.json", packages)
    return executables, metadata


def clean_environment(cores):
    prefixes = ("TSRS_", "TS_TEST_", "RAYON_", "MIMALLOC_", "MALLOC_", "Malloc")
    removed = {k for k in os.environ if k.startswith(prefixes)} | {"GOGC", "GOMEMLIMIT", "GOMAXPROCS", "GODEBUG"}
    env = {k: v for k, v in os.environ.items() if k not in removed}
    env.update(GOMAXPROCS=str(cores), RAYON_NUM_THREADS=str(cores))
    return env, sorted(removed)


def check_log(path):
    text = path.read_text(errors="replace")
    if any(setup.CRASH_RE.match(line) for line in text.splitlines()):
        fail(f"Compiler crash; see {path}")
    return text


def preflight(command, cwd, env, path):
    # These statistics can force Go GC, so collect them outside every timed run.
    with path.open("w") as stream:
        result = subprocess.run(command + ["--extendedDiagnostics"], cwd=cwd, env=env,
                                stdout=stream, stderr=subprocess.STDOUT, timeout=900)
    text = check_log(path)
    files = re.search(r"^Files:\s+(\d+)", text, re.M)
    if result.returncode not in (0, 1, 2) or not files or int(files[1]) == 0:
        fail(f"Compiler did not complete its preflight; see {path}")
    errors = Counter(m[1] for line in text.splitlines() if (m := setup.ERROR_RE.match(line)))
    return {"exit_code": result.returncode, "files": int(files[1]),
            "errors": sum(errors.values()), "error_keys": dict(sorted(errors.items()))}


def measurements(path, runs, commands):
    data = json.loads(path.read_text())
    if data.get("schema_version") != 2 or len(data["results"]) != commands:
        fail(f"Unexpected Hyperfine JSON schema/results: {path}")
    for result in data["results"]:
        if len(result["measurements"]) != runs:
            fail(f"Missing runs: {path}")
        for sample in result["measurements"]:
            for metric in ("time_wall_clock", "instructions", "memory_peak_resident"):
                if sample.get(metric, {}).get("value", 0) <= 0:
                    fail(f"Missing {metric}; hardware counters are required: {path}")
            if sample["exit_code"] not in (0, 1, 2):
                fail(f"Compiler failed: {path}")
    return data["results"]


def main():
    parser = argparse.ArgumentParser(description="Published tsrs vs TypeScript nightly; raw per-run measurements.")
    parser.add_argument("--runs", type=int, default=10)
    parser.add_argument("--warmup", type=int, default=3)
    parser.add_argument("--projects", default=",".join(PROJECTS), help="subset for a smoke run; default: all four")
    parser.add_argument("--work-dir", type=Path, default=BENCH / ".work" / "laptop")
    parser.add_argument("--hyperfine", type=Path, help="Hyperfine 2 executable; otherwise download verified 2.0.0")
    parser.add_argument("--setup-only", action="store_true")
    args = parser.parse_args()
    if args.runs < 1 or args.warmup < 0:
        parser.error("--runs must be positive and --warmup nonnegative")
    names = args.projects.split(",")
    if len(names) != len(set(names)) or set(names) - set(PROJECTS):
        parser.error("--projects must contain unique names from " + ",".join(PROJECTS))
    if sys.platform != "darwin" or platform.machine() != "arm64":
        fail("This script requires an Apple Silicon Mac (darwin-arm64)")
    for tool in ("npm", "node", "pnpm", "git"):
        if not shutil.which(tool):
            fail(f"Required command not found: {tool}")
    cores = int(output(["/usr/sbin/sysctl", "-n", "hw.logicalcpu"]))
    ram = int(output(["/usr/sbin/sysctl", "-n", "hw.memsize"]))
    # Leave 4 GiB for macOS. Both limits are soft budgets, not identical allocator policies.
    memory_gib = max(1, ram // (1024 ** 3) - 4)
    env, removed = clean_environment(cores)
    work = args.work_dir.resolve()
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    result_dir = work / "results" / stamp
    result_dir.mkdir(parents=True)
    log(f"Results: {result_dir}")
    hf = hyperfine(work, args.hyperfine)
    executables, packages = install_compilers(work, result_dir)
    cfg = json.loads((BENCH / "projects.json").read_text())
    projects = [next(p for p in cfg["projects"] if p["name"] == name) for name in names]
    for project in projects:
        setup.setup_project(cfg, project, work)
    modes = {
        "aligned": {"label": "Aligned", "checkers": 4, "memory_gib": None},
        "maximum": {"label": "Maximum resources", "checkers": cores, "memory_gib": memory_gib},
    }
    metadata = {"started_utc": stamp, "machine": {"cpu": output(["/usr/sbin/sysctl", "-n", "machdep.cpu.brand_string"]),
                "cores": cores, "ram_bytes": ram, "macos": platform.mac_ver()[0]},
                "compilers": packages, "hyperfine": {"version": output([hf, "--version"]), "sha256": sha256(hf)},
                "runs": args.runs, "warmup": args.warmup, "projects": projects, "modes": modes,
                "cleared_environment_variables": removed, "cells": []}
    metadata_path = result_dir / "metadata.json"
    write_json(metadata_path, metadata)
    if args.setup_only:
        return
    # Fail before the matrix if this Mac cannot supply instruction counters.
    probe = result_dir / "counter-probe.json"
    subprocess.run([str(hf), "--shell=none", "--runs", "1", "--warmup", "0", "--style", "none",
                    "--metrics", METRICS, "--export-json", str(probe), "/usr/bin/true"], env=env, check=True)
    measurements(probe, 1, 1)
    fields = ["mode", "project", "compiler", "run", "wall_seconds", "instructions", "peak_rss_bytes", "exit_code"]
    rows = []
    with (result_dir / "raw.csv").open("w", newline="") as raw:
        writer = csv.DictWriter(raw, fieldnames=fields)
        writer.writeheader()
        print("\t".join(fields), flush=True)
        for mode, settings in modes.items():
            mode_env = dict(env)
            if mode == "maximum":
                mode_env["GOMEMLIMIT"] = f"{memory_gib}GiB"
            # Preserve the agreed GC policy: default GOGC in both modes.
            # A high GOMEMLIMIT permits memory use; it does not disable normal Go GC.
            for project in projects:
                name = project["name"]
                cwd, config = setup.project_path(cfg, project, work)
                cell_dir = result_dir / mode / name
                cell_dir.mkdir(parents=True)
                commands = {compiler: [str(exe), "-p", str(config), *setup.TIMED_FLAGS,
                                       "--checkers", str(settings["checkers"])]
                            for compiler, exe in executables.items()}
                if mode == "maximum":
                    commands["tsrs"] += ["--maxMemory", f"{memory_gib}G"]
                log(f"{settings['label']} / {name}: {settings['checkers']} checkers")
                checks = {compiler: preflight(command, cwd, mode_env, cell_dir / f"{compiler}-preflight.log")
                          for compiler, command in commands.items()}
                comparison = {"same_files": checks["typescript"]["files"] == checks["tsrs"]["files"],
                              "same_diagnostic_keys": checks["typescript"]["error_keys"] == checks["tsrs"]["error_keys"]}
                if not all(comparison.values()):
                    log(f"{name}: compiler versions differ in files or diagnostic locations/codes; see preflight.json")
                write_json(cell_dir / "preflight.json", {"compilers": checks, "comparison": comparison})
                hf_json = cell_dir / "hyperfine.json"
                argv = [str(hf), "--shell=none", "--runs", str(args.runs), "--warmup", str(args.warmup),
                        "--metrics", METRICS, "--style", "none", "--output", "inherit",
                        "--ignore-failure=1,2", "--export-json", str(hf_json)]
                for compiler, command in commands.items():
                    argv += ["--command-name", compiler, shlex.join(command)]
                metadata["cells"].append({"mode": mode, "project": name, "cwd": str(cwd), "commands": commands,
                                          "environment": {k: mode_env.get(k) for k in
                                                          ("GOMAXPROCS", "RAYON_NUM_THREADS", "GOGC", "GOMEMLIMIT")},
                                          "hyperfine_argv": argv})
                write_json(metadata_path, metadata)
                log_path = cell_dir / "hyperfine.log"
                with log_path.open("w") as stream:
                    subprocess.run(argv, cwd=cwd, env=mode_env, stdout=stream, stderr=subprocess.STDOUT,
                                   check=True, timeout=900 * 2 * (args.runs + args.warmup))
                check_log(log_path)
                for result in measurements(hf_json, args.runs, 2):
                    compiler = result["name"]
                    for index, sample in enumerate(result["measurements"], 1):
                        if sample["exit_code"] != checks[compiler]["exit_code"]:
                            fail(f"Exit status changed from preflight: {log_path}")
                        row = dict(zip(fields, (mode, name, compiler, index, sample["time_wall_clock"]["value"],
                                                int(sample["instructions"]["value"]),
                                                int(sample["memory_peak_resident"]["value"]), sample["exit_code"])))
                        writer.writerow(row)
                        rows.append(row)
                        print("\t".join(str(row[field]) for field in fields), flush=True)
                raw.flush()
                write_json(result_dir / "raw.json", rows)
    metadata["completed_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
    write_json(metadata_path, metadata)
    log(f"Raw samples: {result_dir / 'raw.csv'}")
    log(f"Raw samples: {result_dir / 'raw.json'}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        fail(f"Benchmark failed: {error}")
PY
