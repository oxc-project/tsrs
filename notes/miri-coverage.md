# miri-coverage: the handle and region code under Miri

Issue #646 (sound lifetimes) and #379 (Miri failures) both ask what Miri can and cannot tell us about `tsrs_core`'s
memory model. notes/fix-arena-recycle-uaf.md ran Miri over the arena free-list tests only; this note widens the run to
every lib test of the crate, adds the owner-and-handle cases the language server relies on, and records what each
configuration found. Taken at main 357d1d3 (2026-10-21) plus this change, on an Apple Silicon machine (18 cores, Miri
interprets one thread at a time) with Miri from `nightly-2026-09-30` (CI pins `nightly-2026-10-04`; every run below
used the local nightly), always with `--features plain-ptrs`.

## In plain words

docs/PORTING.md ("Memory model") gives every arena handle a `'static` view: `P::get` returns `&'static T`, a region
frees everything inside it when its last `Region` handle drops, and nothing in the types connects the two. The
convention holds by code review and by the poison runs in docs/DEBUGGING.md. Miri is the one tool that sees the
convention being broken at run time (a read through a handle of a freed region, a write through a protected borrow,
a retag of a dead reference), and it only sees it in a build where arena memory is ordinary memory: with compressed
handles the arena is a `PROT_NONE` reservation that Miri cannot model, so every run here is `plain-ptrs`, where a
slab is a system allocation and a freed region's memory is really gone.

Two kinds of test were added:

- Positive tests of the region ownership rules (`arena::tests`, run natively in both pointer modes and under Miri):
  a rebuild frees the old version's region while the new one is live and runs only the old drop list; `on_free` hooks
  run in registration order before the drop list, and a second handle keeps the region alive until it drops;
  `adopt_owner` / `enter_owner` route a shared object's lazy data to the adopting region while it lives and to the
  thread arena afterwards; `enter_table_owner` keeps the current target only for a table inside it; a diagnostic route
  applies to one target, and escapes a scratch region to the target that was current when it was entered; a region's
  free list and rewinds stay inside the region; a region made on one thread is filled on another, read on the first,
  and freed by whichever handle is the last.
- Negative controls, `#[ignore]`d so the normal suite skips them and run on purpose by the Miri job, which fails
  unless Miri rejects them with the known error: the existing `miri_rejects_freeing_a_reference_parameter` (a write
  through a protected borrow) and the new `miri_rejects_reading_a_handle_of_a_freed_region`: safe code keeps a `P`
  past its region's owner and reads it. That read is exactly the gap #646 is about; the test proves Miri sees it, it
  does not close it (the program owner work, #668, is the first step of that).
- `crates/tsrs_core/tests/region_poison.rs`: the same stale read in the shipped build with `TSRS_ARENA_POISON=1`
  reads poison instead of stale or reused data, in both pointer modes. It is a separate test binary because the
  poison mode is read once per process. Not run under Miri: the read is the use after free Miri reports.

## What ran

Three runs of the whole lib test suite, one module at a time (a 25-minute cap per module), with the two negative
controls at the end:

- `sb`: Stacked Borrows, `-Zmiri-ignore-leaks` (the CI model).
- `tb`: Tree Borrows, `-Zmiri-ignore-leaks -Zmiri-tree-borrows`.
- `leaks`: Stacked Borrows with the leak checker on (no flags).

Wall seconds per module, the harness's own "finished in" time is Miri's virtual clock and is not used:

| module | tests | `sb` | `tb` | `leaks` |
| --- | --- | --- | --- | --- |
| arena | 14 + 2 negative controls | ok 2 | ok 3 | 1 leak, see below; ok 3 after the fix |
| bfs | 1 | ok 1 | ok 1 | ok 1 |
| collections | 9 | ok 262 | ok 525 | ok 278 |
| context | 2 | ok 1 | ok 1 | ok 1 |
| core_go | 4 | ok 30 | ok 41 | ok 33 |
| debug | 1 | ok 1 | ok 1 | ok 1 |
| frozen | 2 | ok 1 | ok 1 | ok 1 |
| glob | 3 | ok 1 | ok 2 | ok 1 |
| gocompat | 3 | ok 1 | ok 1 | ok 2 |
| goslices | 3 | ok 5 | ok 39 | ok 5 |
| jsnum | 19 | 3 float failures, 61; ok 60 with `-Zmiri-deterministic-floats` | 4 float failures, 87 | 3 float failures, 64 |
| json | 3 | ok 636 | `max_nesting_depth` not done at the 1500 s cap | ok 1033 |
| linkstore | 1 | ok 1 | ok 1 | ok 1 |
| pattern | 1 | ok 1 | ok 1 | ok 1 |
| ptr | 5 | ok 17 | ok 29 | 8 leaks, all `Box::leak` fixtures of the tests, 17 |
| runslots | 1 | ok 1 | ok 1 | ok 1 |
| semver | 13 | ok 25 | ok 41 | ok 27 |
| stringutil | 14 | 3 ok, then `normalization_matches_pinned_combining_sequences` not done at the cap (1500) | same, stopped at 211 | same, not done at the cap (1501) |
| text | 5 | ok 2 | ok 1 | ok 1 |
| tspath | 27 | 11 ok, then `test_has_relative_path_segment_matches_segment_split` stopped at 512 | same, stopped at 572 | same, stopped at 86 |
| negative control: handle of a freed region | 1 | rejected, 1 | rejected, 1 | rejected, 1 |
| negative control: free of a reference parameter | 1 | rejected, 1 | rejected (other text), 1 | rejected, 1 |

The two stopped tests were stopped by hand once each was the only thing left running in its module, in all three
runs at the same moment; the normalization test had already hit the cap in `sb`. The 15 `tspath` tests after its
slow one and the `stringutil` tests outside `norm` ran in the CI-shaped run below; `norm`'s next test,
`normalization_matches_pinned_tables_for_every_scalar`, ran there for 10 minutes without finishing, so the job skips
the whole `stringutil::norm` module (5 tests, no `unsafe`).

The CI-shaped run, one `cargo miri test` of the lib with the skips of the job and the job's flags
(`-Zmiri-ignore-leaks -Zmiri-deterministic-floats`), passes 114 tests in about 145 s of interpretation here (144 and
146 in two runs), after the build. Before this change the job ran `arena::tests`: 8 tests and one negative control.

## What it found

- **A leak in the new rewind test**, under the leak checker: the first version of
  `region_free_lists_and_rewinds_stay_in_the_region` put an `Rc` clone into the slot it then rewound past, and a
  rewound value is never dropped, so the `Rc` allocation was still live at exit (`memory leaked: alloc237924 (Rust
  heap, size: 16, align: 8)`, `Rc::new` at arena.rs:1731). The test now counts drops of a heap-free value instead,
  and the leak run of `arena::` is clean. Nothing else in the lib leaks: the eight `ptr` reports are the tests' own
  `Box::leak`ed `'static` fixtures (`thin_slices_keep_identity`, `packed_str`), which is why the job keeps
  `-Zmiri-ignore-leaks`. The thread arenas' retained chunks were not reported in any run.
- **The negative controls fail the way the job expects.** The handle of a freed region: `error: Undefined Behavior:
  constructing invalid value of type &u64: encountered a dangling reference (use-after-free)` at `P::get`
  (ptr.rs:464), the same text under Stacked Borrows, Tree Borrows and the leak checker; the job greps for
  `dangling reference (use-after-free)`. The free of a reference parameter: `not granting access to tag <wildcard>
  because that would remove [SharedReadOnly for <144680>] which is strongly protected` under Stacked Borrows (the
  job's `strongly protected`); Tree Borrows rejects it too but says `write access through <wildcard> at
  alloc44450[0xffff0] is forbidden`, after warning that it does not support the integer-to-pointer cast
  (`with_exposed_provenance_mut` in `arena::free_block`, arena.rs:727).
- **No positive test fails for a memory reason** in any configuration: every failure above is a float or a time.
- **`jsnum` fails only because Miri perturbs floats.** `test_exponentiate` (`1e308` came back as
  `1.0000000000000002e308`), `test_from_string` and `test_string_roundtrip` (`19686109595169230000` came back as
  `...240000`, `...243000`, `...220000` in three runs), and under Tree Borrows also
  `float_conversion_preserves_rounding_and_overflow`: the set differs between runs because Miri adds a random error
  to imprecise float operations. `-Zmiri-deterministic-floats` removes it; all 19 pass with the flag (60 s).
- **Four pieces of safe code cost minutes each**: `collections` (ordered-map loops, 262 s), `json`'s
  `max_nesting_depth` (10,000 nested arrays on a 256 MiB stack, 636 s), `stringutil::norm`'s pinned-table checks
  (`normalization_matches_pinned_combining_sequences`, 20,000 random combining sequences through NFD, more than
  25 min; `normalization_matches_pinned_tables_for_every_scalar`, every Unicode scalar, more than 10 min) and
  `tspath`'s `test_has_relative_path_segment_matches_segment_split` (every string of up to 11 bytes over three
  letters, twice: 531,440 strings, more than 8 min). None of them contains `unsafe` or touches the arena, so Miri can
  find nothing in them that the native run does not; the job skips exactly these four (`collections::`,
  `stringutil::norm::` and the two tests by name).
- **One test pins the toolchain, not the code.** `stringutil::rangetable::tests::test_unicode_version_of_case_mappings`
  asserts `char::UNICODE_VERSION == (17, 0, 0)`, true of the pinned stable 1.99.1 and false of the Miri nightly, whose
  core is at 18.0.0. It would fail under any nightly, so the job skips it by name; the three case-mapping tests next
  to it pass against the newer tables.
- **Tree Borrows** agrees with Stacked Borrows on every test that finished, at 1.5 to 8 times the cost (`goslices`
  39 s against 5, `collections` 525 against 262); `json` did not finish inside the cap.

## What it cannot see

| blind spot | why | what covers it instead |
| --- | --- | --- |
| The compressed handles: `reserve.rs`, the `PROT_NONE` reservation, slab commit, decommit and poison fill, `P<T>` as a 32-bit offset | `plain-ptrs` compiles them out; Miri has no model for the reservation or for `madvise` | `cargo test -p tsrs_core` in the default build (137 tests), the recycling checks and `TSRS_ARENA_POISON=1` runs of docs/DEBUGGING.md, the `panic = "abort"` conformance gate |
| `FrozenCell`'s borrow rule in release builds | the borrow counter exists only with `debug_assertions` or `checked-cells`; Miri runs the dev profile, so it checks the counted variant, never the flagless one that ships | the `checked-cells` feature on a release build; the `Send` / `Sync` by decree in `tools/lint/send_sync.tsv` stays a review item |
| Everything above `tsrs_core`: the checker's arena recycling, `Program` and region ownership in `tsrs_compiler`, `tsrs_project` and `tsrs_ls` | no test of those crates runs under Miri; a checker test interprets the whole checker, and `tsrs_core`'s cheapest modules already cost seconds. Nothing in the code is known to stand in the way: the checker frees its blocks through `P<T>` (a `NonNull` in `plain-ptrs` builds, never a reference) and `*const [T]`, and `tools/lint/source.py` check 3 reports no arena free of a reference parameter | `tools/lint/source.py` check 3, the `arena-safety` job, the recycling and language-server lifetime regressions |
| A handle that outlives its owner in production code | the negative control shows Miri reports the read; it cannot enumerate the places where safe code could do it (`&'static Program` signatures, `P::get` everywhere) | the ownership work of #646; `tests/region_poison.rs` shows what such a read does in the shipped build |
| Leaks by design | thread arenas keep their chunks until exit; a rewound value loses its drop entry; `ptr`'s tests `Box::leak` their fixtures | the job runs with `-Zmiri-ignore-leaks`; the leak-checking run is recorded above, not in CI |
| Exposed provenance in the free path | `free_block` rebuilds the block pointer from an address (`with_exposed_provenance_mut`); Stacked Borrows accepts it with a warning, Tree Borrows does not support it | the `sb` configuration in CI; a strict-provenance free path would be a change of the arena API |

## The job

`.github/workflows/miri.yml` runs every lib test of `tsrs_core` under Stacked Borrows with `plain-ptrs`, skipping
the four slow pieces of safe code and the Unicode-version pin named above, with
`-Zmiri-ignore-leaks -Zmiri-deterministic-floats`, then the two
negative controls with `-- --exact --ignored`. The step fails unless at least 114 tests passed (a filter that
quietly matched fewer tests after a rename would drop the coverage without failing; raise the number when tests
are added) and unless each negative control's log holds its known error (`strongly protected`, `dangling reference
(use-after-free)`), so a Miri that stopped seeing either class fails the job rather than passing it. The job runs on
pull requests that touch `crates/tsrs_core` or the workflow, so a change to the handle or region code meets Miri
before it merges instead of on `main` afterwards. Its timeout is 45 minutes: the interpretation took 2.4 min here,
CI runners are two to three times slower, and the sysroot and the test build come first.

## Not done

- Tree Borrows is not the CI model: it costs up to 8 times more, it does not finish `json` inside 25 minutes, and
  it does not support the integer-to-pointer cast in the free path. Worth a second job once the free path uses
  strict provenance, not before.
- The skipped tests (`collections`, `stringutil::norm`, the two slow single tests, the Unicode-version pin) run
  natively only. If one of the slow ones ever needs Miri, give it a `cfg!(miri)` size.
- The crates above `tsrs_core` never meet Miri. The cheapest useful step would be one `tsrs_checker` unit test of
  `recycle_mapper` under Miri, timed first.
- `memsplit.rs` (7 `unsafe` sites, the resident-memory split) has no tests, under Miri or otherwise.
- A compile-fail (`trybuild`) harness for the owner types of #646 makes sense once there is an owner type whose misuse
  the compiler can reject; today there is none.
