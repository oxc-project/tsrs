import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { chmod, mkdtemp, mkdir, readFile, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { cacheKeys, catalog, selectProjects, writeJson } from './lib.mjs';
import { loadBundle, mergeBundles } from './results.mjs';
import { render } from './render.mjs';
import { watchSession } from './special.mjs';

const temp = () => mkdtemp(path.join(os.tmpdir(), 'tsrs-bench-test-'));
const sample = (wall, instructions, memory) => ({ time_wall_clock: { value: wall, unit: 'second' }, instructions: { value: instructions, unit: null }, memory_peak_resident: { value: memory, unit: 'byte' }, exit_code: 0 });

async function fixture(root, project = 'Compiler', wall = 2, { at = '2026-10-20T00:00:00Z', instructions = 100000, revision = 'fixture-old' } = {}) {
  await mkdir(path.join(root, project), { recursive: true });
  const commands = { tsrs: ['/test/tsrs', '-p', '/test/tsconfig.json'], typescript_1: ['/test/tsgo', '-p', '/test/tsconfig.json'] };
  const result = [{ name: 'tsrs', command: commands.tsrs.map(x => `'${x}'`).join(' '), measurements: [sample(wall, instructions, 200000)] }, { name: 'typescript_1', command: commands.typescript_1.map(x => `'${x}'`).join(' '), measurements: [sample(wall * 2, 200000, 400000)] }];
  await writeJson(path.join(root, project, 'hyperfine.json'), { schema_version: 2, results: result });
  const run = { schemaVersion: 2, kind: 'compile', status: 'complete', startedAt: at, completedAt: at, repositoryRevision: revision, rustc: 'fixture rustc', machine: { label: 'fixture', cpu: 'fixture', libc: 'glibc 1' }, suite: { repo: 'fixture', commit: 'abc' }, projects: [{ name: project, commit: 'abc' }], compilers: { tsrs: { version: '1', label: 'tsrs', sha256: 'a' }, typescript_1: { version: '7', label: 'TypeScript', sha256: 'b' } }, settings: { checkers: 4, workers: 8, memoryGiB: null, runs: 1, warmup: 0 }, hyperfine: { version: '2', sha256: 'c' }, cells: [{ project, cwd: '/test', commands }], preflight: { [project]: { tsrs: { exitCode: 0, files: 10, errors: 0 }, typescript_1: { exitCode: 0, files: 10, errors: 0 } } }, samples: [{ project, compiler: 'tsrs', repetition: 1, wallSeconds: wall, instructions, peakRssBytes: 200000, exitCode: 0 }, { project, compiler: 'typescript_1', repetition: 1, wallSeconds: wall * 2, instructions: 200000, peakRssBytes: 400000, exitCode: 0 }] };
  await writeJson(path.join(root, 'run.json'), run);
  return run;
}

test('project planning is stable and rejects duplicates', async () => {
  const cfg = await catalog();
  const projects = selectProjects(cfg, 'Compiler,Compiler-Unions');
  assert.deepEqual(projects.map(p => p.name), ['Compiler', 'Compiler-Unions']);
  assert.deepEqual(await cacheKeys(cfg, projects), await cacheKeys(cfg, projects));
  assert.throws(() => selectProjects(cfg, 'Compiler,Compiler'));
});

test('saved bundle is checked against raw Hyperfine data', async () => {
  const source = await temp();
  await fixture(source);
  assert.equal((await loadBundle(source)).samples.length, 2);
  const raw = path.join(source, 'Compiler', 'hyperfine.json');
  const data = JSON.parse(await readFile(raw)); data.results[0].measurements[0].instructions.value++;
  await writeJson(raw, data);
  await assert.rejects(loadBundle(source), /samples differ/);
});

test('publication SVGs reproduce from committed chart data', async () => {
  const source = await temp(), first = await temp(), second = await temp();
  await fixture(source);
  await render(source, first);
  await render(path.join(first, 'publication.json'), second);
  for (const name of ['benchmark.svg', 'large-projects.svg', 'large-projects-dark.svg', 'per-project-time.svg', 'per-project-time-dark.svg', 'per-project-memory.svg', 'per-project-memory-dark.svg', 'RESULTS.md']) {
    assert.equal(await readFile(path.join(first, name), 'utf8'), await readFile(path.join(second, name), 'utf8'));
  }
  assert.match(await readFile(path.join(first, 'benchmark.svg'), 'utf8'), /<title id="title">/);
  assert.doesNotMatch(await readFile(path.join(first, 'benchmark.svg'), 'utf8'), /<image\b|<text\b|foreignObject/);
});

test('merge retains samples and rejects duplicate projects', async () => {
  const first = await temp(), second = await temp(), dest = path.join(await temp(), 'merged');
  await fixture(first, 'Compiler'); await fixture(second, 'Compiler-Unions');
  const merged = await mergeBundles([first, second], dest);
  assert.equal((await loadBundle(merged)).samples.length, 4);
  await assert.rejects(mergeBundles([first, first], path.join(await temp(), 'duplicate')), /Duplicate project/);
});

test('watch restores its source file after edits and after a timeout', async () => {
  const root = await temp(), leaf = path.join(root, 'leaf.ts'), exe = path.join(root, 'watcher');
  await writeFile(leaf, 'export const value = 1;\n');
  await writeFile(exe, `#!/usr/bin/env node\nconst fs = require('fs');\nconst file = process.argv[process.argv.indexOf('-p') + 1];\nfunction report() { console.log('Found ' + (fs.readFileSync(file, 'utf8').includes('tsrsBenchWatchError') ? 1 : 0) + ' errors. Watching for file changes.'); }\nreport(); fs.watchFile(file, { interval: 10 }, report);\nprocess.on('SIGINT', () => process.exit(0));\n`);
  await chmod(exe, 0o755);
  const good = await watchSession(exe, root, leaf, leaf, { edits: 1, quiet: 0.05, timeout: 3 }, path.join(root, 'good.log'));
  assert.equal(good.ok, true, good.failure);
  assert.equal(await readFile(leaf, 'utf8'), 'export const value = 1;\n');
  const silent = path.join(root, 'silent');
  await writeFile(silent, '#!/usr/bin/env node\nsetInterval(() => {}, 1000); process.on("SIGINT", () => process.exit(0));\n');
  await chmod(silent, 0o755);
  const bad = await watchSession(silent, root, leaf, leaf, { edits: 1, quiet: 0.05, timeout: 0.1 }, path.join(root, 'bad.log'));
  assert.equal(bad.ok, false);
  assert.equal(await readFile(leaf, 'utf8'), 'export const value = 1;\n');
});

test('history and regression reports compare compatible run bundles', async () => {
  const root = await temp();
  await fixture(path.join(root, 'old'), 'Compiler', 2, { at: '2026-10-20T00:00:00Z', revision: 'fixture-old' });
  await fixture(path.join(root, 'new'), 'Compiler', 2, { at: '2026-10-21T00:00:00Z', revision: 'fixture-new', instructions: 102000 });
  const cli = path.join(import.meta.dirname, 'bench.mjs');
  const history = execFileSync(process.execPath, [cli, 'history', '--results-dir', root, '--all', '--metrics', 'instructions'], { encoding: 'utf8' });
  assert.match(history, /fixture-new.*102000 \(\+2\.0%!\)/);
  const regression = execFileSync(process.execPath, [cli, 'regressions', '--results-dir', root], { encoding: 'utf8' });
  assert.match(regression, /Compiler \| instructions \| 100000 \| 102000 \| 2\.00% \| regression/);
});
