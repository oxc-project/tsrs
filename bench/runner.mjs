import { chmod, mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { BENCH, REPO, TIMED_FLAGS, cleanEnvironment, copyFile, exec, fileSha, log, nativePlatform, now, projectPath, quote, readJson, requireValue, shellJoin, stamp, which, writeJson } from './lib.mjs';

const METRICS = 'time_wall_clock:s,instructions:B,memory_peak_resident:GiB';
const REGISTRY = 'https://registry.npmjs.org/';
const HF_VERSION = '2.0.0';
const HF_ARCHIVE_SHA256 = '590b839cfea386b384ce0b42d16daf2910277cf30950656194bb3a5f20dea81e';
const HF_BINARY_SHA256 = 'dc9f035feaa4f5e25985c0d36a9c1b60985ebd159652908f0ba65a31645ac8dc';

async function release(name, tag) {
  const response = await fetch(`${REGISTRY}${name}/${tag}`, { signal: AbortSignal.timeout(120000) });
  requireValue(response.ok, `npm metadata ${name}@${tag}: HTTP ${response.status}`);
  const value = await response.json();
  requireValue(value.name === name && value.version, `Invalid npm metadata for ${name}@${tag}`);
  return value;
}

export async function hyperfine(work, override) {
  let exe = override ? path.resolve(override) : which('hyperfine');
  if (exe && !override && nativePlatform() === 'darwin-arm64') {
    const help = await exec([exe, '--help']);
    if (!help.stdout.includes('--metrics')) exe = null;
  }
  if (!exe && nativePlatform() === 'darwin-arm64') {
    const archive = path.join(work, 'tools', 'hyperfine.tar.gz');
    exe = path.join(work, 'tools', `hyperfine-${HF_VERSION}`);
    const { exists, sha } = await import('./lib.mjs');
    if (!exists(exe) || await fileSha(exe) !== HF_BINARY_SHA256) {
      const url = `https://github.com/sharkdp/hyperfine/releases/download/v${HF_VERSION}/hyperfine-v${HF_VERSION}-aarch64-apple-darwin.tar.gz`;
      const response = await fetch(url, { signal: AbortSignal.timeout(120000) });
      requireValue(response.ok, `Hyperfine download: HTTP ${response.status}`);
      const bytes = Buffer.from(await response.arrayBuffer());
      requireValue(sha(bytes) === HF_ARCHIVE_SHA256, 'Hyperfine archive checksum mismatch');
      await mkdir(path.dirname(archive), { recursive: true }); await writeFile(archive, bytes);
      const member = (await exec(['tar', '-tzf', archive])).stdout.split('\n').find(name => name.endsWith('/hyperfine'));
      requireValue(member, 'Hyperfine binary absent from archive');
      await exec(['tar', '-xzf', archive, '-C', path.dirname(exe), '--strip-components=1', member]);
      await rename(path.join(path.dirname(exe), 'hyperfine'), exe);
      await chmod(exe, 0o755);
      requireValue(await fileSha(exe) === HF_BINARY_SHA256, 'Hyperfine binary checksum mismatch');
    }
  }
  requireValue(exe, 'Install Hyperfine 2 with hardware counters or pass --hyperfine');
  const help = await exec([exe, '--help']);
  requireValue(help.stdout.includes('--metrics'), 'Hyperfine 2 with hardware counters is required');
  return exe;
}

export async function installCompilers(work, source, versions, tsrsVersion, localTsrs) {
  const platform = nativePlatform();
  const native = `@typescript/typescript-${platform}`;
  const libc = process.platform === 'linux' && process.report?.getReport()?.header?.glibcVersionRuntime ? '-gnu' : '-musl';
  const tsrsPackage = `@tsrs/${platform}${process.platform === 'linux' ? libc : ''}`;
  const executables = {}, compilers = {}, packages = {};
  const resolved = new Set();
  const specs = versions.map((tag, i) => [`typescript_${i + 1}`, 'typescript', tag, 'TypeScript']);
  if (localTsrs !== false && !localTsrs) specs.push(['tsrs', tsrsPackage, tsrsVersion ?? 'latest', 'tsrs']);
  for (const [id, name, tag, label] of specs) {
    const rootPackage = await release(name, tag);
    let installed = rootPackage, all = [rootPackage];
    if (name === 'typescript') {
      requireValue(!resolved.has(rootPackage.version), `Duplicate TypeScript version ${rootPackage.version}`);
      resolved.add(rootPackage.version);
      const nativeVersion = rootPackage.optionalDependencies?.[native];
      requireValue(nativeVersion, `typescript@${tag} has no native ${platform} compiler`);
      installed = await release(native, nativeVersion); all = [rootPackage, installed];
    }
    const folder = path.join(work, 'packages', `${id}-${installed.version}`);
    await mkdir(folder, { recursive: true });
    await writeJson(path.join(folder, 'package.json'), { name: 'tsrs-benchmark', private: true });
    await exec(['npm', 'install', '--prefix', folder, '--registry', REGISTRY, '--cache', path.join(work, 'npm-cache'), '--ignore-scripts', '--no-audit', '--no-fund', '--save-exact', ...all.map(p => `${p.name}@${p.version}`)], { timeout: 1200, inherit: true });
    await copyFile(path.join(folder, 'package-lock.json'), path.join(source, `${id}-package-lock.json`));
    const exe = path.join(folder, 'node_modules', installed.name, name === 'typescript' ? 'lib/tsc' : 'tsrs');
    requireValue((await readJson(path.join(folder, 'node_modules', installed.name, 'package.json'))).version === installed.version, `Installed ${id} version mismatch`);
    executables[id] = exe; packages[id] = all;
    compilers[id] = { package: installed.name, version: installed.version, label, requested: `${name}@${tag}`, dist: installed.dist, binary: exe, sha256: await fileSha(exe), version_output: (await exec([exe, '--version'])).stdout.trim() };
  }
  if (localTsrs && localTsrs !== false) {
    const exe = path.resolve(localTsrs), output = (await exec([exe, '--version'])).stdout.trim();
    executables.tsrs = exe;
    compilers.tsrs = { version: output.replace(/^tsrs /, ''), label: 'tsrs', binary: exe, requested: exe, sha256: await fileSha(exe), version_output: output };
  }
  await writeJson(path.join(source, 'npm-packages.json'), packages);
  return { executables, compilers };
}

function metrics(sample, context) {
  const fields = { wallSeconds: ['time_wall_clock', 'second'], instructions: ['instructions', null], peakRssBytes: ['memory_peak_resident', 'byte'] };
  const out = {};
  for (const [dest, [source, unit]] of Object.entries(fields)) {
    const item = sample[source];
    requireValue((item?.unit ?? null) === unit && Number.isFinite(item.value) && item.value > 0, `${context}: invalid ${source}`);
    if (dest !== 'wallSeconds') requireValue(Number.isSafeInteger(item.value), `${context}: ${source} is not a safe integer`);
    out[dest] = item.value;
  }
  return out;
}

export function hyperfineResults(data, expected, repetitions, context) {
  requireValue(data.schema_version === 2, `${context}: unsupported Hyperfine schema`);
  const results = data.results;
  requireValue(Array.isArray(results) && results.length === expected.length && new Set(results.map(r => r.name)).size === expected.length && results.every(r => expected.includes(r.name)), `${context}: missing or duplicate compilers`);
  for (const result of results) {
    requireValue(result.measurements?.length === repetitions, `${context}: missing runs for ${result.name}`);
    for (const sample of result.measurements) {
      metrics(sample, context);
      requireValue([0, 1, 2].includes(sample.exit_code), `${context}: compiler failed`);
    }
  }
  return results;
}

async function preflight(command, cwd, env, logFile) {
  const result = await exec([...command, '--extendedDiagnostics'], { cwd, env, timeout: 900, logFile, accept: [0, 1, 2] });
  const output = result.stdout + result.stderr;
  requireValue(!/^(?:panic: |fatal error: |thread '.*' panicked at )/m.test(output), `Compiler crash: ${logFile}`);
  const files = Number(output.match(/^Files:\s+(\d+)/m)?.[1]);
  requireValue(files > 0, `Preflight returned no file count: ${logFile}`);
  const errors = {};
  for (const line of output.split('\n')) {
    const match = line.match(/^((?:\S.*\(\d+,\d+\): )?error TS\d+):/);
    if (match) errors[match[1]] = (errors[match[1]] ?? 0) + 1;
  }
  return { exitCode: result.code, files, errors: Object.values(errors).reduce((a, b) => a + b, 0), errorKeys: Object.fromEntries(Object.entries(errors).sort(([a], [b]) => a.localeCompare(b))) };
}

export async function measureOne(hf, argv, { cwd, env, output, logFile, name = 'command', accept = [0, 1, 2], showOutput = false } = {}) {
  const args = [hf, '--shell=none', '--runs', '1', '--warmup', '0', '--metrics', METRICS, '--style', 'none', `--ignore-failure=${accept.join(',')}`, '--export-json', output];
  if (showOutput) args.push('--show-output');
  args.push('--command-name', name, shellJoin(argv));
  await exec(args, { cwd, env, timeout: 900, logFile, inherit: showOutput });
  const result = hyperfineResults(await readJson(output), [name], 1, output)[0];
  const sample = result.measurements[0];
  return { ...metrics(sample, output), exitCode: sample.exit_code };
}

export async function runCompile({ cfg, projects, work, out, versions, tsrsVersion, localTsrs, checkers, workers, memoryGiB, runs, warmup, hyperfinePath, dryRun = false }) {
  requireValue(runs > 0 && warmup >= 0 && checkers > 0 && workers > 0 && (!memoryGiB || memoryGiB > 0), 'Invalid benchmark settings');
  const source = path.join(out, stamp()); await mkdir(source, { recursive: true });
  const hf = await hyperfine(work, hyperfinePath);
  const { executables, compilers } = await installCompilers(work, source, versions, tsrsVersion, localTsrs);
  const settings = { checkers, workers, memoryGiB: memoryGiB ?? null, runs, warmup };
  const { env, removed } = cleanEnvironment(workers, memoryGiB);
  const cells = projects.map(project => {
    const { cwd, config } = projectPath(project, work);
    const commands = Object.fromEntries(Object.entries(executables).map(([id, exe]) => [id, [exe, '-p', config, ...TIMED_FLAGS, '--checkers', String(checkers), ...(id === 'tsrs' && memoryGiB ? ['--maxMemory', `${memoryGiB}G`] : [])]]));
    return { project: project.name, cwd, commands, environment: { GOMAXPROCS: env.GOMAXPROCS, RAYON_NUM_THREADS: env.RAYON_NUM_THREADS, GOMEMLIMIT: env.GOMEMLIMIT ?? null } };
  });
  const machine = (await import('./lib.mjs')).machine();
  const run = { schemaVersion: 2, kind: 'compile', status: 'prepared', preparedAt: now(), machine, suite: cfg.suite, projects, compilers, settings, hyperfine: { version: (await exec([hf, '--version'])).stdout.trim(), sha256: await fileSha(hf) }, clearedEnvironmentVariables: removed, cells, preflight: {}, samples: [] };
  try { run.repositoryRevision = (await exec(['git', 'rev-parse', 'HEAD'], { cwd: REPO })).stdout.trim(); } catch { run.repositoryRevision = null; }
  try { run.rustc = (await exec(['rustc', '-V'], { cwd: REPO })).stdout.trim(); } catch { run.rustc = null; }
  await writeJson(path.join(source, 'run.json'), run);
  if (dryRun) {
    for (const cell of cells) process.stdout.write(`${cell.cwd}: ${Object.values(cell.commands).map(shellJoin).join(' | ')}\n`);
    return source;
  }
  run.status = 'running'; run.startedAt = now(); await writeJson(path.join(source, 'run.json'), run);
  await measureOne(hf, ['/usr/bin/true'], { cwd: source, env, output: path.join(source, 'counter-probe.json') });
  for (const cell of cells) {
    const directory = path.join(source, cell.project); await mkdir(directory, { recursive: true });
    const checks = {};
    for (const [id, command] of Object.entries(cell.commands)) checks[id] = await preflight(command, cell.cwd, env, path.join(directory, `${id}-preflight.log`));
    run.preflight[cell.project] = checks;
    const args = [hf, '--shell=none', '--runs', String(runs), '--warmup', String(warmup), '--metrics', METRICS, '--style', 'basic', '--output', path.join(directory, 'compiler.log'), '--ignore-failure=1,2', '--export-json', path.join(directory, 'hyperfine.json')];
    for (const [id, command] of Object.entries(cell.commands)) args.push('--command-name', id, shellJoin(command));
    log(`${cell.project}: measuring ${Object.keys(cell.commands).join(', ')}`);
    await exec(args, { cwd: cell.cwd, env, timeout: 900 * Object.keys(cell.commands).length * (runs + warmup), inherit: true });
    requireValue(!/^(?:panic: |fatal error: |thread '.*' panicked at )/m.test(await readFile(path.join(directory, 'compiler.log'), 'utf8')), `${cell.project}: compiler crash; see ${directory}/compiler.log`);
    const result = hyperfineResults(await readJson(path.join(directory, 'hyperfine.json')), Object.keys(compilers), runs, cell.project);
    for (const row of result) {
      requireValue(row.command === shellJoin(cell.commands[row.name]), `${cell.project}/${row.name}: command differs from metadata`);
      for (const [index, sample] of row.measurements.entries()) {
        requireValue(sample.exit_code === checks[row.name].exitCode, `${cell.project}/${row.name}: exit status changed`);
        run.samples.push({ project: cell.project, compiler: row.name, repetition: index + 1, ...metrics(sample, cell.project), exitCode: sample.exit_code });
      }
    }
    await writeJson(path.join(source, 'run.json'), run);
  }
  run.completedAt = now(); run.status = 'complete'; await writeJson(path.join(source, 'run.json'), run);
  log(`Saved ${run.samples.length} samples: ${source}`);
  return source;
}
