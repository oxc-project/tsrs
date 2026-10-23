# Compact signature-link caches

SignatureLinks stores a resolved signature plus two node-specific caches. TypeScript Go keeps all three pointers separately. Effects signatures are used only for call/binary expressions, while runtime decorator signatures are cached on the decorated declaration. The latter two can share a slot. The resolved signature remains independent: call expressions can need both resolved and effects signatures, and decorated methods can need both resolved and decorator signatures.

The whole-process memory benefit is modest. In the nine-pair default-checker confirmation, VS Code uses 2000.92 → 1992.38 MiB (-0.43%), with lower RSS in 7/9 pairs. Its initial -1.75% median reduction did not repeat at that magnitude. Playwright's default-checker median increased in both batches; the confirmation is 409.27 → 415.06 MiB (+1.42%). This is an observed tradeoff despite the smaller cache record. This change does not claim a speedup.

## Layout and behavior

| Layout on 64-bit hosts | TypeScript Go | tsrs before | tsrs after |
| --- | ---: | ---: | ---: |
| Optional signature fields | 3 × 8 B | 3 × 4 B | 2 × 4 B |
| Struct size | 24 B | 12 B | 8 B |
| Arena bytes per record, excluding indexing/capacity overhead | 24 B | 16 B | 8 B |

The new storage is private and exposed through effects/decorator accessors with debug assertions on the node kinds. Invalid decorator parents still cache the existing any-signature sentinel; they are not restricted to valid decorator targets by the assertion. Resolved-signature caching, the stable address of the link record, language-service save/clear/restore, and frozen-store copying retain their previous behavior. There is no extra per-node tag, map or pointer indirection. Plain 64-bit pointers also shrink the struct 24 → 16; 32-bit builds shrink 12 → 8.

The object-layout audit checked the pinned TypeScript callers and observed effects/decorator access across five public projects. It found no node using both. Prior memory/layout work, the measured-and-rejected list in notes/perf-round2-followups.md, docs/RUST.md Techniques and notes/mem-round4.md were reviewed before implementation; this slot-sharing optimization was not a previously rejected experiment.

## Release measurements

Baseline: compact-symbol PR #732 head `3fc8e48c13ad5d08d5a4c0571cf15c7256d1934b`. Candidate: the same head with only the SignatureLinks runtime change in checker_15.rs, flow.rs and types.rs. Both include the existing compact-symbol change; the table isolates the additional signature-link change. The TypeScript reference pin remains `3e9236361fa97a9e6c685f4833ac457ae2d8c50a`.

Local Apple M4 Pro, 12 logical cores, macOS arm64. Both binaries used `cargo build --release --locked --offline -p tsrs_cli --config profile.release.debug=true --config profile.release.strip=false`, Rust 1.99.1, fat LTO, one codegen unit, compressed handles, and the same environment. Measurements started after all builds and tests completed. Hyperfine 2.0.0 measured the compiler directly, with no shell or Node process included in its peak RSS. Each cell had two warmups per binary and alternating baseline/candidate order, reversed on every other pair. The commands use `--noEmit --incremental false --pretty false` with each catalog project's config. No emit or incremental cache writes are included.

Coverage is now all 19 projects in bench/projects.json. The first sweep attempted 17; its Drizzle row was subsequently found to exit on removed configuration options, so that row is replaced by an actual type-checking run. TypeORM and date-fns were prepared at their pinned commits and measured afterward with the same saved binaries, build configuration, machine, checker settings and sample counts. The runtime patch and binary hashes were reverified before the additional runs. Original samples are preserved; the valid sessions are combined in [committed measurement data](data/signature-links.json).

date-fns needs an explicit CLI exception: both binaries additionally receive `--composite false --declaration true`. Its catalog config enables composite mode, which conflicts with `--incremental false` and otherwise exits with TS6379 before checking types. Disabling composite mode alone also needs declaration enabled because the config sets emitDeclarationOnly; without that, it exits with TS5069. Both failed-config runs are retained in the committed data under excluded, outside the tables and improvement counts. The date-fns rows use the successful type-checking run. No project source or configuration was changed for this override.

Drizzle uses [a temporary config](data/signature-links.json) extending the pinned build config. It clears the removed baseUrl option, makes the existing ~/* mapping point to the same source directory, and sets rootDir to the original package directory so moving the config outside that directory does not change its root. Both binaries use the same config and source files. The original TS5090/TS5102 samples are retained under excluded in the committed data, outside the current tables and improvement counts. The replacement runs exercise 468,521 types and 2,017,576 instantiations in single-threaded mode.

Default-checker peak RSS: seven interleaved pairs per project, median bytes converted to MiB. Checker count is left at the compiler's machine/program default on both sides. “Lower pairs” counts the pairs where candidate peak RSS was lower. A signed delta is candidate minus baseline.

| Public project | Before MiB | After MiB | Delta MiB | Median delta | Paired delta | Lower pairs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| vscode | 2031.61 | 1996.00 | -35.61 | -1.75% | -1.75% | 6/7 |
| xstate-main | 249.59 | 249.53 | -0.06 | -0.03% | -0.14% | 4/7 |
| webpack | 457.78 | 454.56 | -3.22 | -0.70% | -0.86% | 6/7 |
| mui-docs | 1461.92 | 1467.05 | 5.13 | +0.35% | +0.16% | 2/7 |
| Compiler | 75.77 | 75.53 | -0.23 | -0.31% | -0.10% | 5/7 |
| Compiler-Unions | 80.56 | 80.45 | -0.11 | -0.14% | -0.12% | 5/7 |
| cal-diy | 1639.17 | 1634.77 | -4.41 | -0.27% | -0.27% | 4/7 |
| formbricks-web | 1798.16 | 1792.19 | -5.97 | -0.33% | -0.15% | 5/7 |
| supabase-studio | 1546.28 | 1545.53 | -0.75 | -0.05% | -0.04% | 4/7 |
| t3code-server | 1918.58 | 1900.06 | -18.52 | -0.97% | -1.19% | 7/7 |
| mikro-orm | 1787.45 | 1781.59 | -5.86 | -0.33% | -0.68% | 4/7 |
| next-packages-next | 609.14 | 606.08 | -3.06 | -0.50% | -0.85% | 4/7 |
| next-root | 431.23 | 430.38 | -0.86 | -0.20% | 0.00% | 3/7 |
| storybook | 447.48 | 452.67 | 5.19 | +1.16% | +0.58% | 1/7 |
| nuxt | 458.64 | 465.28 | 6.64 | +1.45% | +0.78% | 3/7 |
| playwright | 407.20 | 415.48 | 8.28 | +2.03% | +2.57% | 2/7 |
| drizzle-orm | 529.75 | 532.03 | 2.28 | +0.43% | +0.55% | 2/7 |
| typeorm | 242.66 | 242.63 | -0.03 | -0.01% | -0.05% | 4/7 |
| date-fns | 176.80 | 176.34 | -0.45 | -0.26% | +0.50% | 3/7 |

The median is lower on 14 of 19 installed projects, higher on 5, and equal on 0. Candidate RSS is lower in every pair on 1 project. The largest absolute median reduction is vscode: 35.61 MiB (-1.75%). Per-pair ranges and quartiles are retained in the raw JSON; changes within that variation should not be described as repeatable whole-process improvements. A 50% reduction in this record's slot does not imply a comparable reduction in the whole compiler.

Every catalog project was measured in both default-checker and single-threaded mode; none remain unavailable. All measured workloads were public projects.

## Single-threaded check

Three interleaved pairs with `--singleThreaded`, medians. This removes variation from worker assignment when checking instruction cost. The macOS instruction counter includes kernel activity and is an advisory local regression screen, not a deterministic Linux instruction gate. Wall time is included for context; this change does not claim a speedup.

| Public project | Before RSS MiB | After RSS MiB | RSS delta | Before instructions, billions | After instructions, billions | Instruction median delta | Paired instruction delta | Wall median delta | Paired wall delta |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| vscode | 1578.31 | 1570.41 | -0.50% | 80.6815 | 81.5755 | +1.11% | -0.01% | -0.02% | -0.21% |
| xstate-main | 178.92 | 178.58 | -0.19% | 5.9057 | 5.9051 | -0.01% | -0.01% | -0.25% | -0.23% |
| webpack | 305.09 | 305.39 | +0.10% | 10.7882 | 10.7887 | +0.00% | -0.03% | -1.08% | -1.19% |
| mui-docs | 730.95 | 729.09 | -0.25% | 37.2127 | 37.2043 | -0.02% | -0.02% | +0.12% | -0.26% |
| Compiler | 56.30 | 56.13 | -0.31% | 1.7130 | 1.7143 | +0.07% | +0.12% | +0.12% | +0.41% |
| Compiler-Unions | 59.84 | 59.67 | -0.29% | 2.9388 | 2.9364 | -0.08% | -0.12% | -1.48% | -1.48% |
| cal-diy | 760.52 | 756.00 | -0.59% | 31.8278 | 31.7069 | -0.38% | -1.82% | -2.01% | -3.39% |
| formbricks-web | 1086.80 | 1082.22 | -0.42% | 40.6090 | 40.5468 | -0.15% | -0.15% | +17.72% | +1.21% |
| supabase-studio | 860.83 | 860.66 | -0.02% | 44.8852 | 44.9002 | +0.03% | +0.20% | -3.11% | -2.32% |
| t3code-server | 716.88 | 714.94 | -0.27% | 39.4728 | 39.4579 | -0.04% | +0.01% | -2.15% | -1.02% |
| mikro-orm | 987.81 | 983.89 | -0.40% | 47.5929 | 47.5990 | +0.01% | -0.01% | -5.94% | -2.46% |
| next-packages-next | 435.23 | 434.77 | -0.11% | 17.1983 | 16.9456 | -1.47% | -0.10% | -2.92% | -2.92% |
| next-root | 314.11 | 312.28 | -0.58% | 17.1370 | 17.0233 | -0.66% | -0.66% | -3.92% | -4.74% |
| storybook | 275.02 | 274.28 | -0.27% | 10.1925 | 10.1898 | -0.03% | -0.01% | -14.61% | -14.61% |
| nuxt | 280.03 | 279.61 | -0.15% | 10.0406 | 10.0537 | +0.13% | +0.04% | +1.95% | +1.14% |
| playwright | 238.34 | 237.91 | -0.18% | 7.5088 | 7.5082 | -0.01% | -0.01% | -0.12% | +0.30% |
| drizzle-orm | 208.83 | 208.94 | +0.05% | 6.3272 | 6.3252 | -0.03% | -0.03% | +0.04% | -0.22% |
| typeorm | 189.03 | 188.39 | -0.34% | 8.1140 | 8.0737 | -0.50% | -0.67% | -33.60% | -38.30% |
| date-fns | 122.69 | 122.53 | -0.13% | 4.5525 | 4.2936 | -5.69% | -3.51% | -9.10% | -6.70% |

“Median delta” compares the separate before/after medians. “Paired delta” is the median of the percentage changes within each adjacent pair. Both are shown because the first VS Code single-threaded measurements fell into two instruction-count bands on both binaries: independent medians show +1.11%, while the paired median shows -0.01%. Three samples cannot resolve that difference; the follow-up below adds seven pairs.

Candidate single-threaded RSS was lower in every pair on 10 of 19 projects. All 38 measurement cells preserve diagnostics and exit status in both warmup runs. Files, Symbols, Types and Instantiations are identical before/after on all 19 single-threaded projects.

## Follow-up measurements

VS Code was repeated to confirm the largest RSS reduction and check the single-threaded instruction-count bands. MUI was repeated because the initial default-checker median increased. Those two were selected before the initial sweep finished. Storybook, Nuxt and Playwright were added after the sweep showed larger default-checker RSS increases. The initial results remain above, including increases and noisy wall times. The follow-up uses the same binaries, machine, flags, warmups and alternating order. These confirmations ran after the original 17-project sweep and before the additional catalog projects were prepared. Benchmark jobs always run sequentially.

TypeORM and date-fns also received seven-pair single-threaded confirmations after their initial samples showed large wall-time or instruction differences. These preserve the same flags as their primary rows, including the date-fns CLI exception.

### Default-checker confirmation

9 additional interleaved pairs per project. These samples are separate from the initial sweep.

| Project | Before RSS MiB | After RSS MiB | RSS median delta | Paired RSS delta | RSS lower pairs | Instruction median delta | Paired instruction delta | Wall median delta | Paired wall delta |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| vscode | 2000.92 | 1992.38 | -0.43% | -0.43% | 7/9 | -0.21% | -0.03% | +3.63% | +0.33% |
| mui-docs | 1462.64 | 1455.75 | -0.47% | +0.13% | 4/9 | +0.26% | +0.21% | -1.37% | +1.52% |
| storybook | 450.66 | 447.25 | -0.76% | -1.03% | 8/9 | -0.64% | -1.19% | +0.46% | +2.41% |
| nuxt | 465.33 | 461.06 | -0.92% | -1.64% | 6/9 | -1.67% | -2.55% | -3.73% | -5.17% |
| playwright | 409.27 | 415.06 | +1.42% | +2.08% | 3/9 | +1.27% | +2.74% | -2.06% | +0.43% |

vscode paired RSS range: -0.91% to +0.30%; quartiles -0.60% to -0.05%. Paired instruction range: -0.85% to +0.47%.

mui-docs paired RSS range: -1.77% to +1.05%; quartiles -0.91% to +0.48%. Paired instruction range: -1.28% to +1.54%.

storybook paired RSS range: -1.38% to +1.32%; quartiles -1.30% to -0.32%. Paired instruction range: -3.32% to +1.42%.

nuxt paired RSS range: -4.42% to +4.41%; quartiles -2.27% to +3.22%. Paired instruction range: -9.54% to +10.94%.

playwright paired RSS range: -5.03% to +6.00%; quartiles -0.91% to +3.42%. Paired instruction range: -4.87% to +7.24%.

Raw samples: [confirm-default/results.json](data/signature-links.json).

### VS Code single-threaded confirmation

7 additional interleaved pairs per project. These samples are separate from the initial sweep.

| Project | Before RSS MiB | After RSS MiB | RSS median delta | Paired RSS delta | RSS lower pairs | Instruction median delta | Paired instruction delta | Wall median delta | Paired wall delta |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| vscode | 1578.14 | 1571.19 | -0.44% | -0.47% | 6/7 | -0.04% | -0.06% | -5.89% | -5.13% |

vscode paired RSS range: -1.16% to +0.01%; quartiles -0.58% to -0.19%. Paired instruction range: -1.06% to +0.03%.

Raw samples: [confirm-single/results.json](data/signature-links.json).

### TypeORM single-threaded confirmation

7 additional interleaved pairs per project. These samples are separate from the initial sweep.

| Project | Before RSS MiB | After RSS MiB | RSS median delta | Paired RSS delta | RSS lower pairs | Instruction median delta | Paired instruction delta | Wall median delta | Paired wall delta |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| typeorm | 186.42 | 185.36 | -0.57% | -0.58% | 7/7 | -0.03% | +0.07% | -0.47% | +0.96% |

typeorm paired RSS range: -1.84% to -0.48%; quartiles -1.60% to -0.57%. Paired instruction range: -0.25% to +0.53%.

Raw samples: [confirm-typeorm-single/results.json](data/signature-links.json).

### date-fns single-threaded confirmation

7 additional interleaved pairs per project. These samples are separate from the initial sweep.

| Project | Before RSS MiB | After RSS MiB | RSS median delta | Paired RSS delta | RSS lower pairs | Instruction median delta | Paired instruction delta | Wall median delta | Paired wall delta |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| date-fns | 122.81 | 122.66 | -0.13% | -0.05% | 5/7 | -0.03% | +0.12% | -0.02% | +0.22% |

date-fns paired RSS range: -0.56% to +0.14%; quartiles -0.23% to -0.04%. Paired instruction range: -0.43% to +0.41%.

Raw samples: [confirm-date-fns-single/results.json](data/signature-links.json).

The VS Code single-threaded confirmation changes RSS by -0.44% and instructions by -0.04% (paired instruction median -0.06%). The initial apparent instruction increase does not repeat. Wall-time ranges remain broad, so no speedup is claimed.

typeorm: the initial single-threaded instruction delta of -0.50% becomes -0.03% in the seven-pair confirmation (paired median +0.07%). The initial wall-time delta of -33.60% becomes -0.47% (paired median +0.96%). The large apparent speedup or instruction reduction does not repeat.

date-fns: the initial single-threaded instruction delta of -5.69% becomes -0.03% in the seven-pair confirmation (paired median +0.12%). The initial wall-time delta of -9.10% becomes -0.02% (paired median +0.22%). The large apparent speedup or instruction reduction does not repeat.

MUI's confirmation has a lower independent RSS median but a positive paired median, with lower RSS in only 4/9 pairs; it is inconclusive. Storybook and Nuxt reverse the initial median increase. Playwright's default-checker median increases in both batches, so that observation is retained as a tradeoff rather than dismissed as noise. Its single-threaded RSS and instruction medians are slightly lower.

### Playwright default-checker work inspection

After the original 17-project sweep and its confirmations, 12 further interleaved pairs ran with extended diagnostics to inspect allocation counts. These are separate diagnostic runs, not additional RSS or instruction measurements. Both binaries process the same 1514 files. Their creation counts vary widely within each binary:

| Counter | Before min–max | After min–max | Before median | After median | Median delta |
| --- | ---: | ---: | ---: | ---: | ---: |
| Symbols | 965437–1072975 | 942208–1085258 | 1009142 | 1047728 | +3.82% |
| Types | 784983–902895 | 756601–905140 | 834168.5 | 873469.5 | +4.71% |
| Instantiations | 1342596–1656248 | 1295251–1658862 | 1477684 | 1578185 | +6.80% |

These counters count creations across checkers, including duplicated work. The higher candidate medians are consistent with the higher concurrent RSS and instructions observed on this project; they do not establish why the work distribution shifted. The effect of the layout change on concurrent scheduling has not been isolated. The single-threaded before/after allocation counts are identical. Allocation-counter samples are included under default_counter_inspection in the [committed measurement data](data/signature-links.json).

## Correctness and checks

The full checks below ran on the measured base. After rebasing onto main `19ba8864`, the optimized dev build, 166 checker/core unit tests (one ignored), all 50 CLI regressions, warnings-denied Clippy, source checks and formatting pass.

- Full compiler baselines, all supported error/type/symbol/emit/map outputs, in both Go-history and canonical/default-checker modes: all 72 before/after outcome lists are byte-identical. Each mode exercises 15,448 variants, with 13,700 diagnostic passes, 12,998 type/symbol passes, 13,626 emit passes, 149 JavaScript-map passes and 156 source-map passes; unsupported cases remain skipped, with no failures, crashes or timeouts.
- Pinned Go Fourslash: 4,092 exercised groups pass, 470 skip, no failures. This covers the language-service paths that save, clear and restore resolved-signature caches.
- Workspace tests: 1,239 pass, four ignored, no failures. Focused checker/core tests also pass.
- All 49 CLI regressions pass. New modern/legacy decorator cases combine declaration signatures, runtime decorator signatures, call narrowing and assertion effects, plus invalid constructor decorators. Their diagnostic baselines were generated by the exact pinned Go reference and matched by the candidate, including exit status.
- Warnings-denied Clippy, source lint, formatting, plain-pointer + shared-graph compilation and WebAssembly compilation pass.

## Reproduction data

[Committed measurement data](data/signature-links.json) includes all primary and confirmation samples, paired summaries, binary and patch hashes, compiler flags, machine information, timestamps, configuration overrides, allocation-counter samples and the excluded configuration-error runs. Paths use <work>, <additional-work>, <source> and <results> placeholders. The primary compiler flags and pinned project revisions are described above and in bench/projects.json. Both binaries use the same prepared workload and per-project overrides.
