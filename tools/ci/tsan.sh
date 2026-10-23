#!/usr/bin/env bash
# ThreadSanitizer gate: tsrs built with `-Zsanitizer=thread` runs the multi-threaded paths, and any report fails it.
# `P<T>` is `Send`/`Sync` by decree (tsrs_core::ptr), so only a run like this checks that threads do not race on arena
# data. Workloads: type checks at 2 and 8 checkers, emit, `-b` (clean and incremental) on a generated project graph,
# `--watch` and `-b --watch` with edits, and a language-server session (tools/lsp-mem/lsp_mem.mjs).
#
#   tools/ci/tsan.sh <tsan tsrs binary> [--projects xstate-main,webpack] [--work-dir bench/.work]
#
# Build the binary with the system allocator (ThreadSanitizer cannot see mimalloc's reuse of memory across threads)
# and plain pointers (its shadow memory takes the address range of the compressed handles' reservation):
#
#   RUSTFLAGS=-Zsanitizer=thread cargo +nightly-2026-10-04 build --release -p tsrs_cli -Zbuild-std \
#     --features tsrs_core/plain-ptrs,tsrs_cli/system-alloc --target <host triple>
#
# Suppressions (tools/ci/tsan.supp) are for reports that are not tsrs races, each with its reason.
set -uo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
[ $# -ge 1 ] || { echo "usage: $0 <tsan tsrs binary> [--projects a,b] [--work-dir DIR]"; exit 2; }
tsrs="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"; shift
projects="xstate-main,webpack"; work="$repo/bench/.work"
while [ $# -gt 0 ]; do
  case "$1" in
    --projects) projects=$2; shift 2 ;;
    --work-dir) work="$(cd "$2" && pwd)"; shift 2 ;;
    *) echo "unknown argument $1"; exit 2 ;;
  esac
done
out="$(mktemp -d "${TMPDIR:-/tmp}/tsrs-tsan.XXXXXX")"
logs="$out/logs"; mkdir -p "$logs"
base_options="suppressions=$repo/tools/ci/tsan.supp halt_on_error=0 exitcode=0 history_size=4"
fail=0

# A gate that cannot fail gates nothing: the binary must run under ThreadSanitizer.
banner="$(TSAN_OPTIONS="verbosity=1" "$tsrs" --version 2>&1)"
if ! grep -q "Running under ThreadSanitizer" <<< "$banner"; then
  echo "FAIL: $tsrs is not built with -Zsanitizer=thread"; exit 2
fi

# run <name> <exit statuses> <command...>: runs under ThreadSanitizer, its reports in $logs/<name>.<pid>. A status not in
# the comma list fails the gate too: a workload that broke would otherwise pass without having run (type checks exit
# 2 when they report errors).
run() {
  local name=$1 expected=$2 allowed=",$2," t0 status; shift 2
  t0=$(date +%s)
  TSAN_OPTIONS="$base_options log_path=$logs/$name" "$@" > "$out/$name.out" 2>&1; status=$?
  local n; n=$(cat "$logs/$name".* 2>/dev/null | grep -c "WARNING: ThreadSanitizer")
  echo "$name: exit $status, $(( $(date +%s) - t0 )) s, $n reports"
  [ "$n" = 0 ] || fail=1
  if [[ "$allowed" != *",$status,"* ]]; then
    echo "FAIL $name: exit $status (expected $expected):"; tail -20 "$out/$name.out"; fail=1
  fi
}

project_dir() {
  node "$repo/tools/ci/workloads.mjs" project "$repo/bench/projects.json" "$1" "$work"
}

IFS=',' read -r -a names <<< "$projects"
for name in "${names[@]}"; do
  dir="$(project_dir "$name")"
  for n in 2 8; do run "check-$name-$n" 0,2 "$tsrs" -p "$dir" --noEmit --incremental false --pretty false --checkers "$n"; done
done
first="$(project_dir "${names[0]}")"
run "emit-${names[0]}" 0,2 "$tsrs" -p "$first" --noEmit false --declaration --sourceMap --outDir "$out/emit" \
  --incremental false --pretty false --checkers 8

# A generated graph of 12 composite projects (40 files each, layered references), so that `-b` builds several at once.
graph="$out/graph"
node "$repo/tools/ci/workloads.mjs" graph "$graph"
run build-clean 0 "$tsrs" -b "$graph" --pretty false
printf '\nexport const edited = 1;\n' >> "$graph/p0/src/f5.ts"
run build-incremental 0 "$tsrs" -b "$graph" --pretty false

# watch_session <file...> -- <command...>: starts a watch, appends a line to each file and restores it, each time
# waiting for the cycle, then stops it with SIGINT.
watch_session() {
  node "$repo/tools/ci/workloads.mjs" watch "$@"
}
run watch-build 0 watch_session "$graph/p0/src/f3.ts" "$graph/p5/src/f0.ts" "$graph/p11/src/f7.ts" -- \
  "$tsrs" -b "$graph" --watch --preserveWatchOutput --pretty false
lsp_file="$(cd "$first" && git ls-files '*.ts' | grep -v '\.d\.ts$' | while read -r f; do grep -qE '^[[:space:]]+return\b' "$f" && { echo "$f"; break; }; done)"
watch_file="$first/$lsp_file"
run watch 0 watch_session "$watch_file" "$watch_file" -- "$tsrs" --watch -p "$first" --noEmit --preserveWatchOutput --pretty false
run lsp 0 node "$repo/tools/lsp-mem/lsp_mem.mjs" --cmd "$tsrs --lsp -stdio" --project "$first" --file "$lsp_file" \
  --edits 40 --every 20 --completion --log "$out/lsp"

if [ "$fail" = 1 ]; then
  echo; echo "FAIL (logs and outputs kept in $out)"
  ls "$logs"/* > /dev/null 2>&1 && node "$repo/tools/ci/tsan_summary.mjs" "$logs"/*
  exit 1
fi
rm -rf "$out"
echo "ThreadSanitizer: no reports"
