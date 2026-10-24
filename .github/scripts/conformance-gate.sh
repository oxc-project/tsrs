#!/usr/bin/env bash
# Compare pinned upstream baselines in Go and CLI modes, then stress scheduling regressions.
set -euo pipefail
cd "$(dirname "$0")/../.."

export TSRS_TESTDATA="${TSRS_TESTDATA:-$PWD/ts-ref/tsc/testdata}"
export TSRS_TEST_RESULTS="${TSRS_TEST_RESULTS:-$PWD/target/test-results}"
commit="$(node npm/build.mjs --print-typescript-commit)"
if [ "$(git -C "$TSRS_TESTDATA" rev-parse HEAD)" != "$commit" ]; then
  echo "::error::testdata checkout does not match the TypeScript source pin"
  exit 1
fi

exec node tools/ci/conformance.mjs "$@"
