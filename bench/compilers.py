#!/usr/bin/env python3
"""Compiler setup and Hyperfine execution helpers for bench/run.py (no separate CLI)."""
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
from results import collect_samples, measurements, write_samples

BENCH = Path(__file__).resolve().parent

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
    elif shutil.which("hyperfine"):
        exe = Path(shutil.which("hyperfine")).resolve()
    else:
        if setup.native_platform() != "darwin-arm64":
            fail("Install Hyperfine 2 with hardware-counter support, or pass --hyperfine /path/to/hyperfine")
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


def install_compilers(work, result_dir, typescript_versions, tsrs_version, tsrs_binary=None):
    platform_id = setup.native_platform()
    if platform_id not in ("darwin-arm64", "darwin-x64", "linux-arm64", "linux-x64"):
        fail(f"Unsupported native compiler platform: {platform_id}")
    native_name = "@typescript/typescript-" + platform_id
    tsrs_platform = platform_id
    if platform_id.startswith("linux-"):
        musl = platform.libc_ver()[0] == "musl" or bool(list(Path("/lib").glob("ld-musl-*.so.1")))
        tsrs_platform += "-musl" if musl else "-gnu"
    specifications = [(f"typescript_{i}", "typescript", version, "TypeScript")
                      for i, version in enumerate(typescript_versions, 1)]
    if tsrs_binary is None:
        specifications.append(("tsrs", "@tsrs/" + tsrs_platform, tsrs_version, "tsrs"))
    executables, metadata, all_packages = {}, {}, {}
    resolved = set()
    for compiler, name, tag, label in specifications:
        requested = f"{name}@{tag}"
        package = release(name, tag)
        packages = [package]
        if name == "typescript":
            if package["version"] in resolved:
                fail(f"TypeScript {package['version']} was selected more than once (including resolved tags)")
            resolved.add(package["version"])
            native_version = package.get("optionalDependencies", {}).get(native_name)
            if not native_version:
                fail(f"{requested} has no native {platform_id} compiler; select TypeScript 7 versions")
            package = release(native_name, native_version)
            packages.append(package)
        log(f"Using {requested}: {package['version']}")
        install = work / "packages" / f"{compiler}-{package['version']}"
        install.mkdir(parents=True, exist_ok=True)
        write_json(install / "package.json", {"name": "tsrs-benchmark", "private": True})
        subprocess.run(["npm", "install", "--prefix", str(install), "--registry", REGISTRY,
                        "--cache", str(work / "npm-cache"), "--ignore-scripts", "--no-audit", "--no-fund",
                        "--save-exact", *[p["name"] + "@" + p["version"] for p in packages]], check=True)
        shutil.copyfile(install / "package-lock.json", result_dir / f"{compiler}-package-lock.json")
        exe = install / "node_modules" / package["name"] / ("lib/tsc" if name == "typescript" else "tsrs")
        installed = json.loads((install / "node_modules" / package["name"] / "package.json").read_text())
        if installed["version"] != package["version"]:
            fail(f"Installed {compiler} version does not match the resolved release")
        executables[compiler] = exe
        all_packages[compiler] = packages
        metadata[compiler] = {"package": package["name"], "version": package["version"], "label": label,
                              "requested": requested, "dist": package["dist"], "binary": str(exe),
                              "sha256": sha256(exe), "version_output": output([exe, "--version"])}
    if tsrs_binary is not None:
        exe = tsrs_binary.resolve()
        version = output([exe, "--version"])
        executables["tsrs"] = exe
        metadata["tsrs"] = {"version": version.removeprefix("tsrs "), "label": "tsrs", "binary": str(exe),
                            "requested": str(exe), "sha256": sha256(exe), "version_output": version}
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


def plan_cells(cfg, projects, settings, executables, work, hf, runs, warmup, result_dir):
    cells = []
    for project in projects:
        cwd, config = setup.project_path(cfg, project, work)
        commands = {compiler: [str(exe), "-p", str(config), *setup.TIMED_FLAGS,
                               "--checkers", str(settings["checkers"])]
                    for compiler, exe in executables.items()}
        memory = settings["memory_gib"]
        if memory is not None:
            commands["tsrs"] += ["--maxMemory", f"{memory}G"]
        directory = result_dir / project["name"]
        argv = [str(hf), "--shell=none", "--runs", str(runs), "--warmup", str(warmup),
                "--metrics", METRICS, "--style", "basic", "--output", str(directory / "compiler.log"),
                "--ignore-failure=1,2", "--export-json", str(directory / "hyperfine.json")]
        for compiler, command in commands.items():
            argv += ["--command-name", compiler, shlex.join(command)]
        cells.append({"project": project["name"], "cwd": str(cwd), "commands": commands,
                      "environment": {"GOMAXPROCS": str(settings["workers"]),
                                      "RAYON_NUM_THREADS": str(settings["workers"]), "GOGC": None,
                                      "GOMEMLIMIT": f"{memory}GiB" if memory is not None else None},
                      "hyperfine_argv": argv})
    return cells


def bash_command(argv, cwd, settings):
    """A copyable Bash command with the exact cwd and cleaned environment used for measurement."""
    _, removed = clean_environment(settings["workers"])
    env = ["env", *[part for key in removed for part in ("-u", key)],
           f"GOMAXPROCS={settings['workers']}", f"RAYON_NUM_THREADS={settings['workers']}"]
    if settings["memory_gib"] is not None:
        env.append(f"GOMEMLIMIT={settings['memory_gib']}GiB")
    return f"( cd {shlex.quote(str(cwd))} && {shlex.join([*env, *argv])} )"


def run_matrix(source, meta, hf, dry_run=False):
    settings = meta["settings"]
    if dry_run:
        for cell in meta["cells"]:
            print(bash_command(cell["hyperfine_argv"], cell["cwd"], settings), flush=True)
        return
    env, removed = clean_environment(settings["workers"])
    if settings["memory_gib"] is not None:
        env["GOMEMLIMIT"] = f"{settings['memory_gib']}GiB"
    meta["cleared_environment_variables"] = removed
    meta["started_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
    write_json(source / "metadata.json", meta)
    probe = source / "counter-probe.json"
    argv = [str(hf), "--shell=none", "--runs", "1", "--warmup", "0", "--style", "none",
            "--metrics", METRICS, "--export-json", str(probe), "/usr/bin/true"]
    print(bash_command(argv, source, settings), flush=True)
    subprocess.run(argv, env=env, cwd=source, check=True)
    measurements(probe, 1)
    for cell in meta["cells"]:
        directory = source / cell["project"]
        directory.mkdir(parents=True)
        checks = {c: preflight(cmd, cell["cwd"], env, directory / f"{c}-preflight.log")
                  for c, cmd in cell["commands"].items()}
        comparisons = {c: {"same_files": check["files"] == checks["tsrs"]["files"],
                           "same_diagnostic_keys": check["error_keys"] == checks["tsrs"]["error_keys"]}
                       for c, check in checks.items() if c != "tsrs"}
        for compiler, compared in comparisons.items():
            if not all(compared.values()):
                log(f"{cell['project']}: {compiler} differs from tsrs; see preflight.json")
        write_json(directory / "preflight.json", {"compilers": checks, "comparisons_to_tsrs": comparisons})
        print(bash_command(cell["hyperfine_argv"], cell["cwd"], settings), flush=True)
        subprocess.run(cell["hyperfine_argv"], cwd=cell["cwd"], env=env, check=True,
                       timeout=900 * len(cell["commands"]) * (meta["runs"] + meta["warmup"]))
        check_log(directory / "compiler.log")
    rows, _ = collect_samples(source, meta)
    write_samples(source, rows)
    meta["completed_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
    write_json(source / "metadata.json", meta)
    log(f"Saved {len(rows)} raw measurements: {source}")
