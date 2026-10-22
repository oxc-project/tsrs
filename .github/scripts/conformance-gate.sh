#!/usr/bin/env bash
# Check the pinned compiler's named outcomes, including emit and source maps by default.
set -euo pipefail
exec node "$(dirname "$0")/conformance-gate.mjs" "$@"
