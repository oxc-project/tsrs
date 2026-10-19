#!/usr/bin/env bash
# Full upstream baselines, checked against tools/conformance/known-mismatches.json.
# CI, releases and local runs all check normal tsrs behavior.
# TSRS_TEST, TSRS_TEST_RESULTS, TEST_TIMEOUT and PYTHON override the binary, output directory, timeout and Python.
set -euo pipefail
exec "${PYTHON:-python3}" tools/conformance/gate.py run "$@"
