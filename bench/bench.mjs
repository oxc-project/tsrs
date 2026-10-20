#!/usr/bin/env node
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { BENCH, REPO, cacheKeys, catalog, cleanEnvironment, exec, fileSha, log, machine, now, parseArgs, prepare, projectPath, requireValue, selectProjects, stamp, values, writeJson } from './lib.mjs';
import { hyperfine, measureOne, runCompile } from './runner.mjs';
import { loadBundle, mergeBundles } from './results.mjs';

const usage = `Benchmark tools (Node 24)

  node bench/bench.mjs plan [--projects names] [--format json|keys]
  node bench/bench.mjs machine
  node bench/bench.mjs prepare [--projects names] [--work-dir path]
  node bench/bench.mjs run [--projects names] [--typescript-versions versions] [--tsrs binary | --tsrs-version tag]
                         [--checkers N] [--workers N] [--memory-gib N] [--runs N] [--warmup N]
                         [--work-dir path] [--out-dir path] [--hyperfine path] [--dry-run]
  node bench/bench.mjs merge <bundle...> --output-dir path
  node bench/bench.mjs render <bundle|publication.json> --output-dir path
  node bench/bench.mjs verify-images <publication.json> --against directory
  node bench/bench.mjs count --output-dir path [--hyperfine path] -- <command...>
  node bench/bench.mjs compare|watch|history|regressions [options]
`;

function integer(value, fallback, minimum = 1) {
  const n = value === undefined ? fallback : Number(value);
  requireValue(Number.isSafeInteger(n) && n >= minimum, `Expected an integer >= ${minimum}, got ${value}`);
  return n;
}

async function verifyImages(input, against) {
  requireValue(against, 'verify-images requires --against');
  const destination = path.join(BENCH, '.work', `verify-images-${stamp()}`);
  const { render } = await import('./render.mjs');
  await render(input, destination);
  const names = ['RESULTS.md', 'benchmark.svg', 'large-projects.svg', 'large-projects-dark.svg', 'per-project-time.svg', 'per-project-time-dark.svg', 'per-project-memory.svg', 'per-project-memory-dark.svg'];
  for (const name of names) requireValue(await fileSha(path.join(destination, name)) === await fileSha(path.join(against, name)), `Image reproduction differs: ${name}`);
  process.stdout.write(`Reproduced ${names.length - 1} SVGs and RESULTS.md from ${input}\n`);
}

async function count(options) {
  const argv = options._command;
  requireValue(argv?.length, 'count requires -- <command...>');
  requireValue(process.platform === 'linux', 'Instruction counting requires Linux');
  const work = path.resolve(options['work-dir'] ?? path.join(BENCH, '.work'));
  const out = path.resolve(options['output-dir'] ?? path.join(BENCH, 'results', stamp()));
  await mkdir(out, { recursive: true });
  const hf = await hyperfine(work, options.hyperfine);
  const result = await measureOne(hf, argv, { cwd: process.cwd(), env: process.env, output: path.join(out, 'hyperfine.json'), logFile: path.join(out, 'command.log'), showOutput: true });
  const run = { schemaVersion: 2, kind: 'count', status: 'complete', startedAt: now(), completedAt: now(), machine: machine(), command: argv, samples: [{ repetition: 1, ...result }], hyperfine: { binary: hf, sha256: await fileSha(hf) } };
  await writeJson(path.join(out, 'run.json'), run);
  process.stdout.write(`${out}/run.json\n`);
  process.exitCode = result.exitCode;
}

async function main() {
  const [command, ...args] = process.argv.slice(2);
  if (!command || ['help', '--help', '-h'].includes(command)) { process.stdout.write(usage); return; }
  if (command === 'machine') { process.stdout.write(JSON.stringify(machine()) + '\n'); return; }
  const { options, positional } = parseArgs(args, ['projects', 'repos', 'typescript-versions']);
  const work = path.resolve(options['work-dir'] ?? path.join(BENCH, '.work'));
  if (command === 'plan' || command === 'prepare' || command === 'run') {
    const cfg = await catalog();
    const projects = selectProjects(cfg, options.projects ?? options.repos);
    if (command === 'plan') {
      const keys = await cacheKeys(cfg, projects);
      if (options.format === 'keys') { for (const [key, value] of Object.entries(keys)) process.stdout.write(`${key}=${value}\n`); }
      else process.stdout.write(JSON.stringify({ projects: projects.map(p => p.name), cacheKeys: keys }) + '\n');
      return;
    }
    await prepare(cfg, projects, work);
    if (command === 'prepare') return;
    const versions = values(options['typescript-versions'], ['next']);
    requireValue(versions.length && new Set(versions).size === versions.length, 'Select distinct TypeScript versions');
    const source = await runCompile({ cfg, projects, work, out: path.resolve(options['out-dir'] ?? path.join(BENCH, 'results')), versions, tsrsVersion: options['tsrs-version'] ?? 'latest', localTsrs: options.tsrs, checkers: integer(options.checkers, 4), workers: integer(options.workers, machine().cpus), memoryGiB: options['memory-gib'] ? integer(options['memory-gib']) : null, runs: integer(options.runs ?? options.reps, 10), warmup: integer(options.warmup, 3, 0), hyperfinePath: options.hyperfine, dryRun: Boolean(options['dry-run']) });
    process.stdout.write(`${source}\n`);
    return;
  }
  if (command === 'merge') { await mergeBundles(positional.map(p => path.resolve(p)), path.resolve(options['output-dir'] ?? '')); return; }
  if (command === 'render') { requireValue(positional.length === 1, 'render requires a bundle or publication.json'); const { render } = await import('./render.mjs'); await render(path.resolve(positional[0]), path.resolve(options['output-dir'] ?? BENCH), { commit: options.commit, runUrl: options['run-url'] }); return; }
  if (command === 'verify-images') { requireValue(positional.length === 1, 'verify-images requires publication.json'); await verifyImages(path.resolve(positional[0]), options.against && path.resolve(options.against)); return; }
  if (command === 'count') { await count(options); return; }
  if (['compare', 'watch', 'history', 'regressions'].includes(command)) { const special = await import('./special.mjs'); await special[command](options, positional); return; }
  throw new Error(`Unknown command ${command}\n${usage}`);
}

main().catch(error => { process.stderr.write(`bench: ${error.stack ?? error}\n`); process.exitCode = 1; });
