#!/usr/bin/env python3
"""Run the pinned, unchanged Go fourslash tests against a Rust LSP subprocess."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
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

    harness_path = "internal/fourslash/fourslash.go"
    harness = (module / harness_path).read_text()
    harness = replace_once(harness, "\ttestData := ParseTestData(t, content, fileName)", """\ttestData := ParseTestData(t, content, fileName)
    if testData.isStateBaseliningEnabled() {
        t.Skip("subprocess prototype: internal project-state baselines need a remote inspection hook")
    }
    if options.ContentMapperSpawner != nil {
        t.Skip("subprocess prototype: Go content-mapper callbacks cannot cross the process boundary")
    }""")
    harness = replace_once(harness, "\t<-f.client.Server.InitComplete()", """\tif f.client.Server != nil {
        <-f.client.Server.InitComplete()
    } else {
        f.client.WaitForSubprocessInit(t)
    }""")
    overlay(harness_path, harness)
    go_env = dict(os.environ, GOTOOLCHAIN="auto", GOWORK="off")
    goroot = subprocess.check_output(["go", "env", "GOROOT"], cwd=module, env=go_env, text=True).strip()
    subprocess.run([str(Path(goroot) / "bin/gofmt"), "-w", *replacements.values()], check=True)
    manifest = work / "overlay.json"
    manifest.write_text(json.dumps({"Replace": replacements}, indent=2) + "\n")
    return module, manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, default=ROOT / "ts-ref", help="Git checkout containing the pinned TypeScript commit (read only)")
    parser.add_argument("--work", type=Path, default=ROOT / "target/go-fourslash", help="isolated reference snapshot, Go cache, and overlay directory")
    parser.add_argument("--server", type=Path, help="prebuilt fourslash_server example; otherwise build it")
    parser.add_argument("go_args", nargs=argparse.REMAINDER, help="go test flags after -- (default: full suite)")
    args = parser.parse_args()
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    module, manifest = prepare(args.reference.resolve(), work)
    env = dict(os.environ, GOTOOLCHAIN="auto", GOWORK="off", GOCACHE=str(work / "go-cache"))
    if args.server:
        server = args.server.resolve()
    else:
        env["CARGO_TARGET_DIR"] = str(work / "cargo")
        subprocess.run(["cargo", "build", "--profile", "dev", "-p", "tsrs_cli", "--example", "fourslash_server"], cwd=ROOT, env=env, check=True)
        server = work / "cargo/debug/examples/fourslash_server"
    if not server.is_file():
        parser.error(f"server does not exist: {server}")
    env["TSRS_FOURSLASH_SERVER"] = str(server)
    flags = args.go_args
    if flags[:1] == ["--"]:
        flags = flags[1:]
    result = subprocess.run(
        ["go", "test", f"-overlay={manifest}", "./internal/fourslash/tests", "-count=1", "-parallel=4", "-timeout=30m", *flags],
        cwd=module, env=env,
    )
    raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
