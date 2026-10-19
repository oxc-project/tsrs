# Benchmarks

[`run.py`](run.py) benchmarks one tsrs release against one or more native TypeScript 7 versions, on selected
repositories from [`projects.json`](projects.json). Each invocation uses one resource configuration for all
compilers. Hyperfine 2 records individual wall-time, instruction-count, peak-RSS and exit-code samples.

```bash
python3 bench/run.py \
  --typescript-versions next 7.0.2 \
  --tsrs-version latest \
  --projects vscode mui-docs webpack t3code-server
```

Versions can be exact npm versions or dist-tags; each tag is resolved once and the actual versions, package
lockfiles, binary hashes, repository commits, commands and environment settings are saved with the raw data.
Use exact versions from that metadata to repeat a comparison. TypeScript versions must provide native compilers
(TypeScript 7); JavaScript `tsc` packages do not accept the same checker/resource flags.

| Parameter | Default | Purpose |
| --- | --- | --- |
| `--typescript-versions` | `next` | One or more versions/tags, separated by spaces or commas |
| `--tsrs-version` | `latest` | One published tsrs version/tag |
| `--projects` / `--repos` | `vscode,mui-docs,webpack,t3code-server` | Repository names from the catalog; `all` selects every entry |
| `--checkers` | `4` | Checker count for every compiler |
| `--workers` | Available CPUs | Worker pools: `GOMAXPROCS` and `RAYON_NUM_THREADS` |
| `--memory-gib` | Unset | Optional soft budget: tsrs `--maxMemory` and Go `GOMEMLIMIT` |
| `--runs` / `--reps` | `10` | Measured repetitions per compiler/repository |
| `--warmup` | `3` | Warmups per compiler/repository |
| `--work-dir` | `bench/.work` | Cached checkouts, dependencies and compiler packages |
| `--out-dir` | `bench/results` | Parent directory for a fresh result bundle per invocation |
| `--hyperfine` | On PATH, otherwise macOS ARM64 download | Hyperfine 2 executable |

For a different resource configuration, pass explicit values in a separate invocation:

```bash
python3 bench/run.py --typescript-versions next 7.0.2 --tsrs-version latest \
  --projects webpack --checkers 14 --workers 14 --memory-gib 32 --runs 10
```

Go GC retains its default behavior. Memory budgets are soft limits, not hard RSS caps. Peak RSS is the process's
peak resident memory, not total allocated memory. Compiler flags always include
`-p <project> --noEmit --incremental false --pretty false --checkers <N>`.

The runner requires Python 3.9+, Node/npm, pnpm and git. Native npm packages are selected for macOS/Linux and
ARM64/x64. Supply Hyperfine 2 with hardware-counter support via PATH or `--hyperfine`; macOS ARM64 can download
checksum-verified Hyperfine 2.0.0 automatically. A counter probe fails early if instruction counts are unavailable.

Every final Hyperfine command is printed in copyable Bash syntax before it runs, including its working directory
and cleaned environment. `--dry-run` prepares repositories and compilers, then prints commands without measuring.
`--setup-only` only prepares repository checkouts/dependencies. To benchmark a local release build, replace
`--tsrs-version` with `--tsrs /path/to/tsrs`.

```bash
python3 bench/run.py --typescript-versions next 7.0.2 --tsrs-version latest --projects webpack --dry-run
python3 bench/run.py --projects all --print-projects
```

## Results and charts

Results stay in ignored `bench/results/<timestamp>/` directories. Do not commit measurements, generated charts,
compiler installs or project checkouts. CI uploads measurements and figures as downloadable artifacts.

Each completed bundle contains `metadata.json`, `raw.csv`, `raw.json`, package lockfiles and, per repository,
`hyperfine.json`, `preflight.json` and compiler logs. Preflights collect file counts and diagnostic locations/codes
outside timed runs; differences between compiler versions are recorded. Raw rows are never aggregated.

The two chart scripts consume the same completed bundle:

```bash
# Existing scatter / per-project style; choose a TypeScript reference if multiple were measured.
python3 bench/chart.py bench/results/<timestamp> \
  --typescript-version 7.0.2 --output bench/results/<timestamp>/scatter.svg
python3 bench/chart.py bench/results/<timestamp> \
  --per-project wall_s --output bench/results/<timestamp>/wall-time.svg

# New bar style, showing every measured version.
python3.12 -m venv bench/.work/charts-venv
bench/.work/charts-venv/bin/python -m pip install -r bench/requirements-charts.txt
bench/.work/charts-venv/bin/python bench/chart_bars.py bench/results/<timestamp>
```

[`chart.py`](chart.py) keeps the existing light/dark SVG styles and can still read historical result JSON.
[`chart_bars.py`](chart_bars.py) draws one figure with wall time and peak memory per repository: tsrs in Cloudflare
orange, TypeScript releases in shades of TypeScript blue, using Matplotlib's default DejaVu Sans font. No font
files are bundled. The number of bars and legend rows follows the selected compiler versions.

The bar renderer writes `benchmark.png`, `benchmark.svg`, `means.csv`, `means.json`, `alt-text.txt`,
`chart-manifest.json` and an `index.html` with parameter facts under `<result>/charts/`. Use `--output-dir` to
choose another destination, `--title` for a custom heading, and `--baseline <TypeScript-version>` to choose the
comparison reference (default: the last selected TypeScript version). Legends always show the measured versions.
Both renderers average repetitions within each compiler/repository; repositories are never combined. Historical
JSON retains the summaries recorded by its original runner.

Rendering needs only the saved bundle, with no compiler binaries or repository checkouts. [`results.py`](results.py)
checks samples against Hyperfine exports, including units, instructions, exit codes, commands and run counts.
SVG dates/IDs are deterministic; the pinned plotting environment keeps chart regeneration reproducible.

## Suite mapping

Suite commit `41f652ab` (`bench/projects.json`). Each suite case is a `scenario.json` (`tsc -p <dir> --outdir <tmp>`)
plus a `setup.sh` (clone, install inside a network-restricted Docker sandbox). We run the same clone and install
commands without the sandbox, and check the same `-p` path.

| suite case | repository | commit | `-p` | install (from the case's `setup.sh`) |
| --- | --- | --- | --- | --- |
| `vscode` | microsoft/vscode | `9cf0128b9822` | `src` | `npm ci --ignore-scripts` |
| `xstate-main` | statelyai/xstate | `fbee62e7c158` | `.` | `pnpm dlx <packageManager> install` (pnpm 10; runs its postinstall `preconstruct dev`, like the suite) |
| `webpack` | webpack/webpack | `b54c5e2aa26d` | `.` | `yarn install --ignore-scripts --ignore-engines`, then `yarn link` webpack into itself |
| `mui-docs` | mui/material-ui | `6780195595c1` | `docs` | `pnpm dlx <packageManager> install --ignore-scripts` (pnpm 12) |
| `Compiler` | (in the suite repo) | suite commit | `cases/solutions/Compiler` | none |
| `Compiler-Unions` | (in the suite repo) | suite commit | `cases/solutions/Compiler-Unions/tsconfig.json` | none |

Commits: the suite pins only `mui-docs` (`6780195595c1`, "last MUI ref known to install in the sandbox"); the other
three clone the repository's HEAD at run time. We pin those to the commits used for the typescript-benchmarking
numbers in microsoft/TypeScript#64475 (gist `c83d9185c6961b6fb43d7d071ef39cf6`, `refs.txt`, late September 2026).
The suite runs yarn 1 from its Docker image; we run `pnpm dlx yarn@1.22.22` (webpack's `packageManager`).

`Compiler` is defined in `cases/` but not scheduled in the suite's pipeline (`scripts/src/setupPipeline.ts` lists only
`Compiler-Unions`); it is kept because the #64475 measurements include it.

Not included: `angular-1` (its tsconfig uses options TypeScript 7 removed); the `*-1` variants (the suite's frozen
baseline copies of the same four projects at older pinned commits, e.g. vscode `f88bce8f`); `strada-compiler`,
`ts-pre-modules`, `strada-build-src`, `self-build-src-public-api` (TypeScript's own 5.x sources, two of them `tsc -b`
builds); the tsserver/LSP/startup scenarios (not `tsc` runs). All six included projects fit a standard runner.

## Application projects

The suite has one application-shaped workload (`mui-docs`), and none of its projects leans on schema-validation
types. Application projects include four large applications (three Next.js/React apps built on Zod and tested with Vitest, and an
Effect server), plus Bun's published benchmark set (mikro-orm, next.js root and packages/next, storybook, nuxt, playwright)
and three more popular libraries (drizzle-orm, typeorm, date-fns). Each is pinned to one commit.

| project | repository @ commit | `-p` | files | workload |
| --- | --- | --- | ---: | --- |
| `cal-diy` | calcom/cal.diy @ `54343aa685ae` | `apps/web` | 10,166 | Zod 3, tRPC 11, Prisma-generated Zod types |
| `formbricks-web` | formbricks/formbricks @ `ec45c28e4d20` | `apps/web/tsconfig.typecheck.json` | 10,451 | Zod 4, Vitest, Prisma 7 |
| `supabase-studio` | supabase/supabase @ `12afe3999951` | `apps/studio` | 12,546 | Zod 3, Vitest, Next.js route types |
| `t3code-server` | pingdotgg/t3code @ `9bd1d8009a6b` | `apps/server` | 3,007 | Effect 4, Effect Schema, `@effect/vitest` |
| **Bun benchmark set** | | | | |
| `mikro-orm` | mikro-orm/mikro-orm @ `97fb231` | `.` | 2,911 | TypeScript 7 monorepo, ORM with schema types |
| `next-packages-next` | vercel/next.js @ `fa8dcf3` | `packages/next` | 2,882 | Next.js core package (Bun measured 2,881 files) |
| `next-root` | vercel/next.js @ `fa8dcf3` | `.` | 3,549 | Next.js repo root monorepo (Bun measured 3,547 files) |
| `storybook` | storybookjs/storybook @ `48dfcc6` | `scripts` | ~1,039 | Storybook scripts (Bun measured 1,039 files) |
| `nuxt` | nuxt/nuxt @ `85b8d54` | `.` | ~3,901 | Nuxt framework monorepo (broader tsconfig than Bun's 839; same commit) |
| `playwright` | microsoft/playwright @ `d469960` | `.` | ~1,515 | Playwright testing framework (broader tsconfig than Bun's 706; same commit) |
| **Other popular libraries** | | | | |
| `drizzle-orm` | drizzle-team/drizzle-orm @ `15454db` | `.` | ~943 | Drizzle ORM (large popular project) |
| `typeorm` | typeorm/typeorm @ `fe9239428751` | `packages/typeorm` | 3,575 | TypeORM sources and tests, decorators |
| `date-fns` | date-fns/date-fns @ `717ce0a807ea` | `pkgs/core` | 2,277 | date-fns core, already on TypeScript 7 |

**Original application projects:** Each install command does what the app's own `typecheck` needs before `tsc` runs, with package scripts off:

- `cal-diy`: Yarn 4 install, then `turbo run post-install @calcom/trpc#build` (Prisma client, Zod and Kysely types,
  the platform packages, and the tRPC router declarations `apps/web` imports). `YARN_NM_MODE=classic` keeps a global
  Yarn configuration (hardlinked node_modules) from changing the install, and the package cache goes to `$TMPDIR` so
  it is not part of the project's CI cache.
- `formbricks-web`: pnpm install, then `turbo run build --filter='@formbricks/web^...'`, which generates the Prisma
  client and builds the workspace packages whose `dist/*.d.ts` the app imports. The build scripts call `pnpm`, so the
  command uses pnpm's automatic version selection from the upstream `packageManager` pin. `DATABASE_URL` is a dummy
  (Prisma's config requires one to generate; nothing connects).
- `supabase-studio`: pnpm install of `studio` and its workspace dependencies, then `next typegen` (the app's
  `pretypecheck`).
- `t3code-server`: pnpm install of `t3`, `@t3tools/scripts` (`apps/server`'s tsconfig includes `scripts/lib`) and
  their workspace dependencies. The repository's `prepare` script patches its TypeScript for the Effect language
  service, which neither compiler measured here runs.

**Bun benchmark set and large popular projects:** Minimal installs without build steps (the projects' tsconfigs require only node_modules):

- `mikro-orm`: Yarn 4 install.
- `next-packages-next` and `next-root`: pnpm install with `--frozen-lockfile`.
- `storybook`: Yarn 4 install (scripts directory).
- `nuxt`: pnpm install.
- `playwright`: npm ci.
- `drizzle-orm`: pnpm install.
- `typeorm`: pnpm install of `typeorm` and its workspace dependencies.
- `date-fns`: pnpm install, then `tsc --build` of the projects `pkgs/core` references (`pkgs/tz`, `pkgs/utc`,
  `pkgs/docs`), with date-fns's own TypeScript 7, so their declarations exist.

**Overlays.** cal.diy, Formbricks and TypeORM still compile with TypeScript 5.9, whose tsconfig options TypeScript 7 removed
(`baseUrl`, `moduleResolution: node`, `target: es5`); unmodified, both stop at config errors before checking a
file. `bench/overlays/<name>/` holds the minimal TypeScript 7 replacements, copied over the checkout before the
install: `baseUrl` becomes a `"*": ["./*"]` path, `node` resolution becomes `bundler` (with `module: esnext` for
TypeORM, whose `commonjs` does not allow it), ES5 becomes ES2017.
cal.diy's generated tRPC declarations import `@trpc/server/dist/...`, which `bundler` resolution blocks through the
package's `exports`, so a `paths` entry maps it to the package directory as `node` resolution did (without it,
tsgo reports 727 errors instead of 136). Formbricks also gets the `next-env.d.ts` Next.js writes on `next dev`
(image module types). The overlay's content hash is part of the checkout marker and the CI cache key.

**Errors.** The applications are not error-free under TypeScript 7 with these settings (cal.diy: 136, mostly test
matchers and untyped resolver modules). As with vscode and webpack, the count is a correctness signal between the two
compilers. On 2026-10-06 (local, macOS arm64), tsgo 7.0.2 and tsrs reported identical errors on cal.diy (136) and
Formbricks (0); on Supabase Studio (0 vs 9) and t3code (5 vs 6), `typescript@7.1.0-dev.20260930.4` reported exactly
tsrs's errors (the table's errors paragraph names such projects).

## Watch mode

`bench/watch.py` measures `--watch` on vscode: tsrs against the tsgo package in `projects.json`, both started as
`--watch -p src --noEmit --preserveWatchOutput --pretty false` with no thread flags. bun check has no watch mode (Bun
1.4.4 canary: `bun check` has no `--watch` flag, and `bun --watch check` checks once and ignores later edits), so it has
no column. Each session:

- **initial build**: from the start of the process to the first "Watching for file changes" report;
- **first comment edit**: a comment appended to `src/vs/base/test/common/strings.test.ts`, a test file nothing imports;
- **repeat comment edit**: five more comment edits to that file, the median of them;
- **type error in a leaf file**: an exported assignment error appended to it (the cycle must report exactly one error
  more than the initial build, or the session fails);
- then the file is restored (the cycle must report the initial build's errors again; not in the table).

An edit is timed from just before the write to the last "Watching for file changes" report of the cycles it started:
after a report the harness waits two quiet seconds for another, so a watch that rebuilds twice for one save is timed
to the second. Memory is the process's peak RSS over the whole session (`wait4`) and its RSS after the last edit. CI
runs 5 tsrs and 3 tsgo sessions (the compiler order alternates) on its own fixed-spec machine, the `measure-watch` job;
the tables report medians and the result keeps every session under `watch.sessions`. `bench/run.py --merge` attaches
the result as the merged result's `watch` section, which `bench/results/<...>.md` and `bench/RESULTS.md` render as a
"Watch mode" table; the chart does not show it.

```sh
python3 bench/watch.py --local                                   # 3 sessions each, 5 repeat edits
python3 bench/watch.py --local --reps 1 --edits 2 --out-dir /tmp/w   # smoke test
```

## CI

CodSpeed runs on pushes to main, pull requests, and manual dispatch.

[Update benchmarks](../.github/workflows/update-benchmarks.yml) prepares the pinned catalog projects, builds the
PGO/BOLT tsrs binary and Hyperfine once, and measures each project with one configuration: 4 checkers, 8 worker
CPUs, default memory policy, 3 warmups and 10 repetitions against TypeScript nightly and 7.0.2. Both chart styles
are generated from each result bundle. Raw samples, logs and charts are uploaded as artifacts; the workflow does
not commit results or open results PRs.

[Benchmark tooling](../.github/workflows/benchmark-checks.yml) runs the offline execution, validation and chart
checks when benchmark code changes. No committed measurement archive is needed.

The comparison and probe workflows are disabled, including manual dispatch. Their definitions are preserved under
`.github/workflows/` as `bench-compare.yml.bak`, `pr-verify.yml.bak` and `perf-probe.yml.bak`. The former `bench.yml`
is available in Git history. On pull requests and merge groups, the active `ci.yml` compares ordered diagnostics and
exit codes against the base build for the determinism job's four projects and regression cases. It does not run the
saved workflow's performance measurements or its full project matrix.
The completed build-layout experiment's workflow and build script were removed; `tools/perf/layout-measure.py`
still compares supplied binaries. Local measurement tools remain available; recorded runs are archived in Git history.

`wallclock.yml` compares the main and PR release binaries on pull requests that may affect performance. If the
binaries differ, it runs one job per `bench/projects.json` project on separate 8-vCPU runners. Each job measures
alternating main/PR timing pairs, peak RSS and instruction counts, then uploads its project result. The report job
downloads all available results, joins them into the `wallclock-joined` artifact in project order and posts one PR
table. Missing or invalid project results appear as failed rows, so a failed shard does not hide the others.

For those saved workflows, restore the original `.yml` filename before using their `gh workflow run` commands.
They produce no automated measurements, regression comments or result/chart updates while disabled.

### CodSpeed

`.github/workflows/codspeed.yml` measures Compiler-Unions, xstate-main, webpack, and drizzle-orm in three ways on
every push to main, pull request, and manual dispatch:

| Instrument | Work measured | Runner | Benchmark name suffix |
| --- | --- | --- | --- |
| CPU simulation | One single-threaded compilation against a captured filesystem | GitHub x64 | `compiler::<project>` |
| Memory | One disk-backed compilation with the CLI's default checker count | GitHub x64 | `compiler_default::<project>` |
| Walltime | The same default-parallelism, disk-backed compilation | CodSpeed Graviton arm64 Macro Runner | `compiler_default::<project>` |

The two benchmark families have separate baselines, and memory and walltime use the default checker count of their
respective runners. The release profile has no PGO or BOLT training; the published CLI and Hyperfine workflows answer
different questions. CodSpeed reports are advisory. The broader PR comparison remains in `wallclock.yml`.

#### CPU simulation

`tsrs_bench` uses Criterion2's built-in CodSpeed support. It first runs an unmeasured
compilation that records filesystem responses: source and config text, package metadata, directory listings,
negative probes, realpaths, and case sensitivity. Subsequent compilations read only that immutable snapshot;
an unrecorded request or a filesystem mutation fails the benchmark rather than falling back to disk.

Only `runner.run(...)` inside `iter_with_setup_wrapper` is measured. It parses configuration, resolves modules,
parses, binds, checks, and formats diagnostics into memory using fresh compiler state. Setup and comparison of
the output and exit status with the capture are outside measurement. Compiler diagnostics are valid outcomes;
crashes, missing project configs, and mismatched results fail. Statistics are disabled because they read process
memory information from the OS and contain nondeterministic timings.
Compiler environment overrides (`TSRS_*`, except `TSRS_BENCH_PROJECT`) are rejected, including profiling switches
that could bypass the filesystem interface.

Criterion2 3.0.4 runs five unmeasured warmups and one measured compilation. CI builds the benchmark suite once
and shares the compiled artifact with a four-job matrix. Each shard runs one project in parallel, setting up
and caching only that project's inputs. Each process uses `RAYON_NUM_THREADS=1`
and the CLI's large stack. CodSpeed also sets `MIMALLOC_ARENA_PURGE_MULT=-1` to disable arena purge scheduling:
the short-lived benchmark process reclaims its seven compilations' arenas at exit, while avoiding allocator clock
reads and purge system calls in the simulated profile. This CodSpeed-only setting deliberately differs from the
production allocator configuration.
`TSRS_BENCH_PROJECT` is required because Criterion2's CodSpeed adapter does not implement benchmark filters.
The target requires the `codspeed` feature: ordinary Criterion calibration would accumulate arena allocations.
Benchmarks are named `compiler::<project>` and start a new baseline, distinct from the former disk-backed CLI measurements.

#### Memory and walltime

The `compiler_default` target uses CodSpeed's Rust hooks around exactly one compilation in a fresh process. It does
not capture the filesystem or warm up the compiler first: earlier compilations would leave arenas resident and
inflate peak memory. Project existence checks, benchmark setup, and outcome checks are outside the measured section.
Walltime mode also records one elapsed-time sample for `cargo-codspeed` to aggregate; the hooks alone provide a
profile but no walltime result.
Diagnostics are valid outcomes; missing projects, crashes, and invalid statuses fail. Output is formatted into memory
to avoid putting thousands of diagnostics in CI logs. Walltime includes filesystem calls and checker parallelism.

CodSpeed's memory instrument reports peak physical memory, heap allocation metrics, and allocation flamegraphs.
Peak physical memory includes tsrs's arena pages; heap allocation flamegraphs chiefly describe mimalloc calls and
do not attribute the arena's direct `mmap` calls as heap allocations. Memory profiling tracks every allocation, so
large projects can cost more to profile. The walltime job uses the arm64 Graviton Macro Runner for stable elapsed-time
measurements; it builds on GitHub's Ubuntu 22.04 arm64 runner first, so compiler time is not charged to the Macro
Runner. Changing the Macro Runner's machine family starts a new walltime baseline.
The walltime artifact enables mimalloc's `no_opt_arch` feature because the Macro Runner lacks the ARMv8.1 LSE
atomics that mimalloc v3 otherwise emits on arm64. The x64 instruments and release binaries keep their normal
allocator build.

The workflow builds each instrument separately and runs one project per job in the same workflow so CodSpeed can
aggregate the shards. The memory and walltime builds use `cargo-codspeed` mode-specific instrumentation. GitHub OIDC
authenticates uploads without a long-lived CodSpeed secret.

From the repository root:

```sh
cargo install cargo-codspeed --version 5.0.3 --locked
python3 bench/run.py --setup-only --projects Compiler-Unions,xstate-main,webpack,drizzle-orm
cargo codspeed build -p tsrs_bench --bench compiler --features codspeed --profile release --locked -m simulation
MIMALLOC_ARENA_PURGE_MULT=-1 RAYON_NUM_THREADS=1 TSRS_BENCH_PROJECT=Compiler-Unions cargo codspeed run -p tsrs_bench --bench compiler -m simulation
cargo codspeed build -p tsrs_bench --bench compiler_default --features codspeed --profile release --locked -m memory
TSRS_BENCH_PROJECT=Compiler-Unions cargo codspeed run -p tsrs_bench --bench compiler_default -m memory
cargo codspeed build -p tsrs_bench --bench compiler_default --features codspeed,mimalloc-safe/no_opt_arch --profile release --locked -m walltime
TSRS_BENCH_PROJECT=Compiler-Unions cargo codspeed run -p tsrs_bench --bench compiler_default -m walltime
```

The simulation command exercises capture, warmups, replay, and output verification outside CodSpeed's Linux
instrumentation without reporting performance measurements. The other targets still execute one compilation there.
Use `--profile dev` when building for local correctness checks. CI uses the release profile with debug symbols
retained by cargo-codspeed.

#### Other CodSpeed capabilities

The current [CodSpeed instruments](https://codspeed.io/docs/instruments) are CPU simulation, memory, and walltime.
Simulation uses cycle estimation and breaks down flamegraph time into instruction, cache, and memory costs. We leave
[allocation exclusion](https://codspeed.io/docs/instruments/cpu/allocation-exclusion) off: a new allocation on a hot
compiler path should remain visible. The benchmark does not spawn child processes, so subprocess tracking adds nothing.
CodSpeed's [comparison view](https://codspeed.io/changelog) includes environment differences and differential,
process/thread-aware flamegraphs; these need no extra workflow setting.

[Sharding](https://codspeed.io/docs/features/sharded-benchmarks) is already used. We run all four projects on every
workflow run because most compiler changes can affect each of them; [partial runs](https://codspeed.io/docs/features/partial-runs)
would reduce that coverage. Benchmark archival is for a future removal or rename. This workflow does not configure
CodSpeed's regression thresholds or a [required performance check](https://codspeed.io/docs/features/performance-checks);
the repository's landing criteria still use instruction counts, two walltime publishes, and peak RSS
(`AGENTS.md`). For investigation, use the CodSpeed [CLI](https://codspeed.io/docs/cli),
[MCP server](https://codspeed.io/docs/ai/mcp), or [Wizard](https://codspeed.io/docs/ai/wizard)
against the uploaded profiles. Those account integrations are optional and do not change CI.

### Historical benchmark workflow

This section describes the removed `bench.yml` workflow; its dispatch commands require restoring it from Git history.
The workflow was built for unattended runs: one run per measured commit (the concurrency group is the commit, so a dispatch for another commit
starts its own run on its own machines and only a re-dispatch of the same commit replaces a queued twin; until
2026-10-07 one shared group let a later push replace the queued bench, and bursts of merges left most commits without
a results file), never cancelled, network steps retried, and the results commit re-created on top of the latest
`main` before pushing. Runs overlap and finish in any order, so `bench/RESULTS.md` and `bench/large-projects.svg` keep
the newest commit's results: a run whose commit is an ancestor of the one the table came from (its last line names
it) adds its results file and leaves the tables and chart alone. The regression flag compares a run with the nearest
earlier benchmarked ancestor of its commit, not with the newest file by date. A pull request that changes code must
not carry a CI-skip marker in its title, body or commit subjects (the local check is
`tools/ci/skip_marker.py`): `gh pr merge --squash` copies them into the commit message, and such a
commit lands without CI and without a results file; notes-only pull requests carry the marker on purpose. A commit
whose bench was skipped or cancelled (before 2026-10-07, most commits of a burst of merges) is backfilled with `gh workflow run bench.yml --ref main -f commit=<sha>`: the build and measuring jobs check out that commit, the merge job runs the current scripts, the results
file is committed, and the tables and chart update only when no newer commit's report is there.

**Noise**: the benchmark machines are shared cloud VMs. Historically, between publishes of the same code vscode's
64-vCPU wall moved by a median 2% and up to 10% on 2026-10-07 (median of 3). The single-threaded instruction count is
the deterministic measure (repeats to 0.001%). The `measure` jobs run 5 reps and `measure-wide` 10 for tsrs and
`bun check`, both with 3 for tsgo (`--reps N --tsgo-reps M`: tsgo takes 10-20x longer per run).
[The historical 64-vCPU comparison](RESULTS.md#historical-comparison-on-64-vcpus) uses the 20-run mean of
`bench-compare.yml`. The README chart updates with the latest tables.

All benchmark jobs use standard GitHub-hosted `ubuntu-24.04` runners. Manual comparison, verification and general
probe workflows accept a `runner` label (default `ubuntu-24.04`) for a larger GitHub-hosted Linux x64 runner if one
is configured. Runner vCPUs and checker threads are separate: 16 checker threads on a 4-vCPU runner oversubscribe it.
Historical Depot results retain their original hardware labels. History starts a new baseline when hardware or
build setup changes; the migration does not rewrite measured results. Backfills measure on the current runners.

**History**: `bench/history.py` prints one table per metric over `bench/results/*.json` in `main`'s first-parent
order, a column per project, each cell with its change against the previous benchmarked commit (with one run per
push, the previous merge): `--metrics instructions,peak,wall,wide_wall,wide_peak`, `--projects`, `--since <commit>`,
`--tsv`. Instructions and peak RSS are deterministic, so a `!` cell (past the regression thresholds) is the commit's
doing; the wall columns are the noisy ones and say whether the cost bought anything, and a `~` marks a wall or
wide-peak change next to an instruction change under 0.3%: the code did not change, that is the runner.

Five jobs (since 2026-10-07; until then one job did everything in sequence, and the measurement alone took 18-20 min
once the four application projects were in; `measure-watch` since 2026-10-13):

1. `build`, on one machine of the fixed spec: the PGO + BOLT `dist` build of the commit (below). The binary and the
   `rustc -V` that built it go up as the `tsrs-pgo-dist` artifact.
2. `measure`, one job per project of `bench/projects.json` (the matrix is `bench/run.py --print-projects`, so a new
   project gets a job without a workflow change), each on its own machine of the fixed spec: it restores the project's
   cache, downloads the binary and runs `bench/run.py --projects <name> --out-dir <dir>`. The result is the
   `bench-partial-<name>` artifact, the compiler output `bench-logs-<name>`. The jobs run at the same time but never
   share a machine, so a measurement is what it was in the sequential run: one project on an idle runner.
3. `measure-wide`, two jobs on `ubuntu-24.04`, each with every other project of `bench/projects.json`, so each
   project's Bun comparison modes come from one machine: default mode (`wide`) and `--checkers` 4, 8 and 16
   (`wide-checkers4`, `wide-checkers8`, `checkers16`), with `bun check` from Bun canary as a third column (`--bun`).
   The jobs restore the caches the `measure` jobs save. If either fails, the result contains only the other's
   cells and the tables and chart stay unchanged (`bench/publish.py` requires every project).
4. `measure-watch`, one job on a machine of the fixed spec: `bench/watch.py` on vscode ("Watch mode" below). It restores
   the vscode cache the `measure` job saves and raises the inotify watch limit. If it fails, the results have no
   watch table.
5. `merge`: `bench/run.py --merge <results...>` joins them into one result, projects in `bench/projects.json` order
   and modes in `MODE_FLAGS` order, after checking that the binary, the compilers and flags agree (the rep counts may differ, as the
   `measure` jobs' 5 and `measure-wide`'s 10 do; the result records every mode's in `mode_reps`) and that no
   (project, mode) is measured twice; then the regression flag and the results commit. The `Commit results` step
   runs `bench/publish.py` after fetching main on every push retry to keep the tables and chart current together.
   Of the fixed-spec partials' machines, the one that measured the most cells is the run's; a cell measured on another
   records its own `machine`. All Bun comparison cells do, and the table names their machine. A fixed-spec project measured on a different CPU model or C library
   is noted under the table, and the regression flag
   compares that project only with runs on the same model. A run started from a branch
   (`gh workflow run bench.yml --ref <branch>`) ends with the `bench-result` artifact: nothing is committed
   or commented.

**Fixed machine spec**: the `measure` jobs run on Namespace `nscloud-ubuntu-24.04-amd64-8x16` (8 vCPU, 16 GB RAM,
Linux x86_64; [runner labels](https://namespace.so/docs/reference/github-actions/runner-configuration)); the build and
publish jobs stay on GitHub-hosted `ubuntu-latest`. The hardware step checks CPU count, memory and architecture. CPU
models can differ; results record the actual machine and the regression/history tools compare compatible
measurements. The 16-checker mode oversubscribes these runners.

Caching uses GitHub's `actions/cache` service and the Rust setup action for
`~/.cargo` and the dependency part of `target/`, which holds the PGO-instrumented build (workspace crates are
rebuilt: they are what is being measured). The final PGO build is not cached: its RUSTFLAGS contain the profile's
hash, which changes every run, so cargo rebuilds all of it anyway (as in the release workflow); one
cache for the suite checkout plus the pnpm-installed compilers (the timed and reference packages); one cache
per cloned project (checkout + `node_modules`) keyed on its pinned commit and install command
(`bench/run.py --print-cache-keys`), so changing one pin re-installs only that project. Each measuring job restores
the shared cache and its own project's; the build job restores the two training projects (xstate-main, webpack) and
saves the shared cache. The build job restores the TypeScript
testdata (`ts-ref/tsc/testdata`, 22 MB compressed) from a cache keyed on the TypeScript commit; on a miss they run the
sparse checkout and save it. That saves about 4 s per job (2026-10-07: checkout 4.9-5.7 s, restore 1.4 s).

These cache details describe the disabled benchmark workflows. The active CI restores four pinned projects for the
determinism gate; it also checks out the pinned TypeScript source and fixtures for conformance and Fourslash.

Historical job duration split, 2026-10-01 (before this migration):

| | GitHub-hosted `ubuntu-24.04` (2 vCPU / 7 GB), before | Depot CI `depot-ubuntu-24.04-8`, all caches warm |
| --- | --- | --- |
| runner start, toolchain, rust-cache restore | ~20 s | 11 s |
| `cargo build --release -p tsrs_cli` | 80-134 s | 34 s |
| restore project caches | 45-85 s (one 5 GB cache) | 14 s (5 caches) |
| clone + install projects (cold cache only) | 151 s, then 91 s to save the cache | 0 s |
| measurement (`bench/run.py`, 6 projects x 2 modes x 2 compilers x 3 reps + warm-ups) | never finished: vscode alone took 19 min (tsgo swapped), cancelled | 424 s |
| commit + push results | | 2 s |
| total | | 8 min 10 s (measurement 87%) |

The first Depot run (cold caches: clone, install and cache save for all projects) took 11 min 3 s. What is left in
a warm run is the tsrs build (it changes with every commit) and the measurement.

Since the bench measures the PGO `dist` build (2026-10-01), the 28-34 s `cargo build --release` step is replaced by
the PGO pipeline, measured on Depot run `ps_h83310tmld`: TypeScript testdata sparse checkout ~14 s, instrumented build
of tsrs + tsrs-test 100 s, training run 15 s, profile merge < 1 s, final build 74 s, so ~3 min more per run (job 7 min
13 s -> 10 min 4 s). The dependencies, the only part rust-cache can reuse, compile in ~3 s; the time is the
workspace crates with one codegen unit and fat LTO, which change with every commit.

BOLT (2026-10-07, notes/perf-binary-layout.md) adds the LLVM tarball download, instrumenting tsrs, two training runs
and the rewrite: build job 7 min 21 s -> 7 min 42 s (Depot run `tq2x4vs94n` vs `main` at `0d3b6c08`), build start to
merge end 24 min 27 s -> 24 min 35 s.

Parallel layout, first run on `main` (2026-10-07, commit `ad65e977b0d7`, all caches warm): 13 min 51 s from the push
to the results commit, down from 26 min 35 s for the last sequential run (`bd945842ada5`, measurement 18 min 19 s).

| job | duration |
| --- | ---: |
| `build`: setup and cache restore | 27 s |
| `build`: instrumented build | 4 min 39 s |
| `build`: training run | 26 s |
| `build`: final build, package, upload | 2 min 37 s |
| `measure (vscode)`, the longest; measurement 295 s | 5 min 10 s |
| `merge` | 7 s |

The other measurements, in `partials` of the result: t3code-server 254 s, mui-docs 157 s, supabase-studio 143 s,
formbricks-web 122 s, cal-diy 91 s, webpack 34 s, xstate-main 15 s, Compiler-Unions 8 s, Compiler 5 s. All ten jobs
landed on the same CPU model. Each measuring job adds a runner start-up and cache restore (about 15 s for vscode) to
the runner minutes.

The measuring jobs still start when the build job ends. Starting them with the build and having them wait for its
artifact works on Depot CI (a job can download an artifact that a still-running job uploaded). It was tried on
2026-10-07 (Depot run `tdzf0g0cqn` vs `main`'s `ps_ghllxhtgmq`): measuring began 2-3 s after the build instead of
20 s (vscode) and 50 s (`measure-wide`) after it. But build start to merge end was 20 min 53 s vs 20 min 56 s,
because the 64-vCPU job's 13-minute measurement varies by more than that (762 s vs 779 s). And eleven runners, one of
them 64 vCPU, sat idle through the 7-minute build.
`count.py`, `watch.py` and `compare.py` remain specialized instruction-count, watch and branch-comparison tools.
`history.py`, `regressions.py` and `publish.py` can still read historical result JSON; their historical mode names
are not configurations of the current runner.

## Never time a run with `--extendedDiagnostics`

Never pass `--extendedDiagnostics` or `--diagnostics` to a run whose wall time or peak memory you measure: not in
`bench/`, `tools/perf/`, `upstream/tools/`, a one-off `time tsc ...` or `hyperfine`, nor in numbers quoted in a pull
request or note. The statistics report runs inside the process, after the check and before it exits, and it costs the
two compilers different amounts:

- tsgo calls `runtime.GC()` twice to settle the heap, then `runtime.ReadMemStats`, then `Program.LineCount()`, which
  builds the line map of every source file (`internal/execute/tsc/emit.go` `EmitAndReportStatistics`,
  `internal/execute/tsc/statistics.go`). Two full collections of a multi-GB heap add wall time and CPU, and the line
  maps add memory.
- tsrs reads its resident size from `/proc/self/statm` and counts lines without building or keeping the line maps
  (notes/mem-frontend.md), then sums the counters across checkers.

A timed run with the flag adds more work to tsgo than to tsrs and tilts the comparison toward tsrs; in a
tsrs-vs-tsrs comparison it still adds end-of-run work that is not type checking. When you need
the counters or phase times, take them from a separate untimed run: `bench/compare.py`'s warm-up, the stats runs of
`tools/perf/clusterprobe.py` and `tools/perf/probe.sh`, or `upstream/tools/measure.py` with `STATS=1`. Instruction
counts (`bench/count.py`) leave it off too.

## Head-to-head on a wide machine

`bench/compare.py` answers a different question from `run.py`: how tsrs, both TypeScript 7 builds and `bun check`
compare on one wide machine, the way Bun's `bun check` announcement measured them (vscode, Linux x64, 64 threads,
mean of 20 runs). It runs four compilers on the same checkouts:

- `tsgo`: the timed TypeScript package (`tsgo` in `projects.json`, as in `run.py`);
- `tsgo-dev`: `typescript@7.1.0-beta` (`reference` in `projects.json`), built from the TypeScript commit
  tsrs ports. At this pin it is the same package as `tsgo`; it remains the explicit like-for-like Go baseline and
  error oracle when `tsgo` advances;
- `tsrs`: the binary given with `--tsrs`;
- `bun`: `bun check` from the binary given with `--bun` (1.4.3 canary or later), run as
  `bun check -p <project> --no-pretty --all` (`--all` turns off the grouping of repeated errors so every error line is
  counted).

Thread settings (`--threads`, default `default,4,8,16,all`): `default` passes no flag (tsgo uses 4 checker threads,
tsrs every core up to 16, then half the cores, at most 32, bun one thread per core); a number N passes `--checkers N` to tsgo and tsrs and
`--threads N` to bun;
`all` is the machine's thread count. The knobs are not identical: `--checkers` sets only the checker threads (parsing
and binding still use every core), while bun's `--threads` caps all of its threads. Per project, one untimed warm-up
per compiler (tsgo and tsrs with `--extendedDiagnostics`, for the program's file count), then `--reps` (default 20) reps in which the compiler order rotates; the table reports mean ± standard
deviation, median and minimum wall time and mean peak RSS, plus whether each compiler's (file, line, column, code)
error list equals 7.1-dev's. bun's file count is its own "checked N files" summary, which is not the program's file
count (13 fewer on `Compiler`, 1,028 fewer on vscode).

```sh
python3 bench/compare.py --tsrs target/release/tsrs --bun ~/.bun/bin/bun --projects vscode --reps 20
python3 bench/compare.py --tsrs ... --bun ... --projects Compiler --threads default,4 --reps 2   # smoke test
```

The disabled [`.github/workflows/bench-compare.yml.bak`](../.github/workflows/bench-compare.yml.bak) defaults to `ubuntu-24.04`.
After restoring its original `bench-compare.yml` filename, run `gh workflow run bench-compare.yml --ref <branch>`;
use `-f runner=<label>` for a configured larger GitHub-hosted
Linux x64 runner. It uses the PGO `dist` build of the commit (built and trained as in `bench.yml`) and Bun canary, and uploads
`bench/results/compare/<date>-<commit>-<threads>t.{json,md}` plus the logs as the `bench-compare` artifact.

First run, 2026-10-06 (Depot run `rnfhvvd79g`, vscode, 20 reps, `bench/results/compare/2026-10-06-2420b7ed410b-64t.md`):
at each compiler's default, bun check (64 threads) 0.85 s / 2.86 GiB, tsrs (8 checkers) 1.51 s / 3.25 GiB, tsgo 7.0.2
9.58 s / 7.51 GiB, tsgo 7.1-dev 7.99 s / 6.85 GiB; at 64 threads tsrs 0.89 s / 4.97 GiB. bun check's 359 errors equal
tsgo 7.0.2's line for line; tsrs's 371 equal 7.1-dev's.
