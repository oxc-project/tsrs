# perf-musl-pgo: PGO for the musl release binaries

Question: the npm packages `@tsrs/linux-x64-musl` and `@tsrs/linux-arm64-musl` (what an Alpine container installs)
were built with `cargo zigbuild --release` and no profile, while the glibc, macOS and Windows binaries get PGO and the
glibc ones also BOLT (notes/perf-pgo.md, notes/perf-build-level.md). How much slower is the musl binary, and what is the
cheapest sound way to give it a profile?

Answer: the published 0.25.0 arm64 musl binary takes 6-22% more wall time and 9-22% more CPU than the glibc one on
the five public projects, with 10-21% more instructions and the same peak memory. Training on musl builds
(the glibc job's workload without the Go fourslash suite, plus `--fast` runs of the two bench projects) removes
most of that: -4 to -14% wall and -6.5 to -14% CPU against the unprofiled musl build, leaving it 0.5-3.3% behind the
glibc binary, which also has BOLT. Reusing the glibc job's profile does not work: 88% of the functions find no
profile data, because cargo's crate hash, which is part of every symbol name, includes the target. Release builds now
train on musl (`build-musl` in `.github/workflows/release.yml`, `pgo-train.sh --musl`). BOLT also works on the
static binary (-1.5 to -2.8% wall in a short run) but needs a linker other than zig and periodic profile dumps, so it is
left for a separate change.

## Setup

One m8g.4xlarge (Graviton4, 16 vCPUs, 64 GiB, Ubuntu 24.04, glibc 2.39), source 221ee7bf (main, crates 0.25.0),
rustc 1.99.1 (LLVM 23.1.1). Every arm in one run, interleaved: each round runs all six binaries once, in an order
rotated every round and reversed on odd rounds, 12 rounds per workload; deltas are medians of the per-round paired
ratios. `taskset -c 0-7` (8 cores) and `taskset -c 0-3` (4 cores), `-p <project> --noEmit --incremental false --pretty
false`, with and without `--fast`. CPU is user + system, instructions and cycles are `perf stat` user-space counts,
peak RSS is `/usr/bin/time`.

Arms:

- `pubG`, `pubM`: `@tsrs/linux-arm64-gnu` and `@tsrs/linux-arm64-musl` 0.25.0-ts7.1.0-beta from npm, as published
  (glibc: PGO + BOLT; musl: no profile).
- `mA`: musl, built on the box exactly as `build-musl` did (cargo-zigbuild 0.24.0, zig 0.13.0, `--release --locked -p
  tsrs_cli --features tsrs_cli/noembed --target aarch64-unknown-linux-musl`, `TSRS_LIB_PATH` set).
- `mR`: `mA` + `-Cprofile-use` with a glibc-trained profile of the same source: the `build` job's instrumented build
  and `pgo-train.sh` workload on this box (conformance suite, the two bench projects, `--lint`; no Go fourslash suite,
  the box has no Go), `CARGO_PROFILE_RELEASE_STRIP=none` as in the glibc job so the strip setting matches, then
  stripped.
- `mS`: `mA` + a profile from a musl instrumented build of tsrs alone, trained on xstate-main and webpack in default
  mode, with `--fast` and with `--lint headless` (4 cores, as the release runners have). Needs only `tsrs`.
- `mF`: `mA` + a profile from a musl instrumented build of tsrs and tsrs-test, trained on the conformance suite plus
  `mS`'s workload. This is what `build-musl` now does (`pgo-train.sh --musl`).

`mA` against `pubM` is the check that the box reproduces the release build: instructions are identical (0.0% on every
workload), CPU 0-2.6% lower.

## Results

CPU (user + system), median paired change. Columns: the published gap, then each candidate against the unprofiled
musl build, then the remaining distance to the published glibc binary.

| CPU | pubG → pubM | mA → mR | mA → mS | mA → mF | mS → mF | pubG → mS | pubG → mF |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| fast vscode, 8 cores | +19.6% | +1.8% | −10.9% | −13.0% | −2.2% | +4.0% | +1.9% |
| fast mikro-orm, 8 cores | +10.6% | +1.0% | −5.2% | −7.9% | −2.9% | +3.4% | +0.5% |
| fast mui-docs, 8 cores | +8.8% | +2.0% | −6.0% | −6.5% | −0.5% | +1.8% | +1.4% |
| fast supabase-studio, 8 cores | +13.1% | +1.3% | −6.7% | −8.6% | −2.0% | +4.2% | +2.1% |
| fast webpack, 8 cores | +17.5% | +1.5% | −10.3% | −12.2% | −2.1% | +3.3% | +1.2% |
| default vscode, 8 cores | +21.0% | +1.6% | −11.4% | −13.4% | −2.2% | +4.9% | +2.3% |
| default mikro-orm, 8 cores | +14.6% | +0.5% | −7.4% | −10.3% | −3.5% | +4.8% | +1.9% |
| default mui-docs, 8 cores | +11.6% | +2.5% | −6.3% | −7.0% | −0.9% | +3.8% | +2.9% |
| default supabase-studio, 8 cores | +15.7% | +1.7% | −8.0% | −9.8% | −2.1% | +5.0% | +2.8% |
| default webpack, 8 cores | +20.5% | +1.6% | −11.7% | −13.0% | −1.7% | +3.5% | +2.1% |
| fast vscode, 4 cores | +20.3% | +2.3% | −11.2% | −13.2% | −1.7% | +3.4% | +1.6% |
| fast mikro-orm, 4 cores | +11.6% | +1.2% | −6.1% | −9.1% | −3.1% | +3.2% | +0.6% |
| fast mui-docs, 4 cores | +10.5% | +1.6% | −7.0% | −7.3% | −0.3% | +2.0% | +1.9% |
| fast supabase-studio, 4 cores | +13.8% | +1.3% | −7.4% | −9.4% | −2.2% | +4.2% | +1.9% |
| fast webpack, 4 cores | +18.8% | +1.9% | −11.4% | −12.6% | −1.6% | +2.8% | +1.5% |
| default vscode, 4 cores | +21.9% | +1.6% | −12.3% | −14.0% | −2.1% | +4.4% | +2.2% |
| default mikro-orm, 4 cores | +14.1% | +0.6% | −7.6% | −9.8% | −2.3% | +4.5% | +2.3% |
| default mui-docs, 4 cores | +11.7% | +2.4% | −6.3% | −7.0% | −0.8% | +4.0% | +3.2% |
| default supabase-studio, 4 cores | +15.5% | +1.4% | −8.1% | −10.1% | −2.1% | +4.9% | +2.6% |
| default webpack, 4 cores | +20.1% | +1.9% | −11.8% | −13.4% | −1.8% | +4.1% | +2.2% |

Wall time:

| wall | pubG → pubM | mA → mR | mA → mS | mA → mF | mS → mF | pubG → mS | pubG → mF |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| fast vscode, 8 cores | +18.9% | +1.7% | −10.0% | −12.1% | −1.8% | +4.4% | +2.3% |
| fast mikro-orm, 8 cores | +10.6% | +0.6% | −5.3% | −8.1% | −2.2% | +3.9% | +0.9% |
| fast mui-docs, 8 cores | +5.9% | +2.5% | −4.4% | −4.3% | −0.1% | +2.1% | +2.2% |
| fast supabase-studio, 8 cores | +12.3% | +1.0% | −5.9% | −7.4% | −1.5% | +4.4% | +2.9% |
| fast webpack, 8 cores | +14.6% | +3.1% | −7.9% | −11.0% | −3.3% | +3.6% | +0.9% |
| default vscode, 8 cores | +20.7% | +1.3% | −11.9% | −13.7% | −1.8% | +4.0% | +2.0% |
| default mikro-orm, 8 cores | +14.4% | +0.4% | −7.4% | −9.8% | −3.6% | +5.1% | +2.3% |
| default mui-docs, 8 cores | +10.2% | +2.2% | −5.2% | −6.5% | −1.1% | +3.6% | +2.7% |
| default supabase-studio, 8 cores | +14.1% | +2.1% | −7.3% | −8.9% | −1.8% | +4.5% | +2.7% |
| default webpack, 8 cores | +19.1% | +1.6% | −11.4% | −13.2% | −2.1% | +3.9% | +1.9% |
| fast vscode, 4 cores | +19.6% | +2.4% | −10.9% | −12.9% | −2.0% | +3.8% | +1.5% |
| fast mikro-orm, 4 cores | +11.9% | +1.3% | −6.1% | −8.5% | −2.9% | +3.1% | +0.7% |
| fast mui-docs, 4 cores | +10.6% | +1.1% | −6.8% | −6.9% | −0.2% | +2.3% | +1.9% |
| fast supabase-studio, 4 cores | +13.3% | +1.5% | −7.0% | −8.9% | −2.1% | +4.1% | +1.6% |
| fast webpack, 4 cores | +15.4% | +2.1% | −10.1% | −10.6% | −1.6% | +2.5% | +0.5% |
| default vscode, 4 cores | +21.8% | +1.1% | −12.9% | −14.4% | −1.6% | +3.7% | +1.9% |
| default mikro-orm, 4 cores | +14.2% | +0.6% | −7.3% | −9.7% | −2.7% | +4.6% | +2.4% |
| default mui-docs, 4 cores | +11.2% | +2.4% | −5.8% | −6.4% | −0.7% | +4.0% | +3.3% |
| default supabase-studio, 4 cores | +14.3% | +1.5% | −7.6% | −9.3% | −1.8% | +4.4% | +2.5% |
| default webpack, 4 cores | +18.8% | +0.6% | −11.6% | −13.9% | −3.0% | +4.1% | +2.1% |

Instructions (user space; the same at 8 and 4 cores within 0.5 points, so 8 cores only):

| instructions | pubG → pubM | mA → mR | mA → mS | mA → mF | mS → mF | pubG → mS | pubG → mF |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| fast vscode | +20.4% | −0.6% | −17.7% | −16.1% | +2.0% | −0.9% | +1.0% |
| fast mikro-orm | +11.8% | +0.1% | −11.4% | −10.6% | +0.9% | −0.9% | −0.0% |
| fast mui-docs | +9.6% | +1.7% | −8.7% | −7.2% | +1.7% | +0.3% | +2.0% |
| fast supabase-studio | +13.1% | +0.7% | −11.0% | −9.9% | +1.2% | +0.7% | +2.0% |
| fast webpack | +18.6% | −0.3% | −16.9% | −15.5% | +1.6% | −1.3% | +0.4% |
| default vscode | +20.1% | −0.8% | −17.9% | −16.3% | +2.0% | −1.4% | +0.5% |
| default mikro-orm | +14.1% | −0.4% | −12.5% | −12.1% | +0.5% | −0.2% | +0.3% |
| default mui-docs | +11.3% | +2.2% | −8.1% | −7.3% | +0.8% | +2.3% | +3.1% |
| default supabase-studio | +13.9% | +0.8% | −11.6% | −10.7% | +1.0% | +0.7% | +1.8% |
| default webpack | +18.7% | −0.1% | −16.6% | −15.3% | +1.5% | −1.0% | +0.5% |

A large private monorepo, `--fast` only, the same six arms, 8 rounds (relative numbers only; not used for training):

| large private monorepo, `--fast` | pubG → pubM | mA → mR | mA → mS | mA → mF | mS → mF | pubG → mS | pubG → mF |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| wall, 8 cores | +11.6% | +0.6% | −6.0% | −8.4% | −2.3% | +3.4% | +1.0% |
| wall, 4 cores | +13.0% | +1.3% | −7.4% | −9.3% | −2.1% | +3.1% | +0.8% |
| CPU, 8 cores | +12.4% | +1.2% | −6.2% | −8.3% | −2.4% | +3.9% | +1.4% |
| CPU, 4 cores | +13.2% | +1.3% | −7.5% | −9.5% | −2.3% | +3.2% | +0.8% |
| instructions, 8 cores | +13.9% | +0.3% | −12.4% | −11.6% | +1.0% | −0.3% | +0.9% |
| instructions, 4 cores | +14.5% | +0.0% | −13.4% | −12.2% | +1.4% | −0.9% | +0.5% |
| peak RSS, 8 / 4 cores | −0.1% / +0.1% | −0.1% / +0.2% | +0.2% / +0.1% | +0.1% / +0.1% | −0.1% / −0.0% | +0.1% / +0.2% | +0.1% / +0.0% |

Peak RSS moves by at most 1.2% in any column (all arms allocate through mimalloc), mostly under 0.5%.

- The published gap is mostly the missing profile: instructions +10-21%, and `mF` brings them to within 0-3% of the
  glibc binary's. What is left in time (+0.5-3.3% wall) is about what BOLT gives the glibc binary
  (notes/perf-build-level.md: -1 to -4% wall); musl's own libc routines (mimalloc replaces only the allocator) may
  account for some of it, which was not separated here.
- `mS` (tsrs only) retires the fewest instructions, but `mF` (plus the conformance suite) is 0-3.6% faster in wall and
  CPU nearly everywhere (mui-docs ties). Presumably the suite profiles code the two bench projects reach rarely, so less
  of it is laid out as cold; that was not checked.
- `mR` is slightly slower than no profile at all (+0.4-3.1% wall).

## Why the glibc profile does not fit

PGO looks up a function's counts by its symbol name, and every Rust symbol name contains the crate's hash, which cargo
derives from `-C metadata`; cargo hashes the compile target into that metadata. The same crate gets a different hash for
each target: `tsrs_checker` is `Cs6ogqFrjbwtp` in the glibc profile and `Cs3T9zNTB7yrZ` in the musl build with the same
strip setting (`Cs8utJt9zQ7Ab` with the musl job's default `strip = "symbols"`, the 2026-10-09 problem in
notes/perf-pgo.md). With `-Cllvm-args=-pgo-warn-missing-function`, the `mR` build reports 27,438 functions without
profile data, 0 hash mismatches, out of about 31,200 in the musl binary (88%). The rest (code whose names carry no crate
hash, such as C-ABI functions) gets counts from a different program; that `mR` ends up slightly slower than no profile
is consistent with a few percent of the code being optimized as hot or cold on the wrong evidence. The `mS` and `mF` builds report 0 missing and 0 mismatched functions. The same cause made the arm64 profile
useless for macOS x86_64 (notes/perf-pgo.md, "macOS x86_64"). Renaming the profile's symbols from one target's hashes
to the other's might work but was not tried: training on musl costs a few minutes, and a rename would be a new piece of
tooling with its own failure modes.

## Building the instrumented musl binary

- rustc links the profile runtime with `-u __llvm_profile_runtime`, and cargo-zigbuild 0.24.0's `zig cc` (zig 0.13.0)
  takes the symbol name for an input file: `ld.lld: error: cannot open __llvm_profile_runtime`. The instrumented
  build therefore links through `.github/scripts/zigbuild-profile-linker.sh` (`-Clinker=`), which rewrites the pair
  to `-Wl,-u,__llvm_profile_runtime` and calls cargo-zigbuild's own wrapper (exported as
  `CARGO_TARGET_<TRIPLE>_LINKER`). The final build does not need it.
- The musl `rust-std` component ships `profiler_builtins`, and the static instrumented binaries write their
  `.profraw` at exit like the glibc ones (7.5 MB per tsrs run here). They run on the glibc runner as they are.
- The instrumented musl tsrs-test gives the same conformance result as the glibc one on this box (15,448 variants:
  13,696 pass, 2 codes, 2 fail, 0 crash, 0 timeout in both).
- mimalloc's CMake flags (`CMakeCache.txt`) contain no `-fprofile-*` in any musl build: cc does not forward rustc's
  profile flags to `zig cc` here, so unlike macOS the job needs no `CFLAGS` override.

Build times on the box (16 cores): unprofiled build 176 s; instrumented tsrs 76 s, tsrs + tsrs-test 91 s; training
(`mS` 6 s, `mF` 23 s with the suite on 16 cores); profile-use builds 174-209 s. The release runners have 4 cores, so
expect several times that; the bench projects' clone and install comes on top, as in the glibc job.

## Correctness

Stdout and exit code of all six binaries are byte-identical on vscode, mikro-orm, mui-docs, supabase-studio,
webpack and xstate-main, default and `--fast`, 8 cores, and on the large private monorepo in both modes (exit 0, no
errors). Every musl build passes the job's `tsrs --version` stamp check.

## Not exercised

- The release workflow itself: the changed `build-musl` job has not run. On the box, by hand and on aarch64 only:
  the instrumented build with the linker wrapper, the changed `pgo-train.sh --musl` (all seven workloads left
  nonempty profiles; the suite gave 13,696 passes as above), the job's "Merge profiles" commands (32,810 functions),
  the same commands on an empty `.profraw`, which fail as intended, and the job's final "Build and stage musl binary"
  step with that profile (profile check, build, `--version` stamp check; its output matches `mF` byte for byte on the
  six public projects in both modes). The `mF` binary measured above came from the same commands run through the
  experiment's own scripts. GitHub runner specifics (the actions' installs, the 4-core runners' times, network for the
  bench projects) are untested.
- x86_64 musl gets the same change unmeasured; only aarch64 could be measured here.
- BOLT on the musl binary: tried by hand (below), not part of the release change.

## BOLT on the static musl binary (not proposed)

BOLT can optimize the static musl binary, but not with the release job's toolchain as it is:

- BOLT needs the binary linked with `--emit-relocs`, and zig 0.13.0's `zig cc` rejects it (`error: unsupported
  linker arg: --emit-relocs`, and the same for `-q`). Linking with the host's `cc` (`-Clinker=cc
  -Clink-arg=-Wl,--emit-relocs`; GNU ld 2.42 with rustc's self-contained musl startup objects and libc) instead
  works: a static, non-PIE `EXEC` binary with relocations, built with `CARGO_PROFILE_RELEASE_STRIP=none` and a profile
  from a `strip=none` musl instrumented build of tsrs alone (0 functions without profile data).
- `llvm-bolt -instrument` refuses static binaries: "instrumentation of static binary currently does not support
  profile output on binary finalization, so it requires -instrumentation-sleep-time=N (N>0)". With
  `-instrumentation-sleep-time=1` the profile is dumped every second while the program runs, so xstate-main (under a
  second) left an empty profile; vscode, mikro-orm and webpack left 17-29 MB each.
- The optimized binary (`bolt.sh`'s flags plus `--drop-cortex-a53-843419-veneers`) prints byte-identical output to its
  input on the six public projects, default and `--fast`. Stripped size: 40.0 MB against 19.5 MB.

Six interleaved rounds, BOLT-optimized against its own input (vscode and webpack were both in the BOLT training set):

| | wall | CPU | instructions |
| --- | ---: | ---: | ---: |
| `--fast` vscode, 8 / 4 cores | −1.7% / −1.5% | −1.8% / −1.7% | −0.1% / −0.1% |
| default vscode, 8 / 4 cores | −2.3% / −2.5% | −2.6% / −2.9% | −0.1% / −0.1% |
| `--fast` webpack, 8 / 4 cores | −2.8% / −1.9% | −1.9% / −1.9% | +0.5% / +0.0% |
| default webpack, 8 / 4 cores | −1.7% / −1.5% | −1.4% / −1.4% | −0.0% / −0.0% |

That is about what BOLT gives the glibc binary, so it would close most of the remaining gap. Shipping it would mean
linking the release binary with something other than zig, a training run long enough for the periodic dumps (or
`perf`-sampled profiles), and the gates `bolt.sh` runs for glibc; left for a separate change.

## Reproduce

Download and unpack the published binaries with `npm pack @tsrs/linux-arm64-gnu@0.25.0-ts7.1.0-beta` (and
`...-musl...`). Install zig 0.13.0, `cargo install cargo-zigbuild --version 0.24.0 --locked`,
`rustup target add aarch64-unknown-linux-musl` and `rustup component add llvm-tools`. Then, from the repository root,
with `TSRS_LIB_PATH=$PWD/crates/tsrs_vfs/libs` and ts-ref checked out:

```sh
T=aarch64-unknown-linux-musl
RUSTFLAGS="-Cprofile-generate=/tmp/pgo/raw -Clto=thin -Clinker=$PWD/.github/scripts/zigbuild-profile-linker.sh" \
  CARGO_TARGET_DIR=/tmp/pgo/target cargo zigbuild --release --locked -p tsrs_cli -p tsrs_testrunner --bins \
  --features tsrs_cli/noembed --target $T
.github/scripts/pgo-train.sh --musl /tmp/pgo/target/$T/release /tmp/pgo/raw /tmp/pgo/bench
llvm-profdata merge -o /tmp/pgo/musl.profdata /tmp/pgo/raw/*.profraw
RUSTFLAGS="-Cprofile-use=/tmp/pgo/musl.profdata" cargo zigbuild --release --locked -p tsrs_cli \
  --features tsrs_cli/noembed --target $T
```

The interleaved runner was a six-arm variant of the bench box's `runner.mjs`.
