import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { Worker } from 'node:worker_threads';
import { test } from 'node:test';
import { check, checkSync } from '../src-js/index.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const cli = process.env.TSRS_CLI ?? path.join(root, 'target/debug', process.platform === 'win32' ? 'tsrs.exe' : 'tsrs');
const require = createRequire(import.meta.url);

function project(t, files, options = {}) {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'tsrs-napi-'));
  t.after(() => fs.rmSync(cwd, { recursive: true, force: true }));
  fs.writeFileSync(path.join(cwd, 'tsconfig.json'), JSON.stringify({ compilerOptions: { strict: true, skipLibCheck: true, ...options } }));
  for (const [name, content] of Object.entries(files)) {
    const file = path.join(cwd, name);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, content);
  }
  return cwd;
}

function compareCli(options) {
  const result = checkSync(options);
  const oracle = spawnSync(cli, ['--noEmit', '-p', options.project ?? '.', '--pretty', 'false'], { cwd: options.cwd, encoding: 'utf8' });
  assert.ifError(oracle.error);
  assert.equal(result.exitCode, oracle.status, oracle.stdout + oracle.stderr);
  assert.deepEqual(result.diagnostics.map(d => d.code), [...oracle.stdout.matchAll(/error TS(\d+):/g)].map(m => Number(m[1])), oracle.stdout);
  return result;
}

test('ESM and CommonJS expose the same checking functions', () => {
  assert.equal(require('../src-js/index.cjs').check, check);
  assert.equal(require('../src-js/index.cjs').checkSync, checkSync);
});

test('sync and async checks match the CLI and read changed files', async t => {
  const cwd = project(t, { 'index.ts': 'export const n: number = "bad";' });
  const bad = compareCli({ cwd });
  assert.equal(bad.diagnostics[0].code, 2322);
  assert.deepEqual(await check({ cwd }), bad);
  fs.writeFileSync(path.join(cwd, 'index.ts'), 'export const n: number = 1;');
  assert.deepEqual(compareCli({ cwd }).diagnostics, []);
  assert.equal((await check({ cwd })).exitCode, 0);
  assert.equal(fs.existsSync(path.join(cwd, 'index.js')), false);
});

test('config errors, missing configs, syntax errors and empty projects match CLI', t => {
  for (const files of [
    { 'index.ts': 'let = ;' },
    { 'tsconfig.json': '{ broken' },
    { 'tsconfig.json': '{"compilerOptions":{"unknownOption":true},"files":[]}' },
    { 'tsconfig.json': '{"files":[]}' },
  ]) compareCli({ cwd: project(t, files) });
  compareCli({ cwd: project(t, { 'index.ts': '' }), project: 'missing.json' });
});

test('extends, references, module resolution and incremental runs follow CLI', t => {
  const cwd = project(t, {
    'base.json': '{"compilerOptions":{"strict":true,"incremental":true,"skipLibCheck":true}}',
    'tsconfig.json': '{"extends":"./base.json","files":["index.ts"],"references":[{"path":"./dep"}]}',
    'index.ts': 'import { value } from "./dep/value"; export const x: number = value;',
    'dep/tsconfig.json': '{"compilerOptions":{"composite":true},"files":["value.ts"]}',
    'dep/value.ts': 'export const value = "bad";',
  });
  compareCli({ cwd });
  compareCli({ cwd });
  assert.ok(fs.existsSync(path.join(cwd, 'tsconfig.tsbuildinfo')));
  fs.writeFileSync(path.join(cwd, 'dep/value.d.ts'), 'export declare const value: string;');
  compareCli({ cwd });
});

test('UTF-16 positions and captured trace output', t => {
  const cwd = project(t, { 'index.ts': 'const emoji = "😀"; const value: number = "bad";\n' }, { listFiles: true });
  const result = compareCli({ cwd });
  const d = result.diagnostics.find(d => d.code === 2322);
  assert.equal(d.pos, 'const emoji = "😀"; const '.length);
  assert.equal(d.startPosition.character, d.pos);
  assert.ok(result.stdout.length > 0);
  assert.equal(result.stderr, '');
});

test('extended diagnostics counters belong to one invocation', t => {
  const cwd = project(t, { 'index.ts': 'export const value = 1;' }, { extendedDiagnostics: true });
  const counts = () => checkSync({ cwd }).stdout.split('\n').filter(line => /:\s+\d+\s*$/.test(line));
  const first = counts();
  assert.ok(first.length > 0);
  assert.deepEqual(counts(), first);
});

test('concurrent checks and Node workers keep projects isolated', async t => {
  const good = project(t, { 'a.ts': 'export const x = 1;' });
  const bad = project(t, { 'a.ts': 'export const x: number = "bad";' }, { skipLibCheck: false });
  const before = process.cwd();
  const results = await Promise.all([check({ cwd: good }), check({ cwd: bad }), check({ cwd: good })]);
  assert.deepEqual(results.map(r => r.exitCode), [0, 2, 0]);
  assert.equal(process.cwd(), before);
  const worker = new Worker(new URL('./worker.mjs', import.meta.url), { workerData: { cwd: bad } });
  const result = await new Promise((resolve, reject) => {
    worker.once('message', resolve);
    worker.once('error', reject);
    worker.once('exit', code => { if (code) reject(new Error(`worker exited ${code}`)); });
  });
  assert.equal(result.diagnostics[0].code, 2322);
});

test('async checking leaves the event loop responsive', async t => {
  const cwd = project(t, { 'index.ts': 'export const x: number = "bad";' });
  let ticks = 0;
  const timer = setInterval(() => ticks++, 1);
  try { await check({ cwd }); } finally { clearInterval(timer); }
  assert.ok(ticks > 0);
});

test('invalid arguments fail before starting compilation', () => {
  for (const options of [null, [], 1, { cwd: 1 }, { project: '' }, { project: '\0' }]) {
    assert.throws(() => checkSync(options), TypeError);
    assert.throws(() => check(options), TypeError);
  }
});

test('repeated checks reclaim compiler state', t => {
  const cwd = project(t, { 'index.ts': 'export const n: number = "bad";' });
  execFileSync(process.execPath, [fileURLToPath(new URL('./memory.mjs', import.meta.url)), cwd], { timeout: 120_000, stdio: 'pipe' });
});
