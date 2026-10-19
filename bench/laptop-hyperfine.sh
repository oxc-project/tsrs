#!/usr/bin/env bash
# Execute the Bash runner prepared by bench/run.py --profile laptop --setup-only.
set -euo pipefail
bench_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
bench_work=${BENCH_WORK:-"$bench_dir/.work/laptop"}
if [[ ! -f "$bench_work/run.sh" ]]; then
  printf 'Prepare the benchmark first:\n  python3 %q --profile laptop --setup-only --work-dir %q\n' \
    "$bench_dir/run.py" "$bench_work" >&2
  exit 1
fi
exec bash "$bench_work/run.sh" "$@"
