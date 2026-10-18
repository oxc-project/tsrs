#!/usr/bin/env python3
"""Run the pinned, unchanged Go fourslash tests against a Rust LSP subprocess."""

import argparse
import json
import os
from pathlib import Path
import selectors
import subprocess
import sys
import tarfile
import tempfile
import time
import tomllib


ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent


def replace_once(source, old, new):
    if source.count(old) != 1:
        raise RuntimeError(f"upstream harness changed: expected one occurrence of {old!r}")
    return source.replace(old, new, 1)


def prepare(reference, work):
    commit = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["metadata"]["typescript"]["commit"]
    checkout = work / "reference"
    stamp = checkout / ".tsrs-commit"
    if not stamp.exists() or stamp.read_text().strip() != commit:
        if checkout.exists():
            raise RuntimeError(f"{checkout} exists without the matching commit stamp; choose a new --work directory")
        with tempfile.TemporaryDirectory(prefix="extract-", dir=work) as temporary:
            archive = subprocess.Popen(
                ["git", "-C", str(reference), "archive", commit, "tsc"], stdout=subprocess.PIPE
            )
            try:
                with tarfile.open(fileobj=archive.stdout, mode="r|") as contents:
                    contents.extractall(temporary, filter="data")
                if archive.wait() != 0:
                    raise RuntimeError("could not extract the pinned TypeScript source")
            finally:
                archive.stdout.close()
                archive.wait()
            Path(temporary, ".tsrs-commit").write_text(commit + "\n")
            Path(temporary).rename(checkout)

    module = checkout / "tsc"
    overlay_dir = work / "overlay"
    overlay_dir.mkdir(exist_ok=True)
    replacements = {}

    def overlay(path, source):
        output = overlay_dir / Path(path).name
        output.write_text(source)
        replacements[str(module / path)] = str(output)

    client_path = "internal/testutil/lsptestutil/lspclient.go"
    client = (module / client_path).read_text()
    client = replace_once(client, '"io"', '"io"\n"os"')
    client = replace_once(client, "type LSPClient struct {", "type LSPClient struct {\nsubprocess *subprocessClient")
    client = replace_once(client, "\ttransport := newClientTransport()", """\tif os.Getenv("TSRS_FOURSLASH_SERVER") != "" {
        return newSubprocessClient(t, serverOpts, onServerRequest)
    }
    transport := newClientTransport()""")
    client = replace_once(client, "\tctx := t.Context()", "\tctx := c.ctx")
    client = replace_once(client, "\tc.Server.SetCompilerOptionsForInferredProjects(c.ctx, options)", """\tif c.subprocess != nil {
        c.subprocess.options <- options
        return
    }
    c.Server.SetCompilerOptionsForInferredProjects(c.ctx, options)""")
    overlay(client_path, client)
    overlay("internal/testutil/lsptestutil/tsrs_subprocess.go", (HERE / "subprocess.go").read_text())
    overlay("internal/testutil/lsptestutil/tsrs_subprocess_test.go", (HERE / "subprocess_test.go").read_text())
    overlay("internal/testutil/lsptestutil/tsrs_worker.go", (HERE / "worker.go").read_text())
    main_path = "internal/fourslash/tests/testmain_test.go"
    main = (module / main_path).read_text()
    main = replace_once(main, '"testing"', '"testing"\n"github.com/microsoft/TypeScript/tsc/internal/testutil/lsptestutil"')
    main = replace_once(main, "\tm.Run()", "\tdefer lsptestutil.CloseSubprocessWorkers()\n\tm.Run()")
    overlay(main_path, main)

    harness_path = "internal/fourslash/fourslash.go"
    harness = (module / harness_path).read_text()
    harness = replace_once(harness, "\ttestData := ParseTestData(t, content, fileName)", """\ttestData := ParseTestData(t, content, fileName)
    if options.ContentMapperSpawner != nil {
        t.Skip("subprocess prototype: Go content-mapper callbacks cannot cross the process boundary")
    }""")
    harness = replace_once(harness, "\t<-f.client.Server.InitComplete()", """\tif f.client.Server != nil {
        <-f.client.Server.InitComplete()
    } else {
        f.client.WaitForSubprocessInit(t)
    }""")
    overlay(harness_path, harness)

    # Keep Go's state comparison and formatting; substitute read-only remote snapshot views.
    state_path = "internal/fourslash/statebaseline.go"
    state = (module / state_path).read_text()
    state = replace_once(state, '\t"github.com/microsoft/TypeScript/tsc/internal/compiler"\n', "")
    state = state.replace("*compiler.Program", "*remoteProgram")
    state = state.replace("*project.ConfigFileRegistry", "*remoteConfigFileRegistry")
    state = state.replace("*project.Snapshot", "*remoteSnapshot")
    state = replace_once(state, "\tsession := f.client.Server.Session()\n\tsnapshot := session.Snapshot()",
                         "\tsnapshot := f.remoteSnapshot(t)")
    overlay(state_path, state)
    overlay("internal/fourslash/tsrs_remotestate.go", (HERE / "remotestate.go").read_text())
    go_env = dict(os.environ, GOTOOLCHAIN="auto", GOWORK="off")
    goroot = subprocess.check_output(["go", "env", "GOROOT"], cwd=module, env=go_env, text=True).strip()
    subprocess.run([str(Path(goroot) / "bin/gofmt"), "-w", *replacements.values()], check=True)
    manifest = work / "overlay.json"
    manifest.write_text(json.dumps({"Replace": replacements}, indent=2) + "\n")
    return module, manifest


def run_with_progress(command, *, cwd, env):
    """Render Go JSON events as progress, keeping diagnostics from unsuccessful cases."""
    started = time.monotonic()
    running = False
    counts = dict.fromkeys(("pass", "fail", "skip"), 0)
    output = {}
    interactive = sys.stderr.isatty()

    def write(text):
        if interactive:
            print("\r\033[K", end="", file=sys.stderr, flush=True)
        print(text, end="", flush=True)

    def report(*, final=False):
        elapsed = time.monotonic() - started
        message = "Building Go suite"
        if running:
            completed = sum(counts.values())
            message = (f"Fourslash: {completed} {'group' if completed == 1 else 'groups'} completed "
                       f"({counts['pass']} passed, {counts['fail']} failed, {counts['skip']} skipped)")
        prefix = "\r\033[K" if interactive else ""
        print(f"{prefix}{message} [{elapsed:.1f}s]",
              end="\n" if final or not interactive else "", file=sys.stderr, flush=True)

    def consume(line):
        nonlocal running
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            write(line.decode(errors="replace") + "\n")
            return
        action = event.get("Action")
        name = event.get("Test", "")
        if action == "start":
            running = True
        if "Output" in event:
            if name:
                output.setdefault(name.split("/", 1)[0], []).append(event["Output"])
            else:
                write(event["Output"])
        if name and "/" not in name and action in counts:
            counts[action] += 1
            lines = output.pop(name, [])
            if action == "fail":
                write("".join(lines))

    report()
    last_report = started
    pending = b""
    with subprocess.Popen(command, cwd=cwd, env=env, stdout=subprocess.PIPE) as process, selectors.DefaultSelector() as selector:
        selector.register(process.stdout, selectors.EVENT_READ)
        while True:
            if selector.select(timeout=1):
                chunk = os.read(process.stdout.fileno(), 65536)
                if not chunk:
                    if pending:
                        consume(pending)
                    break
                pending += chunk
                *lines, pending = pending.split(b"\n")
                for line in lines:
                    consume(line)
            now = time.monotonic()
            if now - last_report >= (1 if interactive else 5):
                report()
                last_report = now
        returncode = process.wait()
    if returncode:
        # A crash may leave cases without a final event; preserve their output too.
        for lines in output.values():
            write("".join(lines))
    report(final=True)
    return returncode


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, default=ROOT / "ts-ref", help="Git checkout containing the pinned TypeScript commit (read only)")
    parser.add_argument("--work", type=Path, default=ROOT / "target/go-fourslash", help="isolated reference snapshot, Go cache, and overlay directory")
    parser.add_argument("--server", type=Path, help="prebuilt fourslash_server binary; otherwise build it")
    parser.add_argument("go_args", nargs=argparse.REMAINDER, help="go test flags after -- (default: full suite)")
    args = parser.parse_args()
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    print("Preparing pinned TypeScript source and Go harness...", file=sys.stderr, flush=True)
    module, manifest = prepare(args.reference.resolve(), work)
    env = dict(os.environ, GOTOOLCHAIN="auto", GOWORK="off", GOCACHE=str(work / "go-cache"))
    if args.server:
        server = args.server.resolve()
    else:
        print("Building Rust Fourslash server...", file=sys.stderr, flush=True)
        env["CARGO_TARGET_DIR"] = str(work / "cargo")
        subprocess.run(["cargo", "build", "--profile", "dev", "-p", "tsrs_fourslash_server"], cwd=ROOT, env=env, check=True)
        server = work / "cargo/debug/fourslash_server"
    if not server.is_file():
        parser.error(f"server does not exist: {server}")
    env["TSRS_FOURSLASH_SERVER"] = str(server)
    print("Checking subprocess bridge shutdown...", file=sys.stderr, flush=True)
    bridge = subprocess.run(
        ["go", "test", f"-overlay={manifest}", "./internal/testutil/lsptestutil", "-count=1"],
        cwd=module, env=env, stdout=sys.stderr,
    )
    if bridge.returncode:
        raise SystemExit(bridge.returncode)
    flags = args.go_args
    if flags[:1] == ["--"]:
        flags = flags[1:]
    command = ["go", "test", f"-overlay={manifest}", "./internal/fourslash/tests", "-count=1", "-parallel=4", "-timeout=30m"]
    native_output = any(flag.split("=", 1)[0].lstrip("-") in ("json", "v", "test.v", "list", "c") for flag in flags)
    if native_output:
        print("Building and running Go Fourslash suite...", file=sys.stderr, flush=True)
        returncode = subprocess.run([*command, *flags], cwd=module, env=env).returncode
    else:
        returncode = run_with_progress([*command, "-json", *flags], cwd=module, env=env)
    raise SystemExit(returncode)


if __name__ == "__main__":
    main()
