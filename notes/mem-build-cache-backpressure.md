# Build package-cache ownership and free-queue backpressure (#696)

Issue [#696](https://github.com/oxc-project/tsrs/issues/696) supplies a public generator with two independent memory
problems. Both reproduce against TypeScript Go at the pinned commit `3e9236361fa97a9e6c685f4833ac457ae2d8c50a`.
No private code is used.

## Causes and changes

`ProgramRegions::enter` creates unregistered loader regions and enters them with `Region::enter`, which does not
establish the TLS scratch context. The package cache lives in one of those regions. Once owner routing is active,
`enter_table_owner` cannot recognize that owner, even on its own thread, and escapes package entries into the
permanent thread arena. Its values do not run `Drop`: dependency maps of both winning and losing parses remain
allocated after their programs are freed. Parallel parsing amplifies this through first-writer races.

`InfoCache` now owns entry storage through `Arc<OnceLock<Region>>`. Clones still copy their lookup tables, as Go
does, and share ownership of the entry region. The last cache generation releases it. A worker reads and parses
outside the region lock, then rechecks the table inside the lock before allocating an entry. A losing owned parse
is dropped without ever being moved into an arena. The critical section only looks up and allocates; it cannot
wait on loader work. There is no global package-content cache or change to cache invalidation.

Separately, ordinary builds enqueued completed programs into the watch session's unbounded, single-worker free
queue. At sixteen builders a trace of 800 projects found 375 outstanding frees, 369 still outstanding at reporting.
The queue now permits four outstanding ordinary-build frees, including the executing job. A builder frees its
own completed program if those slots are occupied. Watch frees keep the existing FIFO path: programs must be freed
before their unreachable file regions.

`mem-build-free-programs.md` measured and rejected always freeing inline (4% wall cost at 64 builders on 8 cores).
The new evidence is the independently traced backlog and roughly 90% memory reduction below. The bounded fallback
keeps free/build overlap while there is capacity. The limit counts queued and executing jobs; it does not bound
active builders or the solution graph itself.

## Release measurements

2026-10-22, same 12-core macOS arm64 host, ordinary `cargo build --release --locked -p tsrs_cli` binaries,
before source `34cc751998bfe8ab6fbaa03d25bf6f008be9fc9b`. Peak RSS and retired instructions are from
`/usr/bin/time -l`. MB is decimal, G is billions of instructions. Each row is a before/after pair, one sample each;
parallel parse races affect both allocations and counts. No wall-time improvement is claimed.

The issue's generator makes N independent projects, each extending a base config with the same 40 declaration
files, strict checking, ES2022 libraries, bundler resolution and `noEmit`. The shared package has D dependencies.
Each file exports T recursive generic interfaces. Regenerate the solution for each N/D/T case, then run each
binary from that directory with `-b tsconfig.json --force --pretty false --builders B`.

Package workload, T=1, B=4, default four checkers per project:

| N | D | RSS before, MB | RSS after, MB | instructions before, G | instructions after, G |
| --- | --- | --- | --- | --- | --- |
| 250 | 0 | 134.3 | 129.6 | 16.218 | 16.142 |
| 250 | 2000 | 743.7 | 158.5 | 24.515 | 24.005 |
| 500 | 0 | 144.7 | 149.6 | 32.064 | 31.960 |
| 500 | 2000 | 1192.5 | 197.9 | 46.264 | 46.864 |
| 1000 | 2000 | 2479.0 | 236.4 | 96.213 | 95.358 |

Free-queue workload, D=0, T=200, default four checkers per project:

| N | B | RSS before, MB | RSS after, MB | instructions before, G | instructions after, G |
| --- | --- | --- | --- | --- | --- |
| 400 | 4 | 362.5 | 373.7 | 181.220 | 181.181 |
| 400 | 16 | 3749.8 | 635.5 | 178.439 | 178.111 |
| 800 | 4 | 413.9 | 402.1 | 362.029 | 361.830 |
| 800 | 16 | 7978.5 | 813.3 | 357.825 | 356.484 |

With `--singleThreaded` (also serializes building/loading):

| N | D | T | RSS before, MB | RSS after, MB | instructions before, G | instructions after, G |
| --- | --- | --- | --- | --- | --- | --- |
| 250 | 2000 | 1 | 93.2 | 33.7 | 10.837 | 10.857 |
| 500 | 2000 | 1 | 160.4 | 41.9 | 21.585 | 21.521 |
| 400 | 0 | 200 | 87.0 | 87.0 | 169.829 | 169.823 |
| 800 | 0 | 200 | 106.1 | 105.4 | 339.978 | 340.826 |

Doubling dependency-heavy projects from 500 to 1000 changes RSS by 2.08x before, 1.19x after. Doubling the
sixteen-builder queue workload changes RSS by 2.13x before, 1.28x after. The solution graph still grows with N.
At N=1000 the package workload saves 90.5% of peak RSS; at N=800/B=16 the queue workload saves 89.8%.
The small zero-dependency controls vary by several MB; the N=500 package case uses 4.8 MB more after the fix.
The parallel package workload at N=500/D=2000 uses 1.3% more instructions in this pair (an earlier pair used
3.5% fewer). These measurements support the memory claim, not an instruction-count optimization claim.

A single-project control uses 400 files, T=50, D=2000 and `-p p0/tsconfig.json --noEmit --incremental false
--pretty false`: default checkers use 113.8 -> 113.3 MB and 1.792 -> 1.793 G instructions; `--singleThreaded`
uses 91.2 -> 91.2 MB and 1.663 -> 1.660 G.

## Checks and lifetime limits

- New resolver regression fails on the original code: cache entries have no reclaimable owner. It passes with
  the fix, including after the unregistered loader region is freed; entry storage is released after the final clone.
- Eight concurrent inserts allocate one entry and drop all owned parses. A blocked free-worker test verifies inline
  overflow and capacity reuse. Existing per-generation cache-isolation and package.json `types` edit/watch tests pass.
- Module, compiler and execute unit tests pass (39, 29 and 15 respectively, one module oracle-corpus test ignored).
  Module/execute tests also pass with `TSRS_ARENA_POISON=1`. The full generated execute scenario corpus is unavailable.
- Both real CLI `build_free_programs` poison tests pass. Smaller versions of both public workloads match pinned Go
  with sixteen builders, fixed tsrs running under arena poison. All measured invocations exit 0 with no output.
- Conformance passes in Go-history and canonical modes: 13,696 error, 12,998 type and 12,998 symbol baselines,
  no crashes/timeouts, no previously passing test lost. All 43 CLI regressions pass. Full Clippy and source checks pass.

The storage lifetime is the cache family, so a still-live generation retains all entries allocated by that family.
This does not implement individual entry collection or bound watch-mode frees. The auto-import registry parses
its long-lived package entries independently under its own region; source-file metadata copies package strings.
