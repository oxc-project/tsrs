#!/usr/bin/env bash
# Keep the minimum in sync with tools/go-fourslash/README.md. Skipped-only parents do not count as coverage.
set -euo pipefail

work="${TSRS_GO_FOURSLASH_WORK:-$PWD/target/go-fourslash}"
python="${PYTHON:-python3}"
mkdir -p "$work"

status=0
"$python" tools/go-fourslash/run.py --work "$work" \
  --server "${TSRS_FOURSLASH_SERVER:-target/debug/fourslash_server}" \
  -- -json > "$work/results.jsonl" 2> "$work/runner.log" || status=$?
cat "$work/runner.log"
"$python" tools/go-fourslash/summarize.py "$work/results.jsonl" --min-pass "${MIN_PASS:-4074}"
exit "$status"
