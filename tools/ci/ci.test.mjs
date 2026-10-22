import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

const repo = path.resolve(import.meta.dirname, '../..');
const fixed = "a.ts(1,1): error TS2322: fixed diagnostic\n";
const old = fixed.replace('fixed', 'old');

function fixture(t, { baseline, actual = fixed, exit = 2, unstable = false, projectMismatch = false } = {}) {
  const root = mkdtempSync(path.join(os.tmpdir(), 'tsrs-ci-test-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const write = (file, text) => {
    mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    writeFileSync(path.join(root, file), text);
  };
  for (const file of ['determinism.sh', 'compare_diagnostics.py']) {
    write(`tools/ci/${file}`, '');
    copyFileSync(path.join(repo, 'tools/ci', file), path.join(root, 'tools/ci', file));
  }
  write('bench/projects.json', JSON.stringify({ projects: [{ name: 'xstate-main', project: '.' }] }));
  write('work/solutions/xstate-main/input.json', JSON.stringify({ actual: fixed, base: projectMismatch ? old : fixed, exit: 2 }));
  write('testdata/regressions/example/expected.txt', fixed);
  write('testdata/regressions/example/input.json', JSON.stringify({ actual, base: old, exit, unstable }));
  mkdirSync(path.join(root, 'base-source'), { recursive: true });
  if (baseline !== undefined) write('base-source/testdata/regressions/example/expected.txt', baseline);
  const compiler = `#!${process.execPath}
const fs = require('node:fs');
const path = require('node:path');
const input = JSON.parse(fs.readFileSync('input.json', 'utf8'));
const base = path.basename(process.argv[1]) === 'base';
const unstable = input.unstable && process.argv.includes('random:1');
process.stdout.write(base || unstable ? input.base : input.actual);
process.exit(input.exit);
`;
  for (const name of ['current', 'base']) {
    write(name, compiler);
    chmodSync(path.join(root, name), 0o755);
  }
  return spawnSync('bash', [path.join(root, 'tools/ci/determinism.sh'), path.join(root, 'current'),
    '--base', path.join(root, 'base'), '--base-source', path.join(root, 'base-source'),
    '--work-dir', path.join(root, 'work'), '--seeds', '1', '--default-reps', '1', '--projects', 'xstate-main'],
  { encoding: 'utf8' });
}

for (const [name, baseline] of [['new', undefined], ['changed', old]]) {
  test(`${name} regression follows expected.txt when the base still has the bug`, (t) => {
    const result = fixture(t, { baseline });
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.match(result.stdout, /new or changed regression matches expected.txt/);
  });
}

test('unchanged regression still compares against the base compiler', (t) => {
  const result = fixture(t, { baseline: fixed });
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stdout, /FAIL case-example: diagnostic line 1 differs/);
});

test('new regression must match its expected diagnostic baseline', (t) => {
  const result = fixture(t, { actual: old });
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stdout, /regression baseline does not match expected.txt/);
});

test('new regression cannot pass with a crash exit', (t) => {
  const result = fixture(t, { exit: 5 });
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stdout, /does not match expected.txt \(exit 5\)/);
});

test('new regression must remain identical under checker assignments', (t) => {
  const result = fixture(t, { unstable: true });
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stdout, /FAIL case-example random-2-1/);
});

test('benchmark diagnostics must still match the base compiler', (t) => {
  const result = fixture(t, { projectMismatch: true });
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stdout, /FAIL xstate-main: diagnostic line 1 differs/);
});

test('benchmark join reads the project catalog and retains valid project measurements', (t) => {
  const root = mkdtempSync(path.join(os.tmpdir(), 'tsrs-benchmark-join-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const data = { project: 'webpack', main: { median_s: 1 }, pr: { median_s: 2 } };
  writeFileSync(path.join(root, 'webpack.json'), JSON.stringify(data));
  writeFileSync(path.join(root, 'nuxt.json'), '{');
  const out = path.join(root, 'joined.json');
  const result = spawnSync('python3', [path.join(repo, 'bench/wallclock.py'), 'join', '--results', root, '--out', out], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.deepEqual(JSON.parse(readFileSync(out, 'utf8')), { projects: { webpack: data } });
});
