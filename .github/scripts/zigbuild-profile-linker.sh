#!/usr/bin/env bash
# Linker for the PGO-instrumented musl build (release.yml build-musl, RUSTFLAGS -Clinker=<this>; notes/perf-musl-pgo.md).
# rustc links the profile runtime with `-u __llvm_profile_runtime`, and cargo-zigbuild 0.24.0's zig cc takes the symbol
# for an input file ("cannot open __llvm_profile_runtime"). Pass it as `-Wl,-u,<symbol>` to cargo-zigbuild's own linker
# wrapper, which cargo-zigbuild exports as CARGO_TARGET_<TRIPLE>_LINKER.
set -euo pipefail
linker=$(env | sed -n 's/^CARGO_TARGET_[A-Z0-9_]*_LINUX_MUSL_LINKER=//p')
[ -n "$linker" ] && [ "$(printf '%s\n' "$linker" | wc -l)" -eq 1 ] \
  || { echo "zigbuild-profile-linker: no single cargo-zigbuild musl linker in the environment" >&2; exit 1; }
args=()
while [ $# -gt 0 ]; do
  if [ "$1" = -u ] && [ $# -ge 2 ]; then args+=("-Wl,-u,$2"); shift 2; else args+=("$1"); shift; fi
done
exec "$linker" "${args[@]}"
