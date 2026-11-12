# perf-pgo-fast-training: train the release PGO and BOLT profiles on `--fast` too

Question: the release binaries' PGO profile (`.github/scripts/pgo-train.sh`) and BOLT profile
(`.github/scripts/bolt.sh`) come from the conformance suite, the Go fourslash suite, `tsrs -p .` on xstate-main and webpack, and `tsrs --lint` on
the same two. None of it passes `--fast`. Since 0.20.0 the fast preset is opt-in (docs/USAGE.md "Performance
options"), and it runs code that default mode never runs: lazy member tables, lazy declaration-file members, the
locality assignment with work stealing and file splitting, canonical history, the inference-walk, union-front and
indexed-access caches, leaf-file regions. Without counts PGO treats those functions as cold and BOLT lays them out as
cold. In a `--fast` profile of a large private monorepo at 8 cores, fast-only functions are hot
(`get_ready_lazy_member_table_worker` alone 1.6% of self time; `create_lazy_member_table`, `get_lazy_declared_member`
and the lazy mapped tables close behind), and 23% of user cycles were front-end stalls. Does training `--fast` help?

Answer: yes. Adding `tsrs -p . --fast` on the same two projects to both trainings (PGO and BOLT) makes every `--fast`
check measured 0.4-2.3% cheaper in CPU time and cycles at 8 and 4 cores, with PGO alone and with PGO + BOLT, with
byte-identical output (22 of the 24 CPU-time cells have intervals that exclude 0; the other two are -0.4% and -1.0%).
Default mode moves by at most +0.3% CPU time. The release scripts now run it.
`--fast --checkers 1`, the lint run under `TSRS_FAST=1`, `--fast` at 8 cores and a second pair of projects add nothing
measurable on top, so they are not in the training.

Not settled by earlier work: notes/perf-build-level.md section 6 added vscode at 8 checkers to the training (-3%
instructions, cycles unchanged) and left it out, and notes/perf-binary-layout.md found more projects help only the
projects they train. Both trained a bigger project in the same mode, before the preset was split out of the default;
this one trains a mode whose code had no counts at all.

## Method

One m8g.4xlarge (16 Graviton4 vCPUs, 64 GiB, Ubuntu 24.04, kernel 7.0), source 221ee7bf, rustc 1.99.1 (LLVM 23.1.1),
`llvm-profdata` from rustup's `llvm-tools`, BOLT from the LLVM 23.1.1 ARM64 release tarball (release.yml's "Install
BOLT" step). Release.yml's commands for `aarch64-unknown-linux-gnu`:

- one instrumented build of tsrs, tsrs-test and fourslash_server (`-Cprofile-generate=<dir> -Clto=thin` in RUSTFLAGS,
  `--features tsrs_cli/noembed`, `CARGO_PROFILE_RELEASE_STRIP=none`); both arms merge raw profiles from these same
  binaries, so they differ only in which runs are merged;
- training as pgo-train.sh, each kind of run into its own directory;
- `llvm-profdata merge` per arm, then the final build with `-Cprofile-use=<profdata> -Clink-arg=-Wl,--emit-relocs`
  under the `release` profile;
- BOLT with `bolt.sh` and `BOLT_TSRS_ONLY=1`: main's script for A, this branch's for B (which adds the `--fast` runs);
  both passed bolt.sh's byte comparison.

Arms:

| arm | PGO training | BOLT training |
| --- | --- | --- |
| A | conformance suite, `tsrs -p .` on xstate-main and webpack, `tsrs --lint headless` on the same two | tsrs on xstate-main and webpack |
| B | A + `tsrs -p . --fast` on xstate-main and webpack | A's + the same two `--fast` runs |

Left out of both arms on the box: the Go fourslash suite (no Go on the box; it trains the language server, which
no measured workload runs), and so the fourslash and conformance gates of bolt.sh (`BOLT_TSRS_ONLY=1`; the byte
comparison of tsrs ran). The final build compiled only `-p tsrs_cli`; release's Linux build also compiles
tsrs-test and fourslash_server in the same invocation, which does not change tsrs's crate metadata
(notes/perf-pgo.md). The tsrs training runs were pinned to 4 cores (`taskset -c 0-3`), because release's Linux runners
have 4 vCPUs and the fast preset's checker count follows the CPU count; the conformance suite ran on 16.

Measurement: 15 alternating pairs per workload (A and B alternate, order flipped every pair) with the bench box
runner, `perf stat` instructions and cycles, `/usr/bin/time` peak RSS, `--noEmit --incremental false --pretty false`,
under `taskset -c 0-7` and then `taskset -c 0-3`. Intervals are ~95% intervals of the median paired ratio; `*` marks an
interval that excludes 0. Workloads: `--fast` on the large private monorepo (relative numbers only), on
vscode, mikro-orm, mui-docs and supabase-studio (not trained) and webpack (trained), and default mode on vscode and
mui-docs. stdout and exit code of A and B were byte-identical on all eight workloads at both core counts, PGO-only and
BOLT binaries alike (32 comparisons).

## PGO alone (B vs A)

CPU time (user + sys) per workload; A's median for the public projects, B vs A as the median paired change. Wall,
cycles and instructions are in the same direction; peak RSS changes by under 1% everywhere.

| workload | 8 cores A cpu | cpu | wall | cycles | instructions | 4 cores A cpu | cpu | wall | cycles | instructions |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| fast private monorepo | | -1.5% [-1.7, -1.0] * | -1.4% * | -1.6% * | -1.4% * | | -0.8% [-1.3, -0.7] * | -0.7% * | -0.9% * | -1.1% * |
| fast vscode | 12.33 s | -0.5% [-1.1, -0.3] * | -0.9% * | -0.6% * | -0.7% * | 11.88 s | -0.4% [-0.8, +0.4] | -0.1% | -0.5% | -0.7% * |
| fast mikro-orm | 7.89 s | -1.7% [-2.2, -0.9] * | -1.4% * | -1.8% * | -1.0% * | 6.66 s | -2.0% [-2.5, -1.6] * | -1.8% * | -2.0% * | -0.9% * |
| fast mui-docs | 8.16 s | -1.6% [-2.1, -0.0] * | -2.5% | -1.6% * | -0.8% * | 5.75 s | -1.6% [-2.1, -1.4] * | -1.9% * | -1.7% * | -0.8% * |
| fast supabase-studio | 7.91 s | -1.2% [-1.6, -1.0] * | -0.8% * | -1.2% * | -0.6% * | 6.69 s | -1.1% [-1.5, -1.0] * | -1.0% * | -1.2% * | -0.5% * |
| fast webpack (trained) | 1.62 s | -1.4% [-1.8, -0.4] * | -1.7% * | -1.6% * | -0.9% * | 1.41 s | -0.8% [-1.3, -0.3] * | -0.6% | -0.8% * | -0.7% * |
| default vscode | 12.51 s | +0.3% [+0.1, +1.3] * | +0.3% | +0.3% * | -0.3% * | 12.45 s | +0.1% [-0.2, +0.2] | -0.2% | -0.0% | -0.3% * |
| default mui-docs | 6.51 s | -0.2% [-0.8, +0.1] | -0.5% | -0.2% | +0.5% * | 6.28 s | -0.4% [-0.9, -0.0] * | -0.3% | -0.4% | +0.5% * |

## PGO + BOLT (B vs A, both BOLT-optimized)

| workload | 8 cores A cpu | cpu | wall | cycles | instructions | 4 cores A cpu | cpu | wall | cycles | instructions |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| fast private monorepo | | -1.0% [-1.5, +0.0] | -0.9% | -1.0% * | -1.1% * | | -1.8% [-1.9, -1.6] * | -1.8% * | -1.8% * | -1.3% * |
| fast vscode | 12.99 s | -1.0% [-1.3, -0.9] * | -0.9% * | -1.1% * | -0.8% * | 11.36 s | -0.8% [-1.0, -0.5] * | -1.1% * | -0.8% * | -0.8% * |
| fast mikro-orm | 8.08 s | -2.2% [-2.4, -2.0] * | -2.2% * | -2.2% * | -1.2% * | 6.57 s | -2.3% [-2.5, -2.1] * | -2.3% * | -2.3% * | -1.1% * |
| fast mui-docs | 8.41 s | -2.1% [-2.5, -1.8] * | -2.1% * | -2.2% * | -1.1% * | 5.72 s | -2.1% [-2.3, -1.8] * | -2.0% * | -2.2% * | -1.0% * |
| fast supabase-studio | 8.20 s | -1.6% [-1.7, -1.5] * | -1.4% * | -1.6% * | -0.7% * | 6.61 s | -1.5% [-1.9, -1.3] * | -1.5% * | -1.6% * | -0.6% * |
| fast webpack (trained) | 1.66 s | -1.4% [-1.6, -0.8] * | -0.9% | -1.2% * | -0.8% * | 1.39 s | -0.8% [-1.6, -0.6] * | -1.3% * | -0.8% * | -0.8% * |
| default vscode | 13.15 s | +0.3% [+0.1, +0.5] * | -0.0% | +0.3% * | -0.4% * | 12.12 s | +0.3% [-0.1, +0.4] | +0.3% | +0.3% | -0.4% * |
| default mui-docs | 6.66 s | +0.3% [-0.2, +0.9] | +0.1% | +0.2% | +0.4% * | 6.17 s | +0.0% [-0.1, +0.2] | +0.0% | +0.1% | +0.4% * |

Absolute times in the two tables come from different runs on the same box and are not comparable between tables
(the bench box rules: only paired ratios hold).

Default mode: its counts are the same in both profiles, but the merged profile's hot set grows. The largest change is
+0.3% CPU on vscode, inside +0.5% and with fewer instructions; mui-docs retires +0.4-0.5% more instructions with CPU
time unchanged. Peak RSS moves by under 1% in every cell of both tables.

## What the profile had

`llvm-profdata show --counts --function=<name>` on the two merged profiles: in A every block of
`get_ready_lazy_member_table_worker`, `create_lazy_member_table` and `get_lazy_declared_member` has count 0 (the
conformance suite and the default-mode runs never reach them); in B they have counts in the 10^4-10^6 range per
block. `llvm-profdata show --covered` lists 8,749 executed functions in A and 8,948 in B.

Side check that BOLT itself still pays on this box (7 pairs, 8 cores, each PGO binary against its own BOLT-optimized
version): -0.8% to -1.9% CPU for A and -0.8% to -2.5% for B on every workload, default mode included, so the
BOLT comparison above is between two binaries that both gain from BOLT. Absolute times drift by up to ~5% between
runs on one box, which is why only paired ratios are compared.

## Counters (private monorepo, `--fast`, 8 cores, 5 alternating rounds)

`perf stat -e cycles:u,instructions:u,stall_frontend,stall_backend,br_mis_pred_retired,l1i_cache_refill,itlb_walk`
(seven events on the PMU's counters, so they are multiplexed and scaled). Shares are of user cycles.

| event | PGO: B vs A | PGO: A / B share of cycles | PGO + BOLT: B vs A | PGO + BOLT: A / B share |
| --- | ---: | ---: | ---: | ---: |
| cycles:u | -0.8% | | -2.0% | |
| instructions:u | -1.3% | IPC 2.14 / 2.13 | -1.6% | IPC 2.25 / 2.26 |
| stall_frontend | -5.4% | 18.5% / 17.6% | -3.9% | 17.6% / 17.2% |
| stall_backend | +3.7% | 34.5% / 35.8% | -0.5% | 32.9% / 33.5% |
| br_mis_pred_retired | +0.2% | | +0.5% | |
| l1i_cache_refill | -2.1% | | -7.3% | |

The gain is fetch: fewer front-end stalls and instruction-cache refills, branch mispredictions unchanged. With
PGO alone part of it goes to more backend stalls; the 15-pair cycle numbers above are the result that counts.

## Other training candidates (5 pairs, `--fast` workloads at 8 cores, each against B, PGO only)

| added to B | cpu | cycles | verdict |
| --- | --- | --- | --- |
| `--fast --checkers 1` on xstate-main and webpack | -0.0% to +1.8%, no interval excludes 0 | same | nothing |
| `TSRS_FAST=1 tsrs --lint headless` on both | +0.5% / +0.6% on mikro-orm / webpack (excluding 0), rest inside noise | same | nothing |
| `--fast` on t3code-server and typeorm | +0.9% on supabase-studio (excluding 0), rest inside noise | same | nothing |
| `--fast` at 8 cores (8 checkers) on both | -1.0% on mui-docs, rest inside noise | same | nothing |

Some of them retire 0.3-1.3% fewer instructions on one or two projects, but cycles and CPU time do not follow. B as
it is covers the fast-only code; more runs of the same paths only re-weight it. The lint run under `TSRS_FAST=1` was
judged on checks; lint itself was not measured.

## Changes

- `.github/scripts/pgo-train.sh`: `tsrs -p . --noEmit --incremental false --pretty false --fast` on xstate-main and
  webpack after the default-mode runs; the final check requires nonempty `fast-xstate-main` and `fast-webpack`
  profiles.
- `.github/scripts/bolt.sh`: the instrumented tsrs also runs `--fast` on both projects (bolt.sh's existing check that
  tsrs left a profile covers them).
- `.github/scripts/pgo-train-windows.ps1`: Windows has no bench projects in its training, so it runs the same CLI
  fixture (`testdata/split-check/declaration-errors`) again with `--fast` into `fast-cli-*.profraw` and checks that
  profile. That covers the preset's scheduling and lazy-member code on a tiny program only (leaf-file regions are
  Unix-only); it was not measured.

Cost: two runs of under a second each per training on the Linux and macOS runners, and two more under BOLT.

## Caveats

- Linux aarch64 only (Graviton4). macOS and Windows get the same training change without a measurement here; x86_64
  Linux not measured either.
- Go fourslash was left out of both arms. It adds language-server counts to both profiles equally, so it can shift
  the hot set a little, not which arm wins.
- The `--fast` checker count is the CPU count up to 16: training at 4 cores trains 4 checkers. At 8 cores the gain
  holds (above), and 8-core training added nothing.

## Reproducing

```sh
# instrumented build (release.yml), once
RUSTFLAGS="-Cprofile-generate=$PWD/raw -Clto=thin" CARGO_TARGET_DIR=inst CARGO_PROFILE_RELEASE_STRIP=none \
  cargo build --release --locked -p tsrs_cli -p tsrs_testrunner -p tsrs_fourslash_server --bins \
  --features tsrs_cli/noembed --target aarch64-unknown-linux-gnu
# training: .github/scripts/pgo-train.sh (A: main's, B: this branch's); merge, final build with
# -Cprofile-use=<profdata> -Clink-arg=-Wl,--emit-relocs, then
BOLT_TSRS_ONLY=1 TSRS_LIB_PATH=$PWD/crates/tsrs_vfs/libs .github/scripts/bolt.sh <dist> <work> <bench work>
# measure: tsrs -p <project> --noEmit --incremental false --pretty false --fast under taskset -c 0-7 / 0-3
```
