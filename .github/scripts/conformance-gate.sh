#!/usr/bin/env bash
# Full upstream baselines, checked against tools/conformance/known-mismatches.json.
# Default: Go-compatible mode (also used by release callers). Pass --mode default for production checker behavior.
# TSRS_TEST, TSRS_TEST_RESULTS, TEST_TIMEOUT and PYTHON override the binary, output directory, timeout and Python.
set -euo pipefail
exec "${PYTHON:-python3}" tools/conformance/gate.py run "$@"
