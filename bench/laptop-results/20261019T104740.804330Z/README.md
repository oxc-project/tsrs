# Laptop benchmark: 2026-10-19

Recorded with the laptop profile (now [`bench/run.py --profile laptop`](../../run.py)) on an Apple M3 Max with 14 logical CPUs and 36 GiB RAM.
The run contains 240 individual measurements: four public projects, three compilers, two resource modes,
and ten measured repetitions after three warmups for each combination.

| Compiler | Measured version |
| --- | --- |
| tsrs | `0.17.0-ts7.1.0-dev.20260929` |
| TypeScript nightly | `7.1.0-dev.20261019.1` |
| TypeScript | `7.0.2` |

The chart legends display `0.18.0-ts7.1.0-dev.20260929` for tsrs as a presentation override. The measurements
remain from `0.17.0-ts7.1.0-dev.20260929`; no benchmark was rerun for that label change. The chart manifest
records both versions.

## Data and figures

- [`raw.csv`](raw.csv) and [`raw.json`](raw.json): all individual wall-time, instruction-count, peak-RSS,
  and exit-code measurements, without aggregation.
- [`metadata.json`](metadata.json): hardware, exact compiler versions and binary hashes, pinned public project
  commits, measured commands, environments, and timestamps. Original local paths are retained as provenance;
  they are not required to render the charts.
- `aligned/<project>/hyperfine.json` and `maximum/<project>/hyperfine.json`: original Hyperfine exports,
  including every measurement and the additional CPU-time and cycle counters.
- `aligned/<project>/preflight.json` and `maximum/<project>/preflight.json`: untimed file counts and diagnostic
  comparisons. Compiler outputs are not identical for every project; the recorded differences are preserved.
- [`counter-probe.json`](counter-probe.json), [`npm-packages.json`](npm-packages.json), and the three
  `*-package-lock.json` files: counter availability and package provenance.
- [`charts/`](charts/): PNG/SVG figures, chart means, parameter facts, accessible descriptions, and the manifest.
  Means average the ten repetitions within each project/compiler/mode; projects are never combined.
- [`SHA256SUMS`](SHA256SUMS): checksums for all archived files except the checksum file itself.

`aligned` is displayed as **TypeScript Default Config**: four checkers with default memory policies.
**Maximum resources** uses fourteen checkers, tsrs `--maxMemory 32G`, and Go `GOMEMLIMIT=32GiB`.
Both modes set `GOMAXPROCS=14` and `RAYON_NUM_THREADS=14`, with default Go GC.

The archive preserves the selected measurement files byte for byte. Figures can be regenerated from them. Repeated console logs, installed packages,
working project clones, caches, and Finder metadata are not included.

## Regenerate the figures

From the repository root, after installing the [plotting dependencies](../../README.md#reproduce-the-blog-charts):

```bash
bench/.work/charts-venv/bin/python bench/chart.py \
  bench/laptop-results/20261019T104740.804330Z --format blog

bench/.work/charts-venv/bin/python bench/chart.py \
  bench/laptop-results/20261019T104740.804330Z --format blog --check
```

The renderer reads the presentation override from [`chart.json`](chart.json), validates all 240 raw samples
against Hyperfine, and reproduces both PNG/SVG figures and their supporting files. SVG dates/IDs are fixed;
`--check` compares bytes in a temporary directory. No benchmark is run and measured versions remain unchanged.
To write elsewhere, add `--output-dir <directory>`.

To repeat measurements using these exact versions, project pins and resource settings, use
`python3 bench/run.py --profile laptop --replay bench/laptop-results/20261019T104740.804330Z --setup-only`,
then inspect and run `bash bench/.work/laptop/run.sh`. New timings are recorded in a fresh result directory.

To verify the archive:

```sh
cd bench/laptop-results/20261019T104740.804330Z
shasum -a 256 -c SHA256SUMS
```
