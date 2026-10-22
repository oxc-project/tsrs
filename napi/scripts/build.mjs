import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const cwd = fileURLToPath(new URL('..', import.meta.url));
const cli = path.resolve(path.dirname(require.resolve('@napi-rs/cli/package.json')), 'cli.mjs');
const result = spawnSync(process.execPath, [cli, 'build', '--platform', '--js', 'binding.cjs',
  '--dts', 'binding.d.cts', '--output-dir', 'src-js', ...process.argv.slice(2), '--', '--locked'],
{ cwd, stdio: 'inherit' });
if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
