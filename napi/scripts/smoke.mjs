// Install the packed release outside the workspace without contacting a registry.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { pnpm } from './release.mjs';

const dist = fileURLToPath(new URL('../dist/', import.meta.url));
const release = JSON.parse(fs.readFileSync(path.join(dist, 'packages.json'), 'utf8'));
const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'tsrs-napi-consumer-'));
try {
  const packages = Object.fromEntries(release.packages.map(p => [p.name, `file:${path.join(dist, p.tarball).replaceAll('\\', '/')}`]));
  const main = packages['@tsrs/napi'];
  assert.ok(main);
  delete packages['@tsrs/napi'];
  fs.writeFileSync(path.join(dir, 'package.json'), JSON.stringify({
    name: 'tsrs-napi-consumer', private: true, type: 'module',
    dependencies: { '@tsrs/napi': main }, optionalDependencies: packages,
  }));
  // Overrides resolve optional dependencies inside the main tarball to these exact local builds.
  // The standalone pnpm executable may be x64 while the consumer runs on ia32 Node.
  fs.writeFileSync(path.join(dir, 'pnpm-workspace.yaml'), `supportedArchitectures:\n  cpu: [${process.arch}]\n  os: [${process.platform}]\noverrides:\n${Object.entries(packages).map(([name, spec]) => `  ${JSON.stringify(name)}: ${JSON.stringify(spec)}`).join('\n')}\n`);
  pnpm(['install', '--offline', '--ignore-scripts'], dir);
  const pkg = JSON.parse(fs.readFileSync(path.join(dir, 'node_modules/@tsrs/napi/package.json'), 'utf8'));
  assert.equal(pkg.version, release.version);
  assert.deepEqual(Object.values(pkg.optionalDependencies), Object.keys(packages).map(() => release.version));
  assert.equal(fs.readdirSync(path.join(dir, 'node_modules/@tsrs/napi/src-js')).some(f => f.endsWith('.node')), false);
  fs.writeFileSync(path.join(dir, 'bad.ts'), 'export const value: number = "bad";');
  fs.writeFileSync(path.join(dir, 'tsconfig.json'), JSON.stringify({ compilerOptions: { strict: true, skipLibCheck: true }, files: ['bad.ts'] }));
  const typed = `import { check, checkSync, type CheckResult, type Diagnostic } from '@tsrs/napi';
const result: CheckResult = checkSync({ project: '.', cwd: '.' });
const pending: Promise<CheckResult> = check();
const diagnostics: Diagnostic[] = result.diagnostics;
void pending; void diagnostics;
// @ts-expect-error project must be a string
checkSync({ project: 1 });
`;
  fs.writeFileSync(path.join(dir, 'typed.mts'), typed);
  fs.writeFileSync(path.join(dir, 'typed.cts'), typed);
  fs.writeFileSync(path.join(dir, 'types.json'), JSON.stringify({ compilerOptions: { module: 'nodenext', target: 'es2022', strict: true, types: [] }, files: ['typed.mts', 'typed.cts'] }));
  fs.writeFileSync(path.join(dir, 'consumer.mjs'), `
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { check, checkSync } from '@tsrs/napi';
const cjs = createRequire(import.meta.url)('@tsrs/napi');
assert.equal(cjs.check, check);
const sync = cjs.checkSync();
assert.equal(sync.exitCode, 2);
assert.deepEqual(sync.diagnostics.map(d => d.code), [2322]);
assert.deepEqual(await check(), sync);
const types = checkSync({ project: 'types.json' });
assert.equal(types.exitCode, 0, JSON.stringify(types));
console.log('Installed NAPI consumer: ESM, CommonJS, declarations, sync and async passed');
`);
  execFileSync(process.execPath, ['consumer.mjs'], { cwd: dir, stdio: 'inherit', timeout: 120_000 });
} finally {
  fs.rmSync(dir, { recursive: true, force: true });
}
