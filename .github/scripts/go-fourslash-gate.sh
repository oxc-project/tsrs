#!/usr/bin/env bash
# Exact reviewed mismatches and coverage; skipped-only parents do not count as passes.
set -euo pipefail

work="${TSRS_GO_FOURSLASH_WORK:-$PWD/target/go-fourslash}"
python="${PYTHON:-python3}"
mkdir -p "$work"

status=0
"$python" tools/go-fourslash/run.py --work "$work" \
  --server "${TSRS_FOURSLASH_SERVER:-target/debug/fourslash_server}" \
  -- -json > "$work/results.jsonl" 2> "$work/runner.log" || status=$?
cat "$work/runner.log"
"$python" tools/go-fourslash/summarize.py "$work/results.jsonl" --min-pass "${MIN_PASS:-4073}" \
  --runner-exit "$status" --known tools/go-fourslash/known-mismatches.json \
  --baselines "$work/reference/tsc/testdata/baselines"
