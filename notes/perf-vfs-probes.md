# Front-end allocations, round three: VFS probes and package.json lookups

Measured 2026-10-24/25 at `a0db53ea` (main after #769) on an 18-core Apple Silicon machine shared with two other
agents' builds, following AGENTS.md "Performance and memory changes" (profile first, attribute, smallest change,
before/after numbers). The two sites are the first two of notes/perf-reparse-cloner.md's "Next" list, from that
round's allocation profile.

## Question

1. The VFS probes (`Common::file_exists`, `directory_exists`, `stat`) were 11.0% of cal-diy's front-end
   allocations (about 347,000 per run), about five avoidable heap allocations per probe before the system call:
   `split_path` normalized the path into a new `String` even when it was already normal (every path the
   resolver probes is a combined, normalized path) and copied the root and the rest into two more; `root_for`
   boxed a `DirFS` (`Box<dyn IoFS>`) holding its own copy of the root. Then `DirFS::file_type` joined root and
   rest into the path the system call takes (one allocation that stays: `fs::metadata` needs the whole path; its
   `CString` is a stack buffer below 384 bytes).
2. `InfoCache::get` (7.7% of cal-diy's front end with `get_package_json_info`) built a `Path` for every lookup,
   an `Arc<str>` plus the normalized and lowercase copy on case-insensitive hosts, and `get_package_json_info`
   joined `package.json` onto the directory into a new `String` first. Most lookups hit: the resolver asks for
   the package.json of every ancestor directory of every candidate.

## Change

Probes (`tsrs_vfs`, `tsrs_wasm`):

- `RootForFn` (`Box<dyn Fn(&str) -> Option<Box<dyn IoFS>>>`) is gone. `Common<R: RootFor>` holds the root
  provider by value and `RootFor::with_root(root, f)` lends the root's io/fs view to `f` for one operation
  instead of returning it boxed: `OsRoots` lends `DirFS { dir: &str }` (os.DirFS), `SubRoots<T>` lends the
  `IoFS` itself for `/` and `.` and `SubFS { fsys: &T, dir: &str }` otherwise (fs.Sub), the wasm `HostRoots`
  lends `HostRoot { root: &str }`. The views borrow the root for the call, like the codebase's other single-call
  views (`WorkGroup<'a>`, `Scanner<'a>`); no arena data carries a lifetime.
- `split_path` returns a `SplitPath` that borrows the path when it is already normal
  (`tspath::normalize_path_cow`, now public) and hands out `root()` and `rest()` as slices; it copies only a
  path with a backslash, a `.` or `..` segment or a doubled separator, as `normalize_path` did.
- `IoVFS<T>` no longer needs the `Arc<T>` it shared with the root closure, nor `T: 'static`; `fsys()` reads the
  value from `SubRoots`. The `IoFS for Arc<T>` forwarding impl only served that closure and is removed.

Probe semantics are unchanged: the same normalization (`normalize_path_cow` is `normalize_path` without the
copy), the same root split (`get_encoded_root_length` on the normalized path: `c:/`, `//server/share/` and URL
roots split as before), the same trailing-separator removal and `.` for an empty rest, the same `valid_path`
and URL handling in `SubRoots`, the same `DirFS::join` (separator and `from_slash` on Windows) and the same
`metadata_file_type` / `statx` call.

package.json lookups (`tsrs_core`, `tsrs_module`):

- `tspath::to_path_cow` is `to_path`'s text as a `Cow`: borrowed when the file name is the key as it is (absolute
  and already normal on a case-sensitive host), owned otherwise; `to_path` wraps it in the `Arc`. Same branches,
  same allocations for `to_path`'s callers (an `Arc<str>` from a `String` copied the bytes already).
- `InfoCache::get` hashes that text against the `Path` keys (`SyncMap::load` by a borrowed key, as the
  cached-VFS maps do with `str` for `String`), so a hit allocates nothing on a case-sensitive host and one
  lowercase copy on macOS and Windows.
- `get_package_json_info` builds the package.json path in the resolver's thread-local scratch string
  (`scratch::with_path_buffer`, a third `PathBuffer` kind next to the extension and suffix buffers) through the
  new `tspath::combine_paths_into`; `combine_paths` is that function on a fresh `String`. The miss path reads
  and parses the file from the same buffer and stores an owned key as before.

## What is different from notes/perf-vfs-lightweight-metadata.md

That note (rejected under the old thresholds, 2026-10-11) changed what a probe asks the kernel: `file_type`
instead of a full `stat`, `statx(STATX_TYPE)` on Linux, no `FileInfo` for existence checks; it is on main as
`IoFS::file_type` and this change keeps it. What is cut here is the path and root plumbing before the system call
and the cache keys after it, which that note did not touch. The system-call count and the probe results are the
same.

## Measurement

Clean fat-LTO `--release` builds (stock profile), `/usr/bin/time -l` instructions retired (kernel included),
`--noEmit --incremental false --pretty false --singleThreaded`, `RAYON_NUM_THREADS=1`, one warm-up then
interleaved base/candidate pairs, medians of the paired changes. Peak RSS with the default checker count, same
recipe. The machine carried two other agents' builds throughout (load average 65-99 on 18 cores), so the pairs
spread about +-1.5% on instructions and +-1.5% on default-checker RSS; the expected instruction change is about
0.1-0.2% (half a million fewer malloc/free pairs on cal-diy), inside that spread, and the counts below are the
exact result. Diagnostics (exit code and output bytes) were identical in every run of every series.

Both changes against base, 11 pairs:

| project | instructions base | candidate | change | pairs lower | single-threaded RSS | default-checker peak RSS |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| webpack | 11.048 G | 10.993 G | **-0.17%** | 8 of 11 | 315.75 -> 315.80 MB (+0.02%) | -0.07% (3 pairs: -0.07, -0.11, +2.63) |
| typeorm | 8.238 G | 8.225 G | **-0.09%** | 8 of 11 | 201.79 -> 201.80 MB (+0.01%) | -0.23% (3 pairs: -0.30, -0.23, -0.07) |
| cal-diy | 32.846 G | 32.755 G | **-0.14%** | 7 of 11 | 807.35 -> 811.24 MB (**+0.48%**) | see below |

The probe change alone, two earlier series (7 and 15 pairs): webpack +0.01% / -0.13% (2 of 7, 11 of 15 lower),
typeorm -0.21% / -0.64% (4 of 7, 12 of 15), cal-diy -0.11% / +0.03% (5 of 7, 7 of 15); default-checker RSS
webpack +1.3% (3 pairs), typeorm +0.2% (3), cal-diy +0.6% (3) and -0.6% (4): noise around zero.

cal-diy's default-checker peak RSS, both changes, 13 pairs (5 in the series above, 8 more): median **+0.2%**, 5 of
13 pairs lower, the pairs from -2.4% to +3.3% (base runs alone range 1898-1955 MB), medians 1921 -> 1912 MB over
the last 8; the probe change against both changes, 6 pairs: -0.4%. Noise around zero, as expected: nothing
retained changes (the single-threaded case is below).

Heap allocations per run, exact counts from a counting `GlobalAlloc` wrapper around mimalloc in the CLI (not
committed; `alloc` + `alloc_zeroed` + `realloc` calls, single-threaded; every run of a build repeats its count):

| project | full check, base | probes | + package.json | change | `--noCheck`, base | probes | + package.json | change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| webpack | 3,370,378 | 3,326,743 | 3,310,689 | **-1.8%** | 658,645 | 615,010 | 598,958 | **-9.1%** |
| typeorm | 2,830,075 | 2,743,935 | 2,659,878 | **-6.0%** | 4,986,667 | 4,900,397 | 4,816,322 | -3.4% |
| cal-diy | 10,593,826 | 10,304,294 | 10,059,321 | **-5.0%** | 3,084,291 | 2,795,394 | 2,550,479 | **-17.3%** |

The probe change is 289,532 fewer allocations on cal-diy (the profile's estimate was 347,000 for the whole site,
of which the joined path stays), the package.json change 244,973; the `realloc` count rises by one (the scratch
string growing once per thread).

### cal-diy's single-threaded RSS

The +3.9 MB on cal-diy repeats to the page in every pair (807.35 -> 811.24 MB) and is there with the probe
change alone; webpack and typeorm move by 16-48 KB. `TSRS_MEM_SPLIT=1` says what it is: at `parse end` the heap's
live blocks are 104.1 MiB in both builds and the thread arenas identical (232.8 MiB used), but 126.2 MiB of the
allocator's page areas are resident against 122.8 (1282 pages against 1280); at `check end` live blocks are
257.9 MiB in both and resident 257.0 against 253.6. `MIMALLOC_PURGE_DELAY=0`, `MIMALLOC_PURGE_DELAY=-1` and a
forced `mi_collect(true)` after the check (`TSRS_MEM_SPLIT=purge`) leave both numbers where they are, so it is
not freed memory awaiting a purge; the allocator touches two more pages for the same live data, which points at
page selection (mimalloc retires and reuses a page by counting allocations, and the front end now makes about
290,000 fewer). No data is retained: the change frees nothing that lived and keeps nothing new.

Exactness: `tools/regressions.sh` passes its 50 cases with the candidate binary; `cargo test` for `tsrs_core`,
`tsrs_vfs`, `tsrs_module` and `tsrs_wasm` (native, over the in-memory host); `cargo check -p tsrs_wasm --target
wasm32-wasip1` for the module's host implementation; clippy with warnings denied, `cargo fmt --check` and
`tools/lint/source.mjs`.

The Linux `bench/count.mjs` gate on the pull request is the landing number; these are the screening numbers.

## Not done here

- The remaining allocation per probe, `DirFS::join`: the system call needs the whole path. On POSIX the root is
  `/` and the joined path is the normalized path without its trailing separator, so the normalized path itself
  could feed `fs::metadata` for the OS root; it needs the view to see the whole path, which io/fs names do not
  carry.
- The lowercase key copy per package.json lookup on case-insensitive hosts (macOS, Windows): a case-folding hash
  and comparison on the `SyncMap` would remove it; the Linux gate does not see it.
- JSDoc tag and comment-range vectors and the per-node `Parser` of `parse_jsdoc_for_node`, the third item of
  notes/perf-reparse-cloner.md's list.
