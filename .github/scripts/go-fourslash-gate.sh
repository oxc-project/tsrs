#!/usr/bin/env bash
# Keep the minimum in sync with docs/COMPATIBILITY.md. Skipped-only parents do not count as coverage.
set -euo pipefail

work="${TSRS_GO_FOURSLASH_WORK:-$PWD/target/go-fourslash}"
node="${NODE:-node}"
mkdir -p "$work"

status=0
"$node" tools/go-fourslash/run.mjs --work "$work" \
  --server "${TSRS_FOURSLASH_SERVER:-target/debug/fourslash_server}" \
  -- -json > "$work/results.jsonl" 2> "$work/runner.log" || status=$?
cat "$work/runner.log"
"$node" tools/go-fourslash/summarize.mjs "$work/results.jsonl" --min-pass "${MIN_PASS:-4092}"
exit "$status"
