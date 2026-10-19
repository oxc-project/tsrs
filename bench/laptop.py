#!/usr/bin/env python3
"""Published-compiler Hyperfine profile for bench/run.py; see bench/README.md."""
import argparse
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

import run as setup
from results import collect_samples, load_bundle, measurements, write_samples

BENCH = Path(__file__).resolve().parent

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


def install_compilers(work, result_dir, replay=None):
    native_name = "@typescript/typescript-darwin-arm64"
    specifications = (
        ("typescript_7_0_2", "typescript", "7.0.2", "TypeScript"),
        ("typescript", "typescript", "next", "TypeScript nightly"),
        ("tsrs", "@tsrs/darwin-arm64", "latest", "tsrs"),
    )
    executables, metadata, all_packages = {}, {}, {}
    archived = json.loads((replay / "npm-packages.json").read_text()) if replay else None
    for compiler, name, tag, label in specifications:
        requested = f"{name}@{tag}"
        package = archived[compiler][0] if archived else release(name, tag)
        if replay:
            requested = f"{name}@{package['version']}"
        packages = [package]
        if name == "typescript":
            native_version = package.get("optionalDependencies", {}).get(native_name)
            if not native_version:
                fail(f"{requested} does not declare the native macOS arm64 compiler")
            package = archived[compiler][1] if archived else release(native_name, native_version)
            packages.append(package)
        log(f"Using {requested}: {package['version']}")
        # Separate installations keep the stable and nightly TypeScript packages from replacing each other.
        install = work / "packages" / f"{compiler}-{package['version']}"
        install.mkdir(parents=True, exist_ok=True)
        if not (install / "package.json").exists():
            write_json(install / "package.json", {"name": "laptop-benchmark", "private": True})
        if replay:
            lock = json.loads((replay / f"{compiler}-package-lock.json").read_text())
            write_json(install / "package-lock.json", lock)
            write_json(install / "package.json", lock["packages"][""])
        subprocess.run(["npm", "ci" if replay else "install", "--prefix", str(install), "--registry", REGISTRY,
                        "--cache", str(work / "npm-cache"), "--ignore-scripts", "--no-audit", "--no-fund",
                        *([] if replay else ["--save-exact", *[p["name"] + "@" + p["version"] for p in packages]])],
                       check=True)
        shutil.copyfile(install / "package-lock.json", result_dir / f"{compiler}-package-lock.json")
        exe = install / "node_modules" / package["name"] / ("lib/tsc" if name == "typescript" else "tsrs")
        installed = json.loads((install / "node_modules" / package["name"] / "package.json").read_text())
        if installed["version"] != package["version"]:
            fail(f"Installed {compiler} version does not match the resolved release")
        with exe.open("rb") as binary:
            if binary.read(4) not in (b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe"):
                fail(f"{exe} is not a native Mach-O executable")
        executables[compiler] = exe
        all_packages[compiler] = packages
        metadata[compiler] = {"package": package["name"], "version": package["version"], "label": label,
                              "requested": requested,
                              "dist": package["dist"], "binary": str(exe), "sha256": sha256(exe),
                              "version_output": output([exe, "--version"])}
    write_json(result_dir / "npm-packages.json", all_packages)
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


def plan_cells(cfg, projects, modes, executables, work, hf, runs, warmup, workers):
    cells = []
    for mode, settings in modes.items():
        for project in projects:
            cwd, config = setup.project_path(cfg, project, work)
            commands = {compiler: [str(exe), "-p", str(config), *setup.TIMED_FLAGS,
                                   "--checkers", str(settings["checkers"])]
                        for compiler, exe in executables.items()}
            memory = settings["memory_gib"]
            if memory is not None:
                commands["tsrs"] += ["--maxMemory", f"{memory}G"]
            # Relative output paths resolve inside each fresh result directory at execution time.
            destination = f"{mode}/{project['name']}"
            argv = [str(hf), "--shell=none", "--runs", str(runs), "--warmup", str(warmup),
                    "--metrics", METRICS, "--style", "basic", "--output", f"{destination}/compiler.log",
                    "--ignore-failure=1,2", "--export-json", f"{destination}/hyperfine.json"]
            for compiler, command in commands.items():
                argv += ["--command-name", compiler, shlex.join(command)]
            cells.append({"mode": mode, "project": project["name"], "cwd": str(cwd), "commands": commands,
                          "environment": {"GOMAXPROCS": str(workers), "RAYON_NUM_THREADS": str(workers),
                                          "GOGC": None, "GOMEMLIMIT": f"{memory}GiB" if memory else None},
                          "hyperfine_argv": argv})
    return cells


def bash_runner(plan, work, metadata, hf):
    """Generate the only timed execution path, also runnable directly in Bash after setup."""
    quote = shlex.quote
    helper = shlex.join([sys.executable, str(Path(__file__).resolve())])
    lines = ['#!/usr/bin/env bash', '# Generated by bench/run.py --profile laptop; do not edit.',
             'set -euo pipefail', 'dry_run=0',
             'case "${1:-}" in "") ;; --dry-run) dry_run=1 ;;',
             '  -h|--help) printf "Usage: bash run.sh [--dry-run]\\n"; exit 0 ;;',
             '  *) printf "Unknown argument: %s\\n" "$1" >&2; exit 2 ;; esac',
             '[[ $# -le 1 ]] || exit 2',
             f'plan={quote(str(plan))}', f'work={quote(str(work))}',
             'if (( dry_run )); then out="$work/results/preview"; else',
             '  mkdir -p "$work/results"',
             '  out=$(mktemp -d "$work/results/$(date -u +%Y%m%dT%H%M%SZ).XXXXXX")',
             f'  {helper} begin "$plan" "$out"', 'fi',
             'clean_env=(env -u GOGC -u GOMEMLIMIT -u GOMAXPROCS -u GODEBUG)',
             'while IFS= read -r key; do',
             '  case "$key" in TSRS_*|TS_TEST_*|RAYON_*|MIMALLOC_*|MALLOC_*|Malloc*) clean_env+=(-u "$key");; esac',
             'done < <(compgen -e)',
             'invoke() {', '  local cwd=$1; shift',
             '  printf "( cd %q && " "$cwd"',
             '  printf "%q " "${run_env[@]}" "$@"', '  printf ")\\n"',
             '  if (( ! dry_run )); then (cd -- "$cwd" && "${run_env[@]}" "$@"); fi', '}',
             'run_env=("${clean_env[@]}")',
             f'invoke "$work" {quote(str(hf))} --shell=none --runs 1 --warmup 0 --style none '
             f'--metrics {quote(METRICS)} --export-json "$out/counter-probe.json" /usr/bin/true',
             f'if (( ! dry_run )); then {helper} probe "$out"; fi']
    for index, cell in enumerate(metadata["cells"]):
        lines.append(f'printf "\\n# %s / %s\\n" {quote(cell["mode"])} {quote(cell["project"])}')
        env = [f"{k}={v}" for k, v in cell["environment"].items() if v is not None]
        lines.append('run_env=("${clean_env[@]}" ' + shlex.join(env) + ')')
        lines.append(f'if (( ! dry_run )); then {helper} preflight "$out" {index}; fi')
        argv = cell["hyperfine_argv"]
        words = ['"$out/"' + quote(a) if i and argv[i - 1] in ("--output", "--export-json") else quote(a)
                 for i, a in enumerate(argv)]
        lines.append(f'invoke {quote(cell["cwd"])} ' + ' '.join(words))
    lines += [f'if (( ! dry_run )); then {helper} finish "$out"; fi', '']
    return '\n'.join(lines)


def begin(plan, destination):
    meta = json.loads((plan / "metadata.json").read_text())
    # Prepared commands must not silently benchmark a replaced executable or different project revision.
    for compiler in meta["compilers"].values():
        if sha256(Path(compiler["binary"])) != compiler["sha256"]:
            fail(f"Compiler changed since setup: {compiler['binary']}")
    if sha256(Path(meta["cells"][0]["hyperfine_argv"][0])) != meta["hyperfine"]["sha256"]:
        fail("Hyperfine changed since setup")
    for project in meta["projects"]:
        cell = next(c for c in meta["cells"] if c["project"] == project["name"])
        if output(["git", "rev-parse", "HEAD"], cwd=cell["cwd"]) != project["commit"]:
            fail(f"Project revision changed since setup: {project['name']}")
    for path in plan.iterdir():
        if path.name != "metadata.json":
            shutil.copyfile(path, destination / path.name)
    meta["started_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
    meta["cleared_environment_variables"] = clean_environment(meta["machine"]["cores"])[1]
    for cell in meta["cells"]:
        argv = cell["hyperfine_argv"]
        for flag in ("--output", "--export-json"):
            index = argv.index(flag) + 1
            argv[index] = str(destination / argv[index])
        (destination / cell["mode"] / cell["project"]).mkdir(parents=True)
    write_json(destination / "metadata.json", meta)


def preflight_cell(source, index):
    meta = json.loads((source / "metadata.json").read_text())
    cell = meta["cells"][index]
    env, _ = clean_environment(meta["machine"]["cores"])
    env.update({k: v for k, v in cell["environment"].items() if v is not None})
    directory = source / cell["mode"] / cell["project"]
    checks = {c: preflight(cmd, cell["cwd"], env, directory / f"{c}-preflight.log")
              for c, cmd in cell["commands"].items()}
    comparisons = {c: {"same_files": check["files"] == checks["tsrs"]["files"],
                       "same_diagnostic_keys": check["error_keys"] == checks["tsrs"]["error_keys"]}
                   for c, check in checks.items() if c != "tsrs"}
    for compiler, compared in comparisons.items():
        if not all(compared.values()):
            log(f"{cell['project']}: {compiler} differs from tsrs; see preflight.json")
    write_json(directory / "preflight.json", {"compilers": checks, "comparison": comparisons["typescript"],
                                              "comparisons_to_tsrs": comparisons})


def finish(source):
    meta = json.loads((source / "metadata.json").read_text())
    for cell in meta["cells"]:
        check_log(source / cell["mode"] / cell["project"] / "compiler.log")
    rows, _ = collect_samples(source, meta)
    write_samples(source, rows)
    meta["completed_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
    write_json(source / "metadata.json", meta)
    log(f"Saved {len(rows)} raw measurements: {source}")
    log(f"Render: python3 bench/chart.py {shlex.quote(str(source))} --format blog")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runs", "--reps", type=int, help="measured repetitions (default 10; replay uses saved count)")
    parser.add_argument("--warmup", type=int, help="warmups (default 3; replay uses saved count)")
    parser.add_argument("--projects", help="comma-separated subset of bench/projects.json (default: four blog projects)")
    parser.add_argument("--work-dir", type=Path, default=BENCH / ".work" / "laptop")
    parser.add_argument("--hyperfine", type=Path, help="Hyperfine 2 executable; otherwise download verified 2.0.0")
    parser.add_argument("--setup-only", action="store_true", help="prepare packages, projects and a printable Bash runner")
    parser.add_argument("--replay", type=Path, help="use a saved bundle's package locks, project pins and resource settings")
    args = parser.parse_args(argv)
    archived = load_bundle(args.replay)[0] if args.replay else None
    runs = args.runs if args.runs is not None else archived["runs"] if archived else 10
    warmup = args.warmup if args.warmup is not None else archived["warmup"] if archived else 3
    if runs < 1 or warmup < 0:
        parser.error("--runs must be positive and --warmup nonnegative")
    cfg = json.loads((BENCH / "projects.json").read_text())
    if archived and "suite" in archived:
        cfg["suite"] = archived["suite"]
    catalog = archived["projects"] if archived else cfg["projects"]
    names = args.projects.split(",") if args.projects else [p["name"] for p in catalog] if archived else list(PROJECTS)
    if len(names) != len(set(names)) or set(names) - {p["name"] for p in catalog}:
        parser.error("Unknown or duplicate --projects")
    if sys.platform != "darwin" or platform.machine() != "arm64":
        fail("The laptop profile requires an Apple Silicon Mac (darwin-arm64)")
    for tool in ("npm", "node", "pnpm", "git"):
        if not shutil.which(tool):
            fail(f"Required command not found: {tool}")
    cores = int(output(["/usr/sbin/sysctl", "-n", "hw.logicalcpu"]))
    ram = int(output(["/usr/sbin/sysctl", "-n", "hw.memsize"]))
    memory_gib = max(1, ram // (1024 ** 3) - 4)
    work = args.work_dir.resolve()
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    plan = work / "plans" / stamp
    plan.mkdir(parents=True)
    hf = hyperfine(work, args.hyperfine)
    executables, packages = install_compilers(work, plan, args.replay)
    if archived:
        for name, package in packages.items():
            if package["sha256"] != archived["compilers"][name]["sha256"]:
                fail(f"Replayed {name} binary differs from archived SHA256")
    projects = [dict(next(p for p in catalog if p["name"] == name)) for name in names]
    for project in projects:
        if "suite_dir" in project:
            project.setdefault("commit", cfg["suite"]["commit"])
    for project in projects:
        setup.setup_project(cfg, project, work)
    modes = archived["modes"] if archived else {
        "aligned": {"label": "TypeScript Default Config", "checkers": 4, "memory_gib": None},
        "maximum": {"label": "Maximum resources", "checkers": cores, "memory_gib": memory_gib},
    }
    workers = int(archived["cells"][0]["environment"]["GOMAXPROCS"]) if archived else cores
    meta = {"kind": "hyperfine", "prepared_utc": stamp,
            "machine": {"cpu": output(["/usr/sbin/sysctl", "-n", "machdep.cpu.brand_string"]),
                        "cores": cores, "ram_bytes": ram, "macos": platform.mac_ver()[0]},
            "compilers": packages, "hyperfine": {"version": output([hf, "--version"]), "sha256": sha256(hf)},
            "runs": runs, "warmup": warmup, "projects": projects, "modes": modes, "suite": cfg["suite"],
            "cells": plan_cells(cfg, projects, modes, executables, work, hf, runs, warmup, workers)}
    if archived:
        meta["replayed_metadata_sha256"] = sha256(args.replay / "metadata.json")
    write_json(plan / "metadata.json", meta)
    # A new measurement always labels its actual binaries. Historical presentation overrides belong to the archive.
    write_json(plan / "chart.json", {"display_versions": {}})
    script = bash_runner(plan, work, meta, hf)
    (plan / "run.sh").write_text(script)
    runner = work / "run.sh"
    runner.write_text(script)
    runner.chmod(0o755)
    log(f"Bash runner: {runner}\nPreview: bash {shlex.quote(str(runner))} --dry-run")
    if not args.setup_only:
        subprocess.run(["bash", str(runner)], check=True)


if __name__ == "__main__":
    # Internal phases used by the generated Bash runner. Only preflight executes a compiler in Python;
    # every timed invocation is the exact printed Bash/Hyperfine command.
    try:
        if len(sys.argv) > 2 and sys.argv[1] == "begin":
            begin(Path(sys.argv[2]), Path(sys.argv[3]))
        elif len(sys.argv) > 2 and sys.argv[1] == "preflight":
            preflight_cell(Path(sys.argv[2]), int(sys.argv[3]))
        elif len(sys.argv) > 2 and sys.argv[1] == "probe":
            measurements(Path(sys.argv[2]) / "counter-probe.json", 1)
        elif len(sys.argv) > 2 and sys.argv[1] == "finish":
            finish(Path(sys.argv[2]))
        else:
            main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        fail(f"Benchmark failed: {error}")
