#!/usr/bin/env bash
# Compare every supported compiler baseline against the pinned upstream fixtures.
set -euo pipefail
cd "$(dirname "$0")/../.."

export TSRS_TESTDATA="${TSRS_TESTDATA:-$PWD/ts-ref/tsc/testdata}"
export TSRS_TEST_RESULTS="${TSRS_TEST_RESULTS:-$PWD/target/test-results}"
commit="$(node npm/build.mjs --print-typescript-commit)"
if [ "$(git -C "$TSRS_TESTDATA" rev-parse HEAD)" != "$commit" ]; then
  echo "::error::testdata checkout does not match the TypeScript source pin"
  exit 1
fi

# tsrs-test fails on any difference, crash, timeout, or empty selection.
exec "${TSRS_TEST:-target/debug/tsrs-test}" run --suite all --baselines all --timeout "${TEST_TIMEOUT:-120}"
