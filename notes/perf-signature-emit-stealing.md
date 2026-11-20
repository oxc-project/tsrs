# perf-signature-emit-stealing: work stealing in the declaration-signature step of a warm incremental run

Follow-up to notes/perf-incremental-pass-stealing.md, which made the incremental check pass steal under `--fast` and
measured the step before it. After an edit that adds an export to a widely imported file, a warm run computes the
declaration signatures of the files that import it (`collect_all_affected_files`, a level of the referenced-by walk
per emit), and on a large private monorepo that step kept about 3 of 8 cores busy: each level ran on its files'
static checkers, and the files of a level sat on a few of them (the busiest checker held 3.7-4.9 times the mean). This
note lets those emits move files between checkers, with the stored signatures, the build info and the output
unchanged.

## Does a printed declaration depend on the checker that prints it?

The signature is a hash of the file's declaration output plus its declaration diagnostics
(`compute_signature_with_diagnostics`). Moving a file's emit to another checker changes nothing only if that text does
not depend on which checker prints it or what that checker resolved before.

notes/perf-hub-edit-shortcut.md ("idea 2", deferred signatures) found that it did: computing the signatures after the
check instead of before it changed the stored signature of 7-11 of about 22k files, all printed declarations of
unions of object literals whose filled-in `x?: undefined` properties came out in another order. That was
`getUndefinedProperty`, which cached the filler symbol per property name for the whole checker, so the filler sorted
at the declaration of whichever `x` the checker had widened first. Since source 112, TypeScript keys that cache by
source symbol, and tsrs uses that cache in both histories (notes/perf-order-independence.md), so the filler always
sorts at the declaration it stands for.

What else could make printed text depend on history, and why it does not here:

- **Union and intersection order**: constituents are sorted by `compareTypes`, a function of the types; its type-id
  fallback only orders types that print the same (the audit in notes/perf-order-independence.md).
- **Member order of anonymous and widened types**: declaration order, by `compareSymbols` (file, then position).
- **Alias names**: unions, intersections and instantiations are interned with their alias in the key, so the alias a
  printed type carries does not depend on which request created it first.
- **Inferred types themselves**: the same canonical-history claim the check passes rely on. Its known exceptions
  (notes/open-history-dependence.md: a base constraint cut by the depth guard, a circularity entered elsewhere, a
  budget error) could in principle change an inferred type and with it a printed declaration, as they can change a
  diagnostic.
- **Declaration diagnostics** (part of the hash): as any other diagnostic of the file.

### The direct test

A comment appended to a global-scope declaration file of the program makes the next warm run treat every file as
affected and compute every file's declaration signature in one batched emit (the global-scope path of
`collect_all_affected_files`), and store them in the build info. On main's binary, from the same cold build info, that
warm run was repeated under seven placements: stealing at 8 checkers, `TSRS_CHECKER_ASSIGNMENT=random:1` and
`random:2` at 4 checkers, `random:3` at 8, `random:4` at 2 (each a random placement and a random visit order per
checker), `locality` at 3 and a single checker. Every stored signature was compared with the first placement's.

| project | files | declaration signatures computed | placements | differing |
| --- | --- | --- | --- | --- |
| vscode | 10,427 | 9,399 | 7 | 0 |
| mikro-orm | 2,820 | 1,899 | 7 | 0 |
| mui-docs | 16,766 | 12,964 | 7 | 0 |
| supabase-studio | 12,553 | 4,823 | 7 | 0 |
| a large private monorepo | | every file's | 7 | 0 |

So on these programs a file's declaration signature is the same on any checker, after any other emits on it.

## The change

- `EmitOptions::steal` and a `GroupPass::SignatureEmit`: the emit runs with stealing and heavy files first (the check
  passes' scheduler), without file splitting or the type-check pass's memory features. Owners follow the files, as in
  the check passes.
- The batched emits of `collect_all_affected_files` (each level of the walk, and the global-scope batch) use it when
  `Program::signature_emits_steal` (fast mode, no named assignment, several checkers, `TSRS_SIGNATURE_STEAL` not `0` or
  `off`). Single-file emits are unchanged.
- The walk steals only when no checker holds a global diagnostic before it. After it, if still none does, every file
  goes back to its static checker (`reset_owners_static`) and the snapshot is updated. If one does, nothing of the walk
  has been committed (the handler only writes the snapshot at the end): the checkers are replaced by fresh ones on the
  static assignment (`restart_static`) and the walk runs again without stealing. `--extendedDiagnostics` counts it as
  `Signatures: walk rerun statically`.
- notes/perf-incremental-pass-stealing.md's `restart_static` now restores the static assignment
  (`PoolState::associations`) rather than the owners before the check, which are the same thing there.

## Why it is exact

The walk's outputs are the signatures it stores, the files it visits (which files' semantic diagnostics are dropped,
which become pending emit) and the state it leaves in the checkers for the check that follows.

- **Signatures**: the same on any checker (above), so each level's signatures, and with them the next level's files,
  the files whose diagnostics are dropped and the pending-emit sets, are the ones the static walk computes. They
  depend only on signatures and the reference map.
- **Global diagnostics in the checkers**: a signature emit can report one (an async generator's inferred return type
  needs `AsyncIterableIterator`), and which checker holds it decides which file the incremental check stores it with
  (notes/perf-incremental-pass-stealing.md, reasons 1-3). The stolen walk is kept only when no checker holds any,
  before and after. Then the static walk would have reported none either (the same files are emitted, and a missing
  global a file needs is reported by whichever checker first needs it), and the check starts with no globals anywhere,
  as after the static walk. Otherwise the walk is redone statically on fresh checkers. Those start with no globals,
  like main's checkers before the walk (the walk stole only if they had none), visit the same files in the same order
  on the same checkers, and so report the same globals on the same checkers as main.
- **Owners**: back to the static assignment after a stolen walk, so the check pass starts from main's placement and
  notes/perf-incremental-pass-stealing.md's argument applies unchanged.
- **Other checker state** (what each checker resolved while emitting): a file's own diagnostics do not depend on it,
  the same claim the check passes make; with its known exceptions.
- **Default mode**: `stealing_enabled` is false under Go history, so no emit steals.

## Results

m8g.4xlarge (Graviton4), one box, `taskset -c 0-7` or `-c 0-3`, `--noEmit --pretty false --fast
--extendedDiagnostics`, build info in `/tmp`. The same binary (a fresh `cp` of it) with `TSRS_SIGNATURE_STEAL=0` (A)
and without (B), alternating; median of per-pair changes with a ~95% interval. "Signature step" is the `Emit time`
row: in a `--noEmit` warm run that is the walk's signature emits plus the build info write. The hub edit appends an
export to the program's most imported file before each run, from a cold build info of the unedited program.

**Warm run after a hub edit**, 10 pairs:

| project | cores | wall A → B (s) | signature step A → B (s) | wall | signature step | check | CPU | peak RSS |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| private monorepo | 8 | | | −9.4% [−10.7, −7.4] | −14.5% [−19.0, −12.1] | −3.0% [−6.4, −2.1] | +1.3% [−0.7, +2.7] | +3.7% [+3.3, +4.7] |
| | 4 | | | −12.1% [−12.8, −11.0] | −28.5% [−29.6, −27.5] | +0.7% [−1.5, +3.5] | −1.0% [−1.9, −0.1] | +3.0% [+2.7, +4.2] |
| vscode | 8 | 2.64 → 2.55 | 0.55 → 0.46 | −3.7% [−4.8, −2.4] | −16.8% [−22.3, −13.0] | +0.3% [+0.0, +0.4] | −1.3% [−3.2, −0.7] | +0.3% [−0.1, +0.7] |
| | 4 | 4.81 → 4.71 | 0.96 → 0.84 | −2.0% [−3.0, −1.1] | −12.4% [−16.0, −8.7] | +0.9% [−0.4, +2.1] | −1.1% [−1.4, −0.5] | +0.1% [−0.3, +0.7] |
| mikro-orm | 8 | 1.04 → 1.03 | 0.11 → 0.10 | −1.1% [−1.6, −0.5] | −10.9% [−11.9, −10.1] | +0.1% [−0.3, +0.6] | −0.7% [−1.0, −0.3] | +0.7% [+0.1, +1.0] |
| | 4 | 1.62 → 1.59 | 0.13 → 0.10 | −1.9% [−3.1, −1.4] | −23.4% [−25.4, −21.1] | −0.2% [−1.5, +0.4] | −1.0% [−2.0, −0.2] | +0.4% [−0.2, +0.8] |
| mui-docs | 8 | 1.67 → 1.62 | 0.07 → 0.07 | −1.4% [−6.1, +6.3] | +0.0% [−1.5, +3.1] | −2.5% [−6.0, +8.4] | −2.1% [−5.1, +10.6] | −0.6% [−2.8, +3.7] |
| | 4 | 2.01 → 2.00 | 0.09 → 0.09 | −0.3% [−1.2, +0.8] | −1.7% [−6.8, +4.9] | −0.2% [−0.7, +0.7] | −0.4% [−1.0, +0.4] | −0.2% [−0.4, +0.5] |
| supabase-studio | 8 | 1.58 → 1.58 | 0.13 → 0.13 | −0.1% [−0.4, +0.4] | +0.0% [−1.5, +0.7] | +0.1% [−0.2, +0.4] | +0.0% [−0.4, +0.4] | −0.2% [−0.6, +0.4] |
| | 4 | 2.49 → 2.50 | 0.18 → 0.18 | +0.5% [−0.2, +1.3] | +0.0% [−1.1, +2.3] | +0.3% [−0.7, +1.4] | +0.4% [−0.3, +0.8] | +0.4% [−0.8, +1.3] |

The 8-core rows are this change's final code; the 4-core rows are its first revision, which differs only by the
thief limit below, and runs the same code at 4 checkers (the limit applies from 8).

**Cold and warm no change**, 8 cores, 8 pairs: no walk runs, and nothing moves. Wall time within ±1% everywhere,
every interval containing 0 (private monorepo cold +0.4% [−0.4, +1.3], warm +0.2% [−0.6, +0.4]; vscode +0.3% and
−1.0%; mikro-orm −0.3% and −0.6%; mui-docs −0.7% and +0.2%; supabase-studio −0.2% and +0.7%).

Reading them:

- Where the hub edit gives the walk real levels, the step gets shorter: the private monorepo −28.5% at 4 cores and
  −14.5% at 8, vscode −12% to −17%, mikro-orm −11% to −23%. On mui-docs and supabase-studio the edit leaves the step
  at 0.07-0.18 s, mostly the build info write, and nothing changes.
- Less than the "about half" estimated in notes/perf-incremental-pass-stealing.md, at 8 cores especially. The step
  still ends every level at a barrier, the build info write is in the same row, and a thief emits a file without the
  types its owner resolved for it: CPU time is flat while the private monorepo's peak RSS grows 3-4%. Not broken
  down further here.
- **The thief limit.** The check passes let at most a quarter of the checkers steal from one queue at a time from 8
  checkers on (notes/perf-icount-assign.md). With that limit the first revision took the private monorepo's step only
  9.9% [−12.6, −5.7] shorter at 8 cores (wall −6.5%) where 4 cores, which have no limit, gave −28.5%: a level puts
  most of its files on a few checkers, so two helpers per queue are not enough. The signature emits now have no limit
  (`max_thieves_per_queue`); at 8 cores that gives the −14.5% above. vscode's step changed little (−17.0% with the
  limit, −16.8% without).

## Exactness evidence

Byte comparisons of stdout (with the exit status) and of the build info, written to the same path by both binaries.

- **The signature test above**: every stored signature of the five projects identical under seven placements.
- **The five projects at 8 and 4 cores, release builds** (main against this branch, fresh copies of both binaries):
  a cold incremental run; a warm run with no change; a warm run after an export is appended to the most imported
  file (the walk's batched levels); and a warm run after a comment is appended to a global-scope declaration file (the
  global-scope batch, every file's signature). For each state: main and the branch print the same bytes and write the
  same build info; the branch run twice from the same build info writes the same bytes; the branch with
  `TSRS_SIGNATURE_STEAL=0` writes the same bytes; and the warm output equals a cold run of the same state. All cells
  identical (the global-scope state ran for mikro-orm and mui-docs at 4 cores only: their only global-scope
  declaration files are under `node_modules`, which the first version of the script skipped). After the thief-limit
  change (below), the 8-core cells again.
- **The generated project of notes/perf-incremental-pass-stealing.md** (local dev builds, 2, 3, 4 and 8 checkers,
  `lib` es2015 and es2020, and with `TSRS_SPLIT_FILES=force:3 --skipLibCheck false`): cold, no change, leaf edit,
  exported-shape edit of the hub, a global-script error added and removed, a global diagnostic added, kept and removed,
  two files with one, each removed, another hub edit. Main and the branch identical at every step, the branch identical
  to itself. The step that adds an async generator takes the walk's fallback (`Signatures: walk rerun statically`).
- **`testdata/regressions`**, every case at 2 and 4 checkers, `--fast --incremental true`, cold and warm: 530 runs,
  all identical to main. (These have no edits, so the walk does not run; the cases above cover it.)
- **`crates/tsrs_cli/tests/integration/signature_emit_stealing.rs`**: stolen against static walk (`TSRS_SIGNATURE_STEAL`)
  over hub edits, a shape change, a global-script edit and a global diagnostic added and removed, at 2 and 4 checkers,
  and that a signature emit reporting a global diagnostic takes the fallback. With the fallback disabled the test fails
  on that assertion; its byte comparisons alone do not catch it every time, because whether the file with the global
  moves to another checker during the walk depends on timing.

## Not done

- **Keeping the owners where the walk moved them**, so that a file is checked on the checker that just emitted its
  declarations: the check would start from a placement that depends on timing, and the static paths of
  notes/perf-incremental-pass-stealing.md (the check pass when the checkers start with different globals, and its
  fallback) assume the static one. Not measured.
- **Stealing in the single-file emits** (a changed file's own signature, a level of one file): nothing to balance.
- **Computing the signatures after the check** (notes/perf-hub-edit-shortcut.md idea 2): rejected there, and it does
  not change the step's parallelism.

## Reproduce

```sh
# a warm run after a hub edit, stolen and static signature emits (same binary)
TSRS_SIGNATURE_STEAL=0 tsrs -p . --noEmit --fast --incremental true --tsBuildInfoFile /tmp/x.tsbuildinfo --extendedDiagnostics
tsrs -p . --noEmit --fast --incremental true --tsBuildInfoFile /tmp/x.tsbuildinfo --extendedDiagnostics
# every file's signature under a placement: comment a global-scope .d.ts, then a warm run from a cold build info
TSRS_CHECKER_ASSIGNMENT=random:1 tsrs -p . --noEmit --fast --checkers 4 --incremental true --tsBuildInfoFile /tmp/x.tsbuildinfo
cargo test -p tsrs_cli --test integration signature_emit_stealing
```
