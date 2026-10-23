# Compact symbol experiment results

Percentages are medians of within-pair candidate/baseline changes; negative means lower. All measured commands disable emit and incremental writes.

Mode `1` uses `--checkers 1` with parallel parsing/binding; `single` uses `--singleThreaded`; `default` uses the default checker count. Each binary has two untimed warmups in each cell. The baseline is `69c9ac78`; builds use release, full debug information and no stripping.

## 16-byte catalog screen

| Project | Mode | Pairs | Wall % | Instructions % | Peak RSS % |
| --- | --- | ---: | ---: | ---: | ---: |
| vscode | 1 | 3 | -1.01 | 0.05 | -0.72 |
| vscode | default | 3 | -0.87 | 0.08 | -2.39 |
| xstate-main | 1 | 3 | 0.09 | -0.05 | -1.36 |
| xstate-main | default | 3 | -0.62 | 0.10 | -1.84 |
| webpack | 1 | 3 | -0.08 | 0.16 | -1.47 |
| webpack | default | 3 | 1.11 | 0.32 | -2.03 |
| mui-docs | 1 | 3 | -2.70 | 0.36 | -0.98 |
| mui-docs | default | 3 | -0.06 | -0.27 | -2.86 |
| Compiler | 1 | 3 | -1.95 | -0.01 | -0.64 |
| Compiler | default | 3 | -0.89 | 0.11 | -0.76 |
| Compiler-Unions | 1 | 3 | -2.68 | 0.12 | -0.34 |
| Compiler-Unions | default | 3 | 1.48 | 0.24 | -0.66 |
| cal-diy | 1 | 3 | -0.30 | 0.21 | -0.99 |
| cal-diy | default | 3 | -0.66 | 0.96 | -2.42 |
| formbricks-web | 1 | 3 | 0.01 | 0.29 | -1.18 |
| formbricks-web | default | 3 | -0.18 | 0.24 | -1.61 |
| supabase-studio | 1 | 3 | -0.45 | 0.12 | 0.15 |
| supabase-studio | default | 3 | -0.87 | -0.06 | -1.68 |
| t3code-server | 1 | 3 | -0.73 | 0.02 | -2.35 |
| t3code-server | default | 3 | 1.24 | 0.37 | -2.34 |
| mikro-orm | 1 | 3 | -0.35 | -0.05 | -2.48 |
| mikro-orm | default | 3 | -5.47 | -1.31 | -3.04 |
| next-packages-next | 1 | 3 | -2.55 | -0.01 | -1.58 |
| next-packages-next | default | 3 | 0.31 | -1.14 | -3.61 |
| next-root | 1 | 3 | 1.87 | 0.62 | -1.02 |
| next-root | default | 3 | 1.17 | 0.77 | -1.01 |
| storybook | 1 | 3 | -0.51 | 0.17 | -1.20 |
| storybook | default | 3 | -1.08 | 1.17 | -0.76 |
| nuxt | 1 | 3 | 0.41 | -0.15 | -0.76 |
| nuxt | default | 3 | -8.86 | -0.62 | -2.04 |
| playwright | 1 | 3 | -0.18 | 0.04 | -1.39 |
| playwright | default | 3 | 2.89 | 1.93 | -0.69 |
| drizzle-orm | 1 | 3 | -0.42 | 0.04 | -0.04 |
| drizzle-orm | default | 3 | 3.39 | 0.03 | 0.10 |

## 16-byte confirmation

| Project | Mode | Pairs | Wall % | Instructions % | Peak RSS % |
| --- | --- | ---: | ---: | ---: | ---: |
| mui-docs | 1 | 10 | 0.23 | 0.70 | -0.65 |
| mui-docs | default | 10 | -0.21 | -0.30 | -2.44 |
| mikro-orm | 1 | 10 | -0.55 | 0.22 | -2.48 |
| mikro-orm | default | 10 | 1.64 | 0.83 | -1.56 |
| next-root | 1 | 10 | 1.20 | 0.04 | -1.20 |
| next-root | default | 10 | -0.51 | -0.55 | -1.53 |
| storybook | 1 | 10 | -0.54 | 0.10 | -1.27 |
| storybook | default | 10 | -0.74 | -0.27 | -1.84 |
| nuxt | 1 | 10 | 0.01 | 0.12 | -0.61 |
| nuxt | default | 10 | -0.18 | -1.66 | -1.85 |
| playwright | 1 | 10 | -0.77 | 0.14 | -1.58 |
| playwright | default | 10 | -1.77 | 0.59 | -1.71 |
| drizzle-orm | 1 | 10 | -0.66 | -0.25 | -0.10 |
| drizzle-orm | default | 10 | -0.03 | -0.13 | 0.07 |

## 16-byte single-threaded screen

| Project | Mode | Pairs | Wall % | Instructions % | Peak RSS % |
| --- | --- | ---: | ---: | ---: | ---: |
| vscode | single | 3 | -0.27 | 0.16 | -0.90 |
| mui-docs | single | 3 | -0.50 | -0.96 | -1.13 |
| Compiler | single | 3 | 2.95 | 0.15 | -0.66 |
| t3code-server | single | 3 | -2.34 | 0.18 | -1.69 |

## 24-byte catalog screen

| Project | Mode | Pairs | Wall % | Instructions % | Peak RSS % |
| --- | --- | ---: | ---: | ---: | ---: |
| vscode | single | 3 | -0.05 | 0.76 | -0.45 |
| vscode | default | 3 | -0.60 | -0.04 | 0.04 |
| xstate-main | single | 3 | -0.35 | -0.14 | -0.95 |
| xstate-main | default | 3 | 0.61 | -0.28 | -0.78 |
| webpack | single | 3 | 0.30 | -0.13 | -0.66 |
| webpack | default | 3 | -0.18 | -0.45 | -1.14 |
| mui-docs | single | 3 | -0.54 | -1.28 | 0.12 |
| mui-docs | default | 3 | -0.04 | -0.41 | -1.47 |
| Compiler | single | 3 | -0.24 | -0.12 | -0.22 |
| Compiler | default | 3 | -0.41 | -0.04 | -0.29 |
| Compiler-Unions | single | 3 | -1.10 | -0.10 | -0.21 |
| Compiler-Unions | default | 3 | -0.04 | -0.31 | -1.19 |
| cal-diy | single | 3 | 0.80 | 0.03 | -1.32 |
| cal-diy | default | 3 | 0.79 | -0.93 | -1.63 |
| formbricks-web | single | 3 | 0.54 | -0.09 | -1.08 |
| formbricks-web | default | 3 | -0.05 | 0.39 | -0.36 |
| supabase-studio | single | 3 | 0.03 | -0.08 | -0.74 |
| supabase-studio | default | 3 | -0.04 | -0.19 | -0.60 |
| t3code-server | single | 3 | 0.36 | -0.06 | -0.88 |
| t3code-server | default | 3 | 2.08 | 0.04 | -0.93 |
| mikro-orm | single | 3 | -2.44 | -0.13 | -1.61 |
| mikro-orm | default | 3 | 3.08 | 1.71 | 0.55 |
| next-packages-next | single | 3 | -0.09 | -0.07 | -0.99 |
| next-packages-next | default | 3 | -2.93 | -1.01 | -2.27 |
| next-root | single | 3 | -1.88 | -0.56 | -0.64 |
| next-root | default | 3 | -1.95 | 0.11 | -1.98 |
| storybook | single | 3 | -0.28 | 0.00 | -0.28 |
| storybook | default | 3 | 0.66 | -1.43 | -1.70 |
| nuxt | single | 3 | 0.43 | -0.17 | -0.62 |
| nuxt | default | 3 | 0.49 | 1.34 | -0.72 |
| playwright | single | 3 | 0.12 | -0.05 | -0.76 |
| playwright | default | 3 | -2.82 | 2.64 | 1.28 |
| drizzle-orm | single | 3 | -1.71 | 0.11 | 0.27 |
| drizzle-orm | default | 3 | 1.37 | 0.17 | -0.33 |

## 16-byte single-threaded confirmation

| Project | Mode | Pairs | Wall % | Instructions % | Peak RSS % |
| --- | --- | ---: | ---: | ---: | ---: |
| Compiler | single | 10 | -0.05 | 0.15 | -0.53 |
| t3code-server | single | 10 | -0.12 | 0.07 | -1.82 |

## 24-byte confirmation

| Project | Mode | Pairs | Wall % | Instructions % | Peak RSS % |
| --- | --- | ---: | ---: | ---: | ---: |
| t3code-server | default | 10 | 0.73 | -0.36 | -1.63 |
| mikro-orm | default | 10 | -4.48 | -0.72 | -1.71 |
| next-packages-next | default | 10 | 5.91 | 0.69 | -0.70 |
| playwright | default | 10 | -2.40 | 1.10 | -0.60 |
