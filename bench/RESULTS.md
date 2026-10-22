# Benchmark results

Measured 2026-10-22T15:22:27.537Z on Namespace `nscloud-ubuntu-24.04-amd64-8x16` (8 vCPU, 16 GB RAM, Linux x64, AMD EPYC).

Arithmetic means of 10 runs after 3 warmups per compiler and project. Projects are never averaged together.

4 checkers and 8 worker CPUs per compiler.

tsrs commit: `2183e779c88579f74cb25218203c221732abba2b`.

[Benchmark run and raw-data artifacts](https://github.com/oxc-project/tsrs/actions/runs/39801070622).

[Methodology](README.md).

![All measured compiler versions](benchmark.svg)

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="large-projects-dark.svg">
  <img alt="Type-check speedup against peak-memory ratio" src="large-projects.svg">
</picture>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="per-project-time-dark.svg">
  <img alt="Check time by project" src="per-project-time.svg">
</picture>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="per-project-memory-dark.svg">
  <img alt="Peak memory by project" src="per-project-memory.svg">
</picture>

| Project | Compiler | Wall time | Peak memory | Instructions | Files | Diagnostics |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Compiler-Unions | tsrs | 0.101 s | 0.080 GiB | 3,323,459,516 | 48 | 41 |
| Compiler-Unions | TypeScript 7.1.0-dev.20261022.1 | 0.499 s | 0.148 GiB | 10,565,114,173 | 48 | 41 |
| Compiler-Unions | TypeScript 7.0.2 | 0.581 s | 0.236 GiB | 15,789,717,156 | 48 | 41 |
| Compiler | tsrs | 0.072 s | 0.077 GiB | 1,757,741,384 | 53 | 43 |
| Compiler | TypeScript 7.1.0-dev.20261022.1 | 0.164 s | 0.132 GiB | 4,551,098,923 | 53 | 43 |
| Compiler | TypeScript 7.0.2 | 0.281 s | 0.211 GiB | 8,391,234,737 | 53 | 43 |
| cal-diy | tsrs | 1.526 s | 1.119 GiB | 42,148,694,369 | 10173 | 136 |
| cal-diy | TypeScript 7.1.0-dev.20261022.1 | 4.042 s | 2.958 GiB | 145,503,417,073 | 10173 | 136 |
| cal-diy | TypeScript 7.0.2 | 6.804 s | 4.632 GiB | 220,167,722,377 | 10166 | 136 |
| date-fns | tsrs | 0.100 s | 0.139 GiB | 1,486,666,017 | 2277 | 1 |
| date-fns | TypeScript 7.1.0-dev.20261022.1 | 0.282 s | 0.240 GiB | 5,952,947,300 | 2277 | 1 |
| date-fns | TypeScript 7.0.2 | 0.312 s | 0.286 GiB | 6,801,435,164 | 2270 | 1 |
| drizzle-orm | tsrs | 0.397 s | 0.472 GiB | 11,634,454,690 | 3476 | 10846 |
| drizzle-orm | TypeScript 7.1.0-dev.20261022.1 | 2.057 s | 1.255 GiB | 47,769,566,598 | 3476 | 10845 |
| drizzle-orm | TypeScript 7.0.2 | 2.160 s | 1.741 GiB | 67,388,709,538 | 3469 | 10845 |
| formbricks-web | tsrs | 1.107 s | 1.398 GiB | 43,907,444,555 | 10458 | 0 |
| formbricks-web | TypeScript 7.1.0-dev.20261022.1 | 4.319 s | 4.209 GiB | 170,020,434,119 | 10458 | 0 |
| formbricks-web | TypeScript 7.0.2 | 5.754 s | 5.732 GiB | 230,593,569,288 | 10451 | 0 |
| mikro-orm | tsrs | 1.809 s | 1.369 GiB | 62,529,415,195 | 2911 | 43651 |
| mikro-orm | TypeScript 7.1.0-dev.20261022.1 | 8.604 s | 3.848 GiB | 257,689,866,369 | 2911 | 43651 |
| mikro-orm | TypeScript 7.0.2 | 12.557 s | 6.748 GiB | 399,240,035,807 | 2911 | 43651 |
| mui-docs | tsrs | 1.473 s | 1.012 GiB | 51,021,594,226 | 16766 | 0 |
| mui-docs | TypeScript 7.1.0-dev.20261022.1 | 6.557 s | 3.314 GiB | 216,305,501,504 | 16766 | 0 |
| mui-docs | TypeScript 7.0.2 | 16.776 s | 7.794 GiB | 562,094,200,726 | 16766 | 0 |
| next-packages-next | tsrs | 0.438 s | 0.514 GiB | 16,380,671,224 | 2889 | 6 |
| next-packages-next | TypeScript 7.1.0-dev.20261022.1 | 2.307 s | 1.264 GiB | 61,676,394,035 | 2889 | 6 |
| next-packages-next | TypeScript 7.0.2 | 2.698 s | 1.783 GiB | 82,138,826,900 | 2882 | 6 |
| next-root | tsrs | 0.405 s | 0.367 GiB | 11,856,012,023 | 3555 | 438 |
| next-root | TypeScript 7.1.0-dev.20261022.1 | 1.491 s | 0.942 GiB | 40,368,319,311 | 3555 | 438 |
| next-root | TypeScript 7.0.2 | 2.184 s | 1.294 GiB | 52,662,466,838 | 3548 | 441 |
| nuxt | tsrs | 0.301 s | 0.353 GiB | 9,558,531,511 | 3705 | 1 |
| nuxt | TypeScript 7.1.0-dev.20261022.1 | 1.068 s | 0.920 GiB | 36,671,612,438 | 3705 | 1 |
| nuxt | TypeScript 7.0.2 | 1.343 s | 1.144 GiB | 47,260,259,405 | 3705 | 3 |
| playwright | tsrs | 0.238 s | 0.312 GiB | 8,220,320,348 | 1514 | 13 |
| playwright | TypeScript 7.1.0-dev.20261022.1 | 0.902 s | 0.770 GiB | 31,660,305,183 | 1514 | 13 |
| playwright | TypeScript 7.0.2 | 1.132 s | 1.078 GiB | 44,667,487,860 | 1507 | 13 |
| storybook | tsrs | 0.436 s | 0.367 GiB | 10,931,921,820 | 3548 | 93 |
| storybook | TypeScript 7.1.0-dev.20261022.1 | 1.273 s | 0.958 GiB | 40,301,688,862 | 3548 | 93 |
| storybook | TypeScript 7.0.2 | 1.629 s | 1.395 GiB | 61,892,708,308 | 3541 | 93 |
| supabase-studio | tsrs | 1.337 s | 1.145 GiB | 50,295,747,874 | 12553 | 9 |
| supabase-studio | TypeScript 7.1.0-dev.20261022.1 | 5.749 s | 3.125 GiB | 185,951,688,548 | 12553 | 9 |
| supabase-studio | TypeScript 7.0.2 | 6.427 s | 4.817 GiB | 271,232,291,498 | 12546 | 0 |
| t3code-server | tsrs | 2.150 s | 1.132 GiB | 60,344,727,941 | 2948 | 6 |
| t3code-server | TypeScript 7.1.0-dev.20261022.1 | 11.365 s | 4.354 GiB | 353,179,083,135 | 2948 | 6 |
| t3code-server | TypeScript 7.0.2 | 17.552 s | 6.450 GiB | 469,614,530,938 | 2941 | 5 |
| typeorm | tsrs | 0.265 s | 0.214 GiB | 5,934,124,153 | 3575 | 31 |
| typeorm | TypeScript 7.1.0-dev.20261022.1 | 0.823 s | 0.447 GiB | 18,242,275,081 | 3575 | 31 |
| typeorm | TypeScript 7.0.2 | 1.133 s | 0.774 GiB | 28,410,129,146 | 3575 | 31 |
| vscode | tsrs | 2.489 s | 1.741 GiB | 74,987,266,861 | 10427 | 371 |
| vscode | TypeScript 7.1.0-dev.20261022.1 | 8.423 s | 5.333 GiB | 240,116,102,924 | 10427 | 371 |
| vscode | TypeScript 7.0.2 | 11.110 s | 7.523 GiB | 314,296,044,040 | 10427 | 359 |
| webpack | tsrs | 0.364 s | 0.366 GiB | 10,723,844,614 | 1607 | 840 |
| webpack | TypeScript 7.1.0-dev.20261022.1 | 1.052 s | 0.836 GiB | 35,177,078,854 | 1607 | 840 |
| webpack | TypeScript 7.0.2 | 1.414 s | 1.173 GiB | 46,653,285,574 | 1607 | 848 |
| xstate-main | tsrs | 0.176 s | 0.222 GiB | 5,892,234,026 | 1539 | 0 |
| xstate-main | TypeScript 7.1.0-dev.20261022.1 | 0.826 s | 0.550 GiB | 21,290,557,289 | 1539 | 0 |
| xstate-main | TypeScript 7.0.2 | 0.921 s | 0.732 GiB | 27,509,089,210 | 1532 | 0 |

File counts and diagnostics were collected in separate, untimed runs.
