# Benchmarks

Run commands from the repository root in Bash. [`run.py`](run.py) compares one tsrs release with one or more
native TypeScript 7 versions on repositories selected from [`projects.json`](projects.json). Each invocation
uses one resource configuration for every compiler.

## Run a benchmark

Requirements: Python 3.9+, Node/npm, pnpm, git, and Hyperfine 2 with hardware-counter support. Use the repository's
Node and pnpm versions ([`.node-version`](../.node-version), [`package.json`](../package.json)). Native compiler
packages are supported on macOS/Linux, ARM64/x64. On macOS ARM64, the runner downloads checksum-verified
Hyperfine 2.0.0 if it is not on PATH; elsewhere, install it or pass `--hyperfine /path/to/hyperfine`.

```bash
python3 bench/run.py \
  --typescript-versions next 7.0.2 \
  --tsrs-version latest \
  --repos vscode mui-docs webpack t3code-server
```

`next` selects the TypeScript nightly; `latest` selects the latest tsrs release. Exact npm versions also work.
Tags are resolved once per invocation, and the resolved versions and binary hashes are saved with the results.
Only native TypeScript 7 packages are supported.

| Parameter | Default | Purpose |
| --- | --- | --- |
| `--typescript-versions` | `next` | Versions/tags separated by spaces or commas |
| `--tsrs-version` | `latest` | Published tsrs version/tag |
| `--repos` / `--projects` | `vscode mui-docs webpack t3code-server` | Catalog names, or `all` |
| `--checkers` | `4` | Checker count per compiler |
| `--workers` | Available CPUs | `GOMAXPROCS` and `RAYON_NUM_THREADS` |
| `--memory-gib` | Unset | Soft budget through tsrs `--maxMemory` and Go `GOMEMLIMIT` |
| `--runs` / `--reps` | `10` | Measured repetitions per compiler/project |
| `--warmup` | `3` | Warmups per compiler/project |
| `--work-dir` | `bench/.work` | Cached checkouts, dependencies and compilers |
| `--out-dir` | `bench/results` | Parent directory for timestamped result bundles |
| `--hyperfine` | PATH or macOS ARM64 download | Hyperfine executable |

Pass resource settings explicitly to change the configuration:

```bash
python3 bench/run.py --typescript-versions next 7.0.2 --tsrs-version latest \
  --repos webpack --checkers 14 --workers 14 --memory-gib 32
```

To measure a local release build, replace `--tsrs-version` with `--tsrs /path/to/tsrs`.

```bash
# List available repositories without installing anything.
python3 bench/run.py --repos all --print-projects

# Prepare repositories and compilers, then print commands without measuring.
python3 bench/run.py --repos webpack --typescript-versions next 7.0.2 --dry-run

# Prepare repository checkouts and dependencies only.
python3 bench/run.py --repos webpack --setup-only
```

The runner prints each final Hyperfine command in copyable Bash syntax before execution, including its working
directory and environment. A counter probe fails early if instruction counts are unavailable.

## Measurements and saved data

Hyperfine records wall time, CPU instructions, peak resident memory (RSS), and exit status for every run.
Peak RSS is not total allocated memory. Memory budgets are soft limits, not hard RSS caps; Go GC otherwise
uses its defaults. Every timed compiler command includes:

```text
-p <project> --noEmit --incremental false --pretty false --checkers <N>
```

A separate untimed preflight records file counts and diagnostics. Compiler versions can report different errors;
the bundle retains those differences alongside the measurements.

Results are saved in ignored `bench/results/<timestamp>/` directories:

| File | Contents |
| --- | --- |
| `metadata.json` | Versions, binary hashes, project commits, machine, settings, commands and environment |
| `raw.csv`, `raw.json` | Individual samples, without aggregation |
| `*-package-lock.json`, `npm-packages.json` | Installed compiler packages |
| `<project>/hyperfine.json` | Original Hyperfine measurements |
| `<project>/preflight.json` and logs | File counts, diagnostics and compiler output |

Keep raw data, compiler installations and checkouts out of git. CI commits only the public report and SVG images
through a PR. To repeat measurements, use the exact versions and resource settings recorded in `metadata.json`.

## Draw charts from a saved bundle

Both chart scripts produce SVG images. Rendering needs only saved data; it does not rerun compilers or require
the benchmarked repositories.
Set up the plotting environment once:

```bash
python3.12 -m venv bench/.work/charts-venv
bench/.work/charts-venv/bin/python -m pip install -r bench/requirements-charts.txt
```

Set `result` to a completed bundle directory, then choose a renderer:

```bash
result=bench/results/REPLACE_WITH_TIMESTAMP
chart_python=bench/.work/charts-venv/bin/python

# Scatter chart comparing tsrs with one measured TypeScript version.
"$chart_python" bench/chart.py "$result" --typescript-version 7.0.2 \
  --output "$result/scatter.svg"

# Per-project chart; use peak_rss_bytes for memory or --theme dark for a dark SVG.
"$chart_python" bench/chart.py "$result" --per-project wall_s \
  --output "$result/wall-time.svg"

# Bar chart showing every measured version.
"$chart_python" bench/chart_bars.py "$result" --baseline 7.0.2
```

[`chart.py`](chart.py) defaults to the first measured TypeScript version. [`chart_bars.py`](chart_bars.py) defaults
to the last version as its comparison baseline and accepts `--title` and `--output-dir`. Bar charts use Cloudflare
orange for tsrs, TypeScript blues, and Matplotlib's default font.

Bar-chart outputs are in `<result>/charts/`: `benchmark.svg`, `means.csv`, `means.json`,
`alt-text.txt`, `chart-manifest.json`, and `index.html` with commands and parameter details. Both renderers average
runs within each compiler/project; they never average across projects. Legends show the measured versions.

To generate the report and all publication SVGs together:

```bash
"$chart_python" bench/publish.py "$result" --output-dir "$result/publication"
```

This writes `RESULTS.md`, `large-projects.svg`, `large-projects-dark.svg`, the light/dark `per-project-time` and
`per-project-memory` SVGs, and `benchmark.svg`.

## Reproduce images from CI

Use a checkout of the repository commit used by the workflow run and the Python 3.12 plotting environment above.
With an authenticated GitHub CLI, replace `YOUR_RUN_ID` with the GitHub Actions run ID:

```bash
run_id=YOUR_RUN_ID
work="bench/.work/reproduce-$run_id"
chart_python=bench/.work/charts-venv/bin/python

# Download the combined measurements and original publication files.
gh run download "$run_id" --repo oxc-project/tsrs --name bench-result --dir "$work"

# Regenerate the report and images from those measurements.
"$chart_python" bench/publish.py "$work/bench-result" \
  --output-dir "$work/images" \
  --commit "$(gh run view "$run_id" --repo oxc-project/tsrs --json headSha --jq '.headSha')" \
  --run-url "https://github.com/oxc-project/tsrs/actions/runs/$run_id"
```

The regenerated report and SVGs are in `$work/images/`.
The downloaded originals are in `$work/bench-publication/`. Keep the same code and pinned plotting dependencies
when reproducing images. Artifacts are retained for 14 days; download the complete bundle to preserve it longer.

## CI publication

[Update benchmarks](../.github/workflows/update-benchmarks.yml) runs manually or when its workflow or project
catalog changes on `main`. It builds tsrs with PGO + BOLT and builds Hyperfine once, resolves the TypeScript nightly
once, then measures every catalog project against that nightly and TypeScript 7.0.2.

Each project uses 4 checkers, 8 worker CPUs, default memory policy, 3 warmups and 10 measured runs. The final job
combines the project bundles without aggregating raw samples, renders both chart styles, and uploads the
`bench-result` artifact. Runs from `main` open or update a draft PR containing `bench/RESULTS.md` and the SVG images;
raw samples, logs and package lockfiles remain in artifacts.

[`publish.py`](publish.py) also accepts several bundle directories with `--merge-dir <new-directory>` to reproduce
this combination locally. It requires matching compiler builds, resource settings and hardware across bundles.

The active `ci.yml` compares ordered diagnostics and exit codes against the base build on pull requests and merge
groups, using the determinism job's four projects and regression cases. `wallclock.yml` compares main and PR release
binaries on pull requests that may affect performance. When the binaries differ, separate 8-vCPU jobs measure
alternating timing pairs, peak RSS and instruction counts for each `bench/projects.json` project. The report job
joins available project results in catalog order and posts one PR table; missing or invalid results appear as failed
rows. The comparison and probe workflows remain disabled under `.github/workflows/*.bak`.

## Project setup

[`projects.json`](projects.json) is the source of truth for repository commits, project paths and dependency
installation commands. It includes TypeScript's benchmark-suite cases and application/library repositories.
The runner prepares these checkouts under `bench/.work/`, including generated declarations needed for type checking.

[`overlays/`](overlays/) contains TypeScript 7 configuration adjustments for projects that still use older
TypeScript options. Overlays are applied before installation; their hashes are recorded in metadata and cache keys.
File counts and diagnostics come from each run's preflight, rather than fixed counts in this README.

## Never time a run with `--extendedDiagnostics`

Do not pass `--extendedDiagnostics` or `--diagnostics` to a timed compiler invocation. TypeScript performs extra
GC and statistics work before exiting, which changes wall time and peak memory differently from tsrs. Collect
statistics in a separate untimed run, as the benchmark runner does. This applies to manual Hyperfine commands
and instruction-count measurements too; see [the memory investigation](../notes/mem-frontend.md).

## Other tools

These specialized tools have separate output formats; they are not part of the release-comparison publication flow.

| Tool | Purpose |
| --- | --- |
| [`count.py`](count.py) | Linux instruction counts and peak RSS for one command |
| [`watch.py`](watch.py) | Watch startup and rebuild measurements on vscode |
| [`compare.py`](compare.py) | tsrs, TypeScript and Bun comparisons across thread counts |
| [`history.py`](history.py), [`regressions.py`](regressions.py) | Reports from historical result JSON |

### Watch mode

`watch.py` measures startup, comment edits and an inserted type error, then restores the edited file. It saves
individual sessions and their medians. The current publication workflow does not run this benchmark.

```bash
python3 bench/watch.py --local --tsrs target/release/tsrs
```

### Head-to-head on a wide machine

`compare.py` uses the TypeScript versions pinned under `tsgo` and `reference` in `projects.json`, plus the supplied
tsrs and Bun binaries. `--threads` selects its thread-count comparisons; it saves JSON and Markdown under
`bench/results/compare/`.

```bash
python3 bench/compare.py --tsrs target/release/tsrs --bun /path/to/bun \
  --projects vscode --threads default,4,8 --reps 20
```
