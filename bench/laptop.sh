#!/usr/bin/env bash
# Compatibility entry point; project setup and commands live in bench/run.py's laptop profile.
set -euo pipefail
bench_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
exec python3 "$bench_dir/run.py" --profile laptop "$@"
