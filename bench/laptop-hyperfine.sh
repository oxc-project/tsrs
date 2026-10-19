#!/usr/bin/env bash
# Run the archived laptop benchmark commands using the existing installed packages and projects.
# Usage: bash bench/laptop-hyperfine.sh [--dry-run]
set -euo pipefail

dry_run=0
if [[ $# -gt 1 ]]; then
  printf 'Usage: %s [--dry-run]\n' "$0" >&2
  exit 2
fi
case "${1:-}" in
  '') ;;
  --dry-run) dry_run=1 ;;
  -h|--help)
    printf 'Usage: %s [--dry-run]\nPrint each Hyperfine command before running it; --dry-run only prints.\n' "$0"
    exit 0
    ;;
  *) printf 'Unknown argument: %s\n' "$1" >&2; exit 2 ;;
esac

bench_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
bench_work=${BENCH_WORK:-"$bench_dir/.work/laptop"}
bench_work=$(cd -- "$bench_work" && pwd)
bench_hf=${BENCH_HF:-"$bench_work/tools/hyperfine-2.0.0"}
bench_ts702="$bench_work/packages/typescript_7_0_2-7.0.2/node_modules/@typescript/typescript-darwin-arm64/lib/tsc"
bench_ts71="$bench_work/packages/typescript-7.1.0-dev.20261019.1/node_modules/@typescript/typescript-darwin-arm64/lib/tsc"
# The archived measurements used 0.17.0; the chart's 0.18.0 version is a display override.
bench_tsrs="$bench_work/packages/tsrs-0.17.0-ts7.1.0-dev.20260929/node_modules/@tsrs/darwin-arm64/tsrs"
projects=(vscode mui-docs webpack t3code-server)
configs=(src docs . apps/server)

for binary in "$bench_hf" "$bench_ts702" "$bench_ts71" "$bench_tsrs"; do
  if [[ ! -x "$binary" ]]; then
    printf 'Missing executable: %s\n' "$binary" >&2
    exit 1
  fi
done
for index in "${!projects[@]}"; do
  config="$bench_work/solutions/${projects[$index]}/${configs[$index]}"
  if [[ ! -d "$config" ]]; then
    printf 'Missing prepared project: %s\n' "$config" >&2
    exit 1
  fi
done

# Match the harness's environment cleanup without changing the invoking shell.
clean_env=(env -u GOGC -u GOMEMLIMIT -u GOMAXPROCS -u GODEBUG)
while IFS= read -r key; do
  case "$key" in
    TSRS_*|TS_TEST_*|RAYON_*|MIMALLOC_*|MALLOC_*|Malloc*) clean_env+=(-u "$key") ;;
  esac
done < <(compgen -e)
clean_env+=(GOMAXPROCS=14 RAYON_NUM_THREADS=14)

if (( dry_run )); then
  bench_out="$bench_work/manual-results.preview"
else
  bench_out=$(mktemp -d "$bench_work/manual-results.XXXXXX")
fi

command_string() {
  local quoted
  printf -v quoted '%q ' "$@"
  printf '%s' "${quoted% }"
}

for mode in aligned maximum; do
  checkers=4
  run_env=("${clean_env[@]}")
  if [[ "$mode" == maximum ]]; then
    checkers=14
    run_env+=(GOMEMLIMIT=32GiB)
  fi

  for index in "${!projects[@]}"; do
    project=${projects[$index]}
    project_root="$bench_work/solutions/$project"
    config="$project_root"
    if [[ "${configs[$index]}" != . ]]; then
      config+="/${configs[$index]}"
    fi
    flags=(-p "$config" --noEmit --incremental false --pretty false --checkers "$checkers")
    tsrs_command=("$bench_tsrs" "${flags[@]}")
    if [[ "$mode" == maximum ]]; then
      tsrs_command+=(--maxMemory 32G)
    fi

    command=(
      "$bench_hf" --shell=none --runs 10 --warmup 3
      --metrics time_wall_clock:s,instructions:B,memory_peak_resident:GiB
      --style basic --ignore-failure=1,2
      --output "$bench_out/$mode-$project.log"
      --export-json "$bench_out/$mode-$project.json"
      --command-name typescript_7_0_2 "$(command_string "$bench_ts702" "${flags[@]}")"
      --command-name typescript "$(command_string "$bench_ts71" "${flags[@]}")"
      --command-name tsrs "$(command_string "${tsrs_command[@]}")"
    )

    printf '\n# %s / %s\n' "$mode" "$project"
    printf '( cd %q && ' "$project_root"
    printf '%q ' "${run_env[@]}" "${command[@]}"
    printf ')\n'
    if (( ! dry_run )); then
      (cd -- "$project_root" && "${run_env[@]}" "${command[@]}")
    fi
  done
done

if (( ! dry_run )); then
  printf '\nResults: %s\n' "$bench_out"
fi
