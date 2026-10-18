#!/usr/bin/env python3
"""Regenerate benchmark reports and propose them through the repository's GitHub App."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def command(*args: str, input: str | None = None) -> str:
    return subprocess.check_output(args, cwd=ROOT, input=input, text=True).strip()


def api(endpoint: str, payload: dict) -> dict:
    return json.loads(command("gh", "api", "--method", "POST", f"repos/{os.environ['GH_REPO']}/{endpoint}",
                              "--input", "-", input=json.dumps(payload)))


def main(result_path: Path) -> None:
    result_path = result_path.resolve()
    result = json.loads(result_path.read_text())
    measured = command("git", "rev-parse", "--verify", f"{result['tsrs']['commit']}^{{commit}}")
    command("git", "fetch", "-q", "origin", "main")
    on_main = subprocess.run(["git", "merge-base", "--is-ancestor", measured, "origin/main"], cwd=ROOT).returncode == 0
    if result["tsrs"]["dirty"] or not on_main:
        print("Keeping results as an artifact: the measured commit is not a clean commit on main.")
        return

    command("git", "checkout", "-q", "-f", "-B", "main", "origin/main")
    destination = ROOT / "bench/results"
    destination.mkdir(parents=True, exist_ok=True)
    for path in (result_path, result_path.with_suffix(".md")):
        shutil.copyfile(path, destination / path.name)
    subprocess.run([sys.executable, str(ROOT / "bench/publish.py"), str(result_path)], cwd=ROOT, check=True)
    command("git", "add", "bench/RESULTS.md", "bench/large-projects*.svg", "bench/per-project-*.svg")
    command("git", "add", "-f", *(str(destination / name) for name in (result_path.name, result_path.with_suffix(".md").name)))
    if subprocess.run(["git", "diff", "--cached", "--quiet"], cwd=ROOT).returncode == 0:
        return

    command("gh", "auth", "setup-git")
    branch = f"bench/results-{measured}"
    title = f"chore(bench): update results for {measured[:12]}"
    previous = command("git", "ls-remote", "--heads", "origin", f"refs/heads/{branch}").split("\t")[0]
    paths = command("git", "diff", "--cached", "--name-only", "-z").split("\0")
    # Read staged contents without trimming whitespace; GitHub supplies commit attribution from the app token.
    entries = [{"path": path, "mode": "100644", "type": "blob",
                "content": subprocess.check_output(["git", "show", f":{path}"], cwd=ROOT, text=True)}
               for path in paths if path]
    tree = api("git/trees", {"base_tree": command("git", "rev-parse", "HEAD^{tree}"), "tree": entries})
    commit = api("git/commits", {"message": title, "tree": tree["sha"],
                                 "parents": [command("git", "rev-parse", "HEAD")]})["sha"]
    command("git", "fetch", "-q", "origin", commit)
    command("git", "push", f"--force-with-lease=refs/heads/{branch}:{previous}", "origin", f"{commit}:refs/heads/{branch}")

    prs = json.loads(command("gh", "pr", "list", "--base", "main", "--head", branch, "--state", "open", "--json", "number"))
    body = (f"Regenerate benchmark reports and README charts for `{measured[:12]}` "
            f"against TypeScript `{result['tsgo']['version']}`.\n\n[Benchmark run]({os.environ['RUN_URL']}).\n")
    with tempfile.NamedTemporaryFile(mode="w", suffix=".md") as description:
        description.write(body)
        description.flush()
        args = ["edit", str(prs[0]["number"])] if prs else ["create", "--draft", "--base", "main", "--head", branch]
        print(command("gh", "pr", *args, "--title", title, "--body-file", description.name))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("result", type=Path)
    main(parser.parse_args().result)
