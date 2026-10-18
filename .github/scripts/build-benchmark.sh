#!/usr/bin/env bash
# Build the same PGO + BOLT tsrs binary used by releases, and share one Bun canary across all measuring jobs.
set -euo pipefail

target=x86_64-unknown-linux-gnu
work="$RUNNER_TEMP/benchmark-build"
mkdir -p "$work/raw" "$work/dist" "$work/llvm"
rustup target add "$target"
python3 bench/run.py --setup-only --projects xstate-main,webpack

# Keep profile settings identical between builds so PGO matches their symbol names.
export CARGO_PROFILE_RELEASE_STRIP=none
RUSTFLAGS="-Cprofile-generate=$work/raw -Clto=thin" \
  cargo build --release --locked -p tsrs_cli -p tsrs_testrunner -p tsrs_fourslash --target "$target"
.github/scripts/pgo-train.sh "target/$target/release" "$work/raw" bench/.work

profdata="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/llvm-profdata"
"$profdata" merge -o "$work/merged.profdata" "$work"/raw/*.profraw
sum=$(shasum -a 256 "$work/merged.profdata" | cut -c1-16)
mv "$work/merged.profdata" "$work/$sum.profdata"
RUSTFLAGS="-Cprofile-use=$work/$sum.profdata -Clink-arg=-Wl,--emit-relocs" CARGO_TARGET_DIR="$work/target" \
  cargo build --release --locked -p tsrs_cli --target "$target"

llvm=$(rustc -vV | sed -n 's/^LLVM version: //p')
name="LLVM-$llvm-Linux-X64"
curl -fsSL --retry 3 --retry-all-errors \
  "https://github.com/llvm/llvm-project/releases/download/llvmorg-$llvm/$name.tar.zst" -o "$work/llvm.tar.zst"
tar -xf "$work/llvm.tar.zst" --zstd -C "$work/llvm" --strip-components=1 --wildcards \
  "$name/bin/llvm-bolt" "$name/bin/merge-fdata" "$name/lib/libbolt_rt_instr.a"
PATH="$work/llvm/bin:$PATH" BOLT_TSRS_ONLY=1 \
  .github/scripts/bolt.sh "$work/target/$target/release" "$work/bolt" bench/.work

cp "$work/target/$target/release/tsrs" "$work/dist/tsrs"
rustc -V > "$work/dist/rustc.txt"
curl -fsSL --retry 3 --retry-all-errors \
  https://github.com/oven-sh/bun/releases/download/canary/bun-linux-x64.zip -o "$work/bun.zip"
unzip -oq "$work/bun.zip" -d "$work"
cp "$work/bun-linux-x64/bun" "$work/dist/bun"
"$work/dist/tsrs" --version
"$work/dist/bun" --revision
"$work/dist/bun" check --help > /dev/null
sha256sum "$work/dist/tsrs" "$work/dist/bun"
