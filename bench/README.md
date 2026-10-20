# Benchmarks

All benchmark tools run through `node bench/bench.mjs` from the repository root. Node 24 is required. `projects.json` pins public repositories, commits, type-check paths and dependency installation commands; `overlays/` supplies TypeScript 7 configuration adjustments. The benchmark never changes these inputs during a measurement.

## Inputs and outputs

| Command | Inputs | Output |
| --- | --- | --- |
| `plan` | Project names and `projects.json` | Selected names and cache keys on stdout; no checkout or install |
| `prepare` | Project names, catalog and overlays | Cached checkouts and dependencies in `bench/.work/` |
| `run` | Prepared projects, compiler versions or binary, settings and Hyperfine 2 | One ignored `bench/results/<timestamp>/` bundle |
| `merge` | Complete per-project bundles | One validated combined bundle |
| `render` | A complete bundle or `publication.json` | `RESULTS.md`, seven SVGs, `publication.json` and render manifest |
| `verify-images` | `publication.json` and an existing publication directory | Re-renders from the JSON and compares every SVG and the report |
| `compare`, `watch`, `count` | Compiler commands and settings | Versioned bundles with individual samples and logs |
| `history`, `regressions` | Saved bundles or historical JSON | Reports on stdout; `regressions --comment` is opt-in |

`run.json` is the source of truth in each new bundle. It records the schema version, benchmark kind and completion status; machine; exact project commits and overlay hashes; compiler versions and binary hashes; command arguments, environment and resource settings; untimed preflight results; and every measured sample. Original Hyperfine JSON and logs remain alongside it. Peak RSS is in bytes and wall time is in seconds. The renderer rejects incomplete bundles and checks samples against the Hyperfine exports. It takes arithmetic means **within** each project/compiler; projects are never averaged together.

`watch` samples the process RSS every 50 ms while it runs, so its reported peak may miss a short spike. Compile, compare and count peaks come from Hyperfine's process measurement.

## Reproduce the published SVGs without benchmarking

Each new publication commits `bench/publication.json`, `bench/RESULTS.md`, and seven SVGs. The JSON contains full-precision chart values and labels, so reproduction requires no compiler, project checkout, Hyperfine, or benchmark machine. From the repository root, with Node 24:

```bash
node bench/bench.mjs render bench/publication.json --output-dir bench/.work/reproduced
node bench/bench.mjs verify-images bench/publication.json --against bench
```

The first command writes fresh SVGs and a report to `bench/.work/reproduced/`. The second independently renders from the JSON and compares all seven SVGs and `RESULTS.md` byte for byte with the committed files in `bench/`. A successful check prints `Reproduced 7 SVGs and RESULTS.md`.

The JSON records hashes of the renderer source and lockfile. SVG text uses the viewer's default system fonts, so rendering needs no bundled font files. If `render` reports that the renderer source differs, reproduce from its recorded Git revision in a separate worktree. This leaves your current checkout intact:

```bash
repo_root="$PWD"
renderer_revision=$(node -p "require('./bench/publication.json').renderer.revision")
renderer_worktree="$(mktemp -d)/renderer"
git worktree add --detach "$renderer_worktree" "$renderer_revision"
(
  cd "$renderer_worktree"
  node bench/bench.mjs render "$repo_root/bench/publication.json" \
    --output-dir "$repo_root/bench/.work/reproduced"
  node bench/bench.mjs verify-images "$repo_root/bench/publication.json" \
    --against "$repo_root/bench"
)
```

The six SVGs committed before this JavaScript migration have no `publication.json`. They are historical images and cannot be reproduced from committed chart data. The commands above apply after the first new benchmark publication.

## Generate a fresh local publication

This measures compilers on **your machine** and creates a new set of seven SVGs. It does not reproduce CI's numerical results. Requirements: Node 24, pnpm, npm, git, Rust, and Hyperfine 2 with hardware counters. On macOS ARM64, the runner can download a checksum-verified Hyperfine 2.0.0 binary; elsewhere install Hyperfine 2 or pass `--hyperfine /path/to/hyperfine`. TypeScript 7 native packages support macOS/Linux ARM64/x64.

This complete example measures the public `webpack` project against a local release build of tsrs and TypeScript 7.0.2:

```bash
pnpm install --frozen-lockfile --ignore-scripts
cargo build --release -p tsrs_cli
node bench/bench.mjs plan --projects webpack
bundle=$(node bench/bench.mjs run \
  --projects webpack \
  --typescript-versions 7.0.2 \
  --tsrs target/release/tsrs \
  --checkers 4 --workers 8 --runs 10 --warmup 3 \
  --out-dir bench/results | tail -n 1)
node bench/bench.mjs render "$bundle" --output-dir bench/.work/local-publication
node bench/bench.mjs verify-images \
  bench/.work/local-publication/publication.json \
  --against bench/.work/local-publication
```

Open the `.svg` files in `bench/.work/local-publication/`. The complete run bundle, including Hyperfine exports and compiler logs, is at the path in `bundle`. Its generated `publication.json` can recreate these images later with `render`.

For the full project list, run `node bench/bench.mjs plan --projects all` and replace `--projects webpack` with `--projects all`. CI measures all projects with TypeScript nightly, TypeScript 7.0.2, and a PGO + BOLT tsrs build on a fixed 8-vCPU runner. To compare a published tsrs package instead of a local binary, replace `--tsrs target/release/tsrs` with `--tsrs-version latest`. Package tags resolve once per run; the bundle records exact versions and binary hashes.

`run` calls `prepare` automatically. Defaults are TypeScript `next`, `vscode,mui-docs,webpack,t3code-server`, four checkers, available worker CPUs, ten measured runs, and three warmups. It also accepts `--memory-gib N`, `--work-dir`, `--out-dir`, `--hyperfine`, and `--dry-run`. `--memory-gib` applies the soft `--maxMemory` and `GOMEMLIMIT` budgets.

Every timed compiler invocation uses `-p <project> --noEmit --incremental false --pretty false --checkers <N>`. `--extendedDiagnostics` runs separately for file and diagnostic counts; adding it to a timed command would change compiler work at exit. Hyperfine measures wall time, retired instructions, peak RSS, and exit status. A hardware-counter probe fails before the matrix if those counters are unavailable. Diagnostic exits 1 and 2 are valid; crashes, changing exit codes, and missing samples are rejected.

## Merge independent project runs

CI measures projects separately and merges complete bundles before rendering. Use the same settings and compiler builds for every partial run:

```bash
node bench/bench.mjs merge /path/to/project-a-bundle /path/to/project-b-bundle \
  --output-dir bench/.work/merged
node bench/bench.mjs render bench/.work/merged \
  --output-dir bench/.work/merged-publication
node bench/bench.mjs verify-images \
  bench/.work/merged-publication/publication.json \
  --against bench/.work/merged-publication
```

`merge` requires matching machine, settings, compiler builds, and repository revision, and rejects duplicate projects. It retains the original partial metadata and raw project files.

## CI and retained data

`update-benchmarks.yml` builds a PGO + BOLT tsrs binary and Hyperfine 2, prepares each pinned project and measures TypeScript nightly, TypeScript 7.0.2 and tsrs on the same 8-vCPU runner specification. Each project uses four checkers, eight worker CPUs, three warmups and ten measured runs. The publish job merges the partial bundles, renders SVGs, re-renders from `publication.json` and verifies the outputs before opening a draft PR. It commits the compact publication data and images; complete raw samples, logs and package locks stay in workflow artifacts for 14 days.

`merge` requires matching compiler builds, settings and machine details, and rejects duplicate projects. Source partials remain in the merged artifact. Raw samples and project checkouts remain ignored by git.

On pull requests and merge groups, `ci.yml` compares ordered diagnostics and exit codes against the base build for
the determinism job's four projects and regression cases. For pull requests that may affect performance,
`wallclock.yml` compares main and PR release binaries. When they differ, separate 8-vCPU jobs measure alternating
timing pairs, peak RSS and instruction counts for each catalog project. The report job joins available results in
catalog order and posts one PR table; missing or invalid project results appear as failed rows. The older comparison
and probe workflows remain disabled under `.github/workflows/*.bak`.

## Specialized runs

```bash
# One Linux command, with instructions and RSS in a bundle:
node bench/bench.mjs count --output-dir /tmp/count -- /path/to/tsrs -p /path/to/project --singleThreaded --noEmit

# Head-to-head comparison over thread counts:
node bench/bench.mjs compare --tsrs target/release/tsrs --bun /path/to/bun \
  --projects vscode --threads default,4,8 --reps 20

# Startup and rebuild measurements; the edited source file is always restored:
node bench/bench.mjs watch --tsrs target/release/tsrs --project vscode

node bench/bench.mjs history --results-dir bench/results
node bench/bench.mjs regressions --results-dir bench/results
```

`history` orders results by the first-parent commits on `origin/main`, reports changes against the previous compatible machine and settings, and marks instruction or memory regressions with `!`. Use `--all` to include results from other branches, `--branch` to select another history, or `--tsv` for tab-separated output. `regressions` compares with the nearest compatible ancestor when Git history is available and falls back to the preceding run by date. It prints a table and GitHub Actions warnings; `--comment --repo owner/name --issue N` is an explicit option for posting a comment.

## CodSpeed simulation

`.github/workflows/codspeed.yml` runs a separate single-threaded compiler benchmark on `Compiler-Unions`, `xstate-main`, `webpack`, and `drizzle-orm`. `tsrs_bench` first captures the project's filesystem responses, then measures fresh parses, binds, checks, and diagnostic formatting from that immutable snapshot. Capture and output verification are outside the measured region; an unrecorded file request or changed result fails the benchmark. This is a separate baseline from the CLI wall-time measurements above.

From the repository root, install `cargo-codspeed` and prepare one project before running the simulation locally:

```bash
cargo install cargo-codspeed --version 5.0.3 --locked
node bench/bench.mjs prepare --projects Compiler-Unions
cargo codspeed build -p tsrs_bench --bench compiler --features codspeed --profile release --locked -m simulation
RAYON_NUM_THREADS=1 TSRS_BENCH_PROJECT=Compiler-Unions \
  cargo codspeed run -p tsrs_bench --bench compiler -m simulation
```

Outside CodSpeed's Linux instrumentation, the last command exercises capture, replay, and output verification without reporting performance measurements. Use `--profile dev` for local correctness builds. CI uses the release profile without PGO and keeps debug symbols for flame graphs.

The current committed `RESULTS.md` and its SVGs are historical until a real run replaces them. Do not regenerate them from synthetic measurements.
