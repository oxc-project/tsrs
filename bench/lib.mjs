import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cp, mkdir, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { existsSync, readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const BENCH = path.dirname(fileURLToPath(import.meta.url));
export const REPO = path.dirname(BENCH);
export const TIMED_FLAGS = ['--noEmit', '--incremental', 'false', '--pretty', 'false'];
export const MARKER = '.tsrs-bench.json';
export const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
export const sha = data => createHash('sha256').update(data).digest('hex');
export const fileSha = async file => sha(await readFile(file));
export const readJson = async file => JSON.parse(await readFile(file, 'utf8'));
export const writeJson = async (file, value) => { await mkdir(path.dirname(file), { recursive: true }); await writeFile(file, JSON.stringify(value, null, 2) + '\n'); };
export const requireValue = (ok, message) => { if (!ok) throw new Error(message); };
export const quote = value => `'${String(value).replaceAll("'", "'\\''")}'`;
export const shellJoin = args => args.map(quote).join(' ');
export const now = () => new Date().toISOString();
export const stamp = () => now().replaceAll(/[-:.]/g, '');
export const median = xs => { const ys = xs.filter(x => x != null).toSorted((a, b) => a - b); return ys.length ? (ys[Math.floor((ys.length - 1) / 2)] + ys[Math.floor(ys.length / 2)]) / 2 : null; };
export const mean = xs => xs.reduce((a, b) => a + b, 0) / xs.length;
export const log = message => process.stderr.write(`[bench ${new Date().toISOString().slice(11, 19)}] ${message}\n`);

export function parseArgs(argv, multi = []) {
  const options = {}, positional = [];
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--') { options._command = argv.slice(i + 1); break; }
    if (!arg.startsWith('--')) { positional.push(arg); continue; }
    const [name, inline] = arg.slice(2).split('=', 2);
    if (inline !== undefined) { options[name] = inline; continue; }
    if (multi.includes(name)) {
      const values = [];
      while (argv[i + 1] && !argv[i + 1].startsWith('--')) values.push(argv[++i]);
      options[name] = values;
    } else if (argv[i + 1] && !argv[i + 1].startsWith('--')) options[name] = argv[++i];
    else options[name] = true;
  }
  return { options, positional };
}

export function values(value, fallback = []) {
  return (value === undefined ? fallback : Array.isArray(value) ? value : [value]).flatMap(v => String(v).split(',')).map(v => v.trim()).filter(Boolean);
}

export async function exec(args, { cwd, env, timeout = 0, logFile, input, accept = [0], inherit = false } = {}) {
  requireValue(args.length > 0, 'Empty command');
  if (inherit) log(`${shellJoin(args)}${cwd ? ` (in ${cwd})` : ''}`);
  return await new Promise((resolve, reject) => {
    const child = spawn(args[0], args.slice(1), { cwd, env, stdio: ['pipe', 'pipe', 'pipe'], detached: false });
    let stdout = '', stderr = '', timer;
    if (timeout) timer = setTimeout(() => { child.kill('SIGKILL'); }, timeout * 1000);
    child.stdout.on('data', b => { stdout += b; if (inherit) process.stdout.write(b); });
    child.stderr.on('data', b => { stderr += b; if (inherit) process.stderr.write(b); });
    child.on('error', reject);
    child.on('close', async (code, signal) => {
      clearTimeout(timer);
      try {
        if (logFile) { await mkdir(path.dirname(logFile), { recursive: true }); await writeFile(logFile, stdout + stderr); }
        const result = { code: code ?? (signal ? 128 + os.constants.signals[signal] : 1), stdout, stderr, signal };
        if (!accept.includes(result.code)) reject(new Error(`${shellJoin(args)} exited ${result.code}${logFile ? `; see ${logFile}` : `: ${stderr.slice(-500)}`}`));
        else resolve(result);
      } catch (error) { reject(error); }
    });
    child.stdin.end(input);
  });
}

export function which(name) {
  const result = spawnSync('which', [name], { encoding: 'utf8' });
  return result.status === 0 ? result.stdout.trim() : null;
}

export function stable(value) {
  if (Array.isArray(value)) return value.map(stable);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => [k, stable(v)]));
  return value;
}

export const digest = value => sha(JSON.stringify(stable(value))).slice(0, 16);
export const catalog = async () => readJson(path.join(BENCH, 'projects.json'));

export function selectProjects(cfg, selection) {
  const names = values(selection, ['vscode', 'mui-docs', 'webpack', 't3code-server']);
  const all = cfg.projects.map(p => p.name);
  const selected = names.length === 1 && names[0] === 'all' ? all : names;
  requireValue(selected.length > 0 && new Set(selected).size === selected.length && selected.every(n => all.includes(n)), 'Select unique names from bench/projects.json, or all');
  return selected.map(n => structuredClone(cfg.projects.find(p => p.name === n)));
}

export async function overlayFiles(project) {
  if (!project.overlay) return [];
  const root = path.join(REPO, project.overlay), files = [];
  async function visit(dir) {
    for (const item of await readdir(dir, { withFileTypes: true })) {
      const file = path.join(dir, item.name);
      if (item.isDirectory()) await visit(file);
      else if (item.isFile()) files.push([path.relative(root, file).split(path.sep).join('/'), await readFile(file)]);
    }
  }
  await visit(root);
  files.sort(([a], [b]) => a.localeCompare(b));
  requireValue(files.length, `${project.name}: empty overlay ${project.overlay}`);
  return files;
}

export async function overlaySha(project) {
  const files = await overlayFiles(project);
  if (!files.length) return null;
  const hash = createHash('sha256');
  for (const [name, bytes] of files) hash.update(name).update(Buffer.from([0])).update(Buffer.from(sha(bytes), 'hex'));
  return hash.digest('hex').slice(0, 16);
}

export function nativePlatform() {
  const arch = { x64: 'x64', arm64: 'arm64' }[process.arch];
  requireValue(arch && ['darwin', 'linux'].includes(process.platform), `Unsupported platform ${process.platform}/${process.arch}`);
  return `${process.platform}-${arch}`;
}

export async function cacheKeys(cfg, projects) {
  const keys = { shared: digest([cfg.suite, nativePlatform()]) };
  for (const p of projects) if (p.repo) keys[p.name] = digest([p, await overlaySha(p)]);
  return keys;
}

async function checkout(name, repo, commit, install, destination, project) {
  const overlay = await overlayFiles(project ?? {});
  const marker = { repo, commit, install: install ?? null };
  if (overlay.length) marker.overlay = await overlaySha(project);
  try { if (JSON.stringify(await readJson(path.join(destination, MARKER))) === JSON.stringify(marker)) { log(`${name}: cached ${commit.slice(0, 12)}`); return; } } catch {}
  await rm(destination, { recursive: true, force: true });
  await mkdir(destination, { recursive: true });
  for (let attempt = 1; attempt <= 3; attempt++) {
    try {
      await exec(['git', 'init', '-q', destination]);
      await exec(['git', '-C', destination, 'fetch', '-q', '--depth', '1', repo, commit], { inherit: true });
      await exec(['git', '-C', destination, '-c', 'advice.detachedHead=false', 'checkout', '-q', 'FETCH_HEAD']);
      for (const [relative, bytes] of overlay) { const target = path.join(destination, relative); await mkdir(path.dirname(target), { recursive: true }); await writeFile(target, bytes); }
      if (install) await exec(['/bin/bash', '-lc', install], { cwd: destination, env: { ...process.env, CI: 'true', HUSKY: '0', COREPACK_ENABLE_PROJECT_SPEC: '0' }, timeout: 3600, inherit: true });
      await writeJson(path.join(destination, MARKER), marker);
      return;
    } catch (error) {
      if (attempt === 3) throw error;
      log(`${name}: setup attempt ${attempt} failed; retrying`);
      await rm(destination, { recursive: true, force: true }); await mkdir(destination, { recursive: true });
      await sleep(attempt * 10000);
    }
  }
}

export async function prepare(cfg, projects, work) {
  for (const p of projects) {
    if (p.suite_dir) { await checkout('typescript-benchmarking', cfg.suite.repo, cfg.suite.commit, null, path.join(work, 'suite')); p.commit = cfg.suite.commit; }
    else { await checkout(p.name, p.repo, p.commit, p.install, path.join(work, 'solutions', p.name), p); if (p.overlay) p.overlay_sha256 = await overlaySha(p); }
  }
}

export function projectPath(project, work) {
  const cwd = project.suite_dir ? path.join(work, 'suite', project.suite_dir) : path.join(work, 'solutions', project.name);
  return { cwd, config: path.resolve(cwd, project.project) };
}

export function machine() {
  const cpus = os.availableParallelism(), mem = Math.round(os.totalmem() / 2 ** 30);
  const cpu = os.cpus()[0]?.model ?? 'unknown';
  const platform = `${os.type()} ${os.arch()}`;
  let libc;
  if (process.platform === 'linux') libc = process.report?.getReport()?.header?.glibcVersionRuntime ? `glibc ${process.report.getReport().header.glibcVersionRuntime}` : 'musl';
  const label = process.env.BENCH_RUNNER ? `${process.env.BENCH_RUNNER} (${cpus} vCPU, ${mem} GB RAM, ${platform}, ${cpu})` : `local machine (${cpu}, ${cpus} cores, ${mem} GB)`;
  return { platform, cpus, cpu, memory_gb: mem, ...(libc ? { libc } : {}), label };
}

export function cleanEnvironment(workers, memoryGiB) {
  const env = { ...process.env }, removed = [];
  for (const key of Object.keys(env)) if (/^(TSRS_|TS_TEST_|RAYON_|MIMALLOC_|MALLOC_|Malloc)/.test(key) || ['GOGC', 'GOMEMLIMIT', 'GOMAXPROCS', 'GODEBUG'].includes(key)) { delete env[key]; removed.push(key); }
  env.GOMAXPROCS = String(workers); env.RAYON_NUM_THREADS = String(workers);
  if (memoryGiB) env.GOMEMLIMIT = `${memoryGiB}GiB`;
  return { env, removed: removed.sort() };
}

export async function copyFile(source, target) { await mkdir(path.dirname(target), { recursive: true }); await cp(source, target); }
export async function isFile(file) { try { return (await stat(file)).isFile(); } catch { return false; } }
export const readText = file => readFile(file, 'utf8');
export const exists = existsSync;
export const readTextSync = file => readFileSync(file, 'utf8');
