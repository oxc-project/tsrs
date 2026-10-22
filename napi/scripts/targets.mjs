// Oxc's native macOS, Linux and Windows targets. This table drives builds and artifact validation.
export const targets = [
  ['aarch64-apple-darwin', 'darwin-arm64', 'macos-latest', '', true],
  ['x86_64-apple-darwin', 'darwin-x64', 'macos-latest', '', false],
  ['x86_64-pc-windows-msvc', 'win32-x64-msvc', 'windows-latest', '', true],
  ['aarch64-pc-windows-msvc', 'win32-arm64-msvc', 'windows-latest', '', false],
  ['i686-pc-windows-msvc', 'win32-ia32-msvc', 'windows-latest', '', false],
  ['x86_64-unknown-linux-gnu', 'linux-x64-gnu', 'ubuntu-latest', '--use-napi-cross', true],
  ['aarch64-unknown-linux-gnu', 'linux-arm64-gnu', 'ubuntu-latest', '--use-napi-cross', false],
  ['armv7-unknown-linux-gnueabihf', 'linux-arm-gnueabihf', 'ubuntu-latest', '--use-napi-cross', false],
  ['powerpc64le-unknown-linux-gnu', 'linux-ppc64-gnu', 'ubuntu-latest', '--use-napi-cross', false],
  ['s390x-unknown-linux-gnu', 'linux-s390x-gnu', 'ubuntu-latest', '--use-napi-cross', false],
  ['riscv64gc-unknown-linux-gnu', 'linux-riscv64-gnu', 'ubuntu-latest', '', false],
  ['x86_64-unknown-linux-musl', 'linux-x64-musl', 'ubuntu-latest', '-x', false],
  ['aarch64-unknown-linux-musl', 'linux-arm64-musl', 'ubuntu-latest', '-x', false],
  ['armv7-unknown-linux-musleabihf', 'linux-arm-musleabihf', 'ubuntu-latest', '-x', false],
  ['riscv64gc-unknown-linux-musl', 'linux-riscv64-musl', 'ubuntu-latest', '-x', false],
].map(([target, suffix, os, flags, test]) => ({ target, suffix, os, flags, test }));

export function validateTargets(configured) {
  if (JSON.stringify([...configured].sort()) !== JSON.stringify(targets.map(t => t.target).sort())) {
    throw new Error('napi.targets must match the release target table');
  }
}
