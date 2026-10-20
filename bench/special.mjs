import { spawn } from 'node:child_process';
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { BENCH, REPO, TIMED_FLAGS, catalog, exec, fileSha, log, machine, mean, median, now, prepare, projectPath, readJson, requireValue, selectProjects, stamp, values, writeJson } from './lib.mjs';
import { hyperfine, installCompilers, measureOne } from './runner.mjs';
import { loadBundle } from './results.mjs';

const number = (value, fallback, min = 1) => { const n = Number(value ?? fallback); requireValue(Number.isSafeInteger(n) && n >= min, `Expected integer >= ${min}: ${value}`); return n; };
const outputDir = (options, suffix = '') => path.resolve(options['out-dir'] ?? path.join(BENCH, 'results', suffix));
const workDir = options => path.resolve(options['work-dir'] ?? path.join(BENCH, '.work'));
const failure = output => /^(?:panic: |fatal error: |thread '.*' panicked at )/m.test(output);
const errors = output => [...output.matchAll(/(?:^|: )error TS\d+:/gm)].length;

export async function compare(options) {
  const selected = values(options.compilers, ['tsgo', 'tsgo-dev', 'tsrs', 'bun']);
  requireValue(selected.length && selected.every(c => ['tsgo', 'tsgo-dev', 'tsrs', 'bun'].includes(c)), 'Invalid compiler selection');
  requireValue(!selected.includes('tsrs') || options.tsrs, 'compare requires --tsrs');
  requireValue(!selected.includes('bun') || options.bun, 'compare requires --bun');
  const cfg = await catalog(), projects = selectProjects(cfg, options.projects ?? 'vscode');
  const work = workDir(options), out = path.join(outputDir(options, 'compare'), stamp());
  await prepare(cfg, projects, work); await mkdir(out, { recursive: true });
  const versions = [...new Set([...(selected.includes('tsgo') ? [cfg.tsgo.version] : []), ...(selected.includes('tsgo-dev') ? [cfg.reference.version] : [])])];
  const installed = versions.length ? await installCompilers(work, out, versions, 'latest', selected.includes('tsrs') ? options.tsrs : false) : { executables: {}, compilers: {} };
  const executable = {}, compilerInfo = {};
  for (const [id, source] of [['tsgo', 'typescript_1'], ['tsgo-dev', versions.length > 1 ? 'typescript_2' : 'typescript_1']]) {
    if (!selected.includes(id)) continue;
    executable[id] = installed.executables[source]; compilerInfo[id] = installed.compilers[source];
  }
  if (selected.includes('tsrs')) { executable.tsrs = path.resolve(options.tsrs); compilerInfo.tsrs = { binary: executable.tsrs, sha256: await fileSha(executable.tsrs), version: (await exec([executable.tsrs, '--version'])).stdout.trim() }; }
  if (selected.includes('bun')) { executable.bun = path.resolve(options.bun); compilerInfo.bun = { binary: executable.bun, sha256: await fileSha(executable.bun), version: (await exec([executable.bun, '--revision'])).stdout.trim() }; }
  const hf = await hyperfine(work, options.hyperfine), threads = values(options.threads, ['default', '4', '8', '16', 'all']).map(t => t === 'all' ? String(os.availableParallelism()) : t);
  requireValue(threads.every(t => t === 'default' || /^\d+$/.test(t)), 'Invalid --threads');
  const reps = number(options.reps, 20);
  const run = { schemaVersion: 2, kind: 'compare', status: 'running', startedAt: now(), machine: machine(), projects, compilers: compilerInfo, settings: { threads, reps }, samples: [], preflight: {} };
  await writeJson(path.join(out, 'run.json'), run);
  for (const project of projects) {
    const { cwd, config } = projectPath(project, work);
    run.preflight[project.name] = {};
    for (const id of selected) {
      const argv = id === 'bun' ? [executable[id], 'check', '-p', config, '--no-pretty', '--all'] : [executable[id], '-p', config, ...TIMED_FLAGS, '--extendedDiagnostics'];
      const check = await exec(argv, { cwd, timeout: 900, accept: [0, 1, 2], logFile: path.join(out, project.name, `${id}-preflight.log`) });
      const text = check.stdout + check.stderr;
      requireValue(!failure(text), `${project.name}/${id} crashed in preflight`);
      run.preflight[project.name][id] = { exitCode: check.code, files: Number(text.match(/^Files:\s+(\d+)/m)?.[1] ?? text.match(/(?:checked|No type errors in) ([\d,]+) files?/i)?.[1]?.replaceAll(',', '') ?? 0), diagnostics: errors(text) };
    }
    for (let rep = 0; rep < reps; rep++) for (const mode of threads) {
      const order = selected.slice(rep % selected.length).concat(selected.slice(0, rep % selected.length));
      for (const id of order) {
        const flags = mode === 'default' ? [] : id === 'bun' ? ['--threads', mode] : ['--checkers', mode];
        const argv = id === 'bun' ? [executable[id], 'check', '-p', config, '--no-pretty', '--all', ...flags] : [executable[id], '-p', config, ...TIMED_FLAGS, ...flags];
        const prefix = path.join(out, project.name, `${mode}-${id}-${rep + 1}`);
        const sample = await measureOne(hf, argv, { cwd, output: `${prefix}.json`, logFile: `${prefix}.log`, name: id });
        requireValue(sample.exitCode === run.preflight[project.name][id].exitCode, `${project.name}/${id}: exit status changed`);
        run.samples.push({ project: project.name, compiler: id, mode, repetition: rep + 1, ...sample });
      }
    }
    await writeJson(path.join(out, 'run.json'), run);
  }
  run.status = 'complete'; run.completedAt = now(); await writeJson(path.join(out, 'run.json'), run);
  const lines = ['# Head-to-head comparison', '', `Measured ${run.startedAt} on ${run.machine.label}.`, '', '| Project | Threads | Compiler | Wall time | Peak RSS | Instructions |', '| --- | --- | --- | ---: | ---: | ---: |'];
  for (const project of projects) for (const mode of threads) for (const id of selected) {
    const group = run.samples.filter(s => s.project === project.name && s.mode === mode && s.compiler === id);
    lines.push(`| ${project.name} | ${mode} | ${id} | ${mean(group.map(s => s.wallSeconds)).toFixed(3)} s | ${(mean(group.map(s => s.peakRssBytes)) / 2 ** 30).toFixed(3)} GiB | ${Math.round(mean(group.map(s => s.instructions))).toLocaleString('en-US')} |`);
  }
  await writeFile(path.join(out, 'report.md'), lines.join('\n') + '\n');
  process.stdout.write(`${out}\n`);
}

async function rss(pid) {
  try { return Number((await readFile(`/proc/${pid}/status`, 'utf8')).match(/^VmRSS:\s+(\d+)/m)?.[1]) * 1024 || null; }
  catch { try { const result = await exec(['ps', '-o', 'rss=', '-p', String(pid)], { accept: [0, 1] }); return Number(result.stdout.trim()) * 1024 || null; } catch { return null; } }
}

export async function watchSession(exe, cwd, config, leaf, { edits, quiet, timeout }, logFile) {
  const original = await readFile(leaf);
  const started = performance.now(), command = [exe, '--watch', '-p', config, '--noEmit', '--preserveWatchOutput', '--pretty', 'false'];
  const child = spawn(command[0], command.slice(1), { cwd, detached: true, stdio: ['ignore', 'pipe', 'pipe'] });
  let buffer = '', observed = [], peak = 0, closed = false, exitCode = null;
  child.on('error', error => { closed = true; exitCode = error.code; });
  const poll = setInterval(async () => { try { peak = Math.max(peak, await rss(child.pid) ?? 0); } catch {} }, 50);
  function feed(bytes) {
    buffer += bytes.toString();
    for (;;) {
      const index = buffer.indexOf('\n'); if (index < 0) break;
      const line = buffer.slice(0, index); buffer = buffer.slice(index + 1);
      observed.push({ at: performance.now(), line });
    }
  }
  child.stdout.on('data', feed); child.stderr.on('data', feed);
  child.on('close', code => { closed = true; exitCode = code; });
  async function settle(since) {
    const deadline = performance.now() + timeout * 1000;
    let latest = null, lastOutput = null, reports = 0, found = null, seen = 0;
    while (performance.now() < deadline) {
      while (seen < observed.length) {
        const event = observed[seen++];
        if (event.at < since) continue;
        lastOutput = event.at;
        const match = event.line.match(/Found (\d+) errors?/);
        if (/Watching for file changes/.test(event.line)) { latest = event.at; reports++; found = Number(match?.[1] ?? 0); }
      }
      if (latest != null && performance.now() - lastOutput >= quiet * 1000) return { seconds: (latest - since) / 1000, reports, errors: found };
      if (closed) throw new Error(`watch exited ${exitCode}`);
      await new Promise(resolve => setTimeout(resolve, 20));
    }
    throw new Error(`No watch report within ${timeout} seconds`);
  }
  const result = { ok: false };
  try {
    const initial = await settle(started); result.initialSeconds = initial.seconds; result.initialErrors = initial.errors;
    const text = original.toString('utf8');
    const measurements = [];
    for (let i = 0; i <= edits; i++) { const time = performance.now(); await writeFile(leaf, `${text}\n// tsrs bench watch edit ${i}\n`); measurements.push(await settle(time)); }
    result.firstEditSeconds = measurements[0].seconds; result.repeatEditSeconds = median(measurements.slice(1).map(m => m.seconds));
    let time = performance.now(); await writeFile(leaf, `${text}\nexport const tsrsBenchWatchError: number = 'not a number';\n`);
    const typeError = await settle(time); result.typeErrorSeconds = typeError.seconds;
    time = performance.now(); await writeFile(leaf, original); const restored = await settle(time);
    result.finalRssBytes = await rss(child.pid);
    result.ok = measurements.every(m => m.errors === initial.errors) && typeError.errors === initial.errors + 1 && restored.errors === initial.errors;
  } catch (error) { result.failure = String(error); }
  finally {
    await writeFile(leaf, original); clearInterval(poll);
    try { process.kill(-child.pid, 'SIGINT'); } catch {}
    await new Promise(resolve => { if (closed) resolve(); else { const timer = setTimeout(() => { try { process.kill(-child.pid, 'SIGKILL'); } catch {} resolve(); }, 15000); child.once('close', () => { clearTimeout(timer); resolve(); }); } });
    result.peakRssBytes = peak;
    await writeFile(logFile, observed.map(e => e.line).join('\n') + '\n');
  }
  return result;
}

export async function watch(options) {
  const cfg = await catalog(), projectName = options.project ?? 'vscode';
  requireValue(projectName === 'vscode', 'Watch benchmark currently supports vscode');
  const project = selectProjects(cfg, projectName)[0], work = workDir(options), out = path.join(outputDir(options, 'watch'), stamp());
  await prepare(cfg, [project], work); await mkdir(out, { recursive: true });
  const { cwd, config } = projectPath(project, work), leaf = path.join(cwd, 'src/vs/base/test/common/strings.test.ts');
  const tsrs = path.resolve(options.tsrs ?? path.join(REPO, 'target/release/tsrs'));
  let tsgo = options.tsgo && path.resolve(options.tsgo);
  if (!tsgo) {
    const installed = await installCompilers(work, out, [cfg.tsgo.version], 'latest', tsrs);
    tsgo = installed.executables.typescript_1;
  }
  const compilers = { tsgo, tsrs }, reps = number(options.reps, 3), tsgoReps = number(options['tsgo-reps'], reps);
  const settings = { reps, tsgoReps, edits: number(options.edits, 5, 0), quiet: Number(options.quiet ?? 2), timeout: Number(options.timeout ?? 600) };
  const run = { schemaVersion: 2, kind: 'watch', status: 'running', startedAt: now(), machine: machine(), projects: [project], compilers: Object.fromEntries(await Promise.all(Object.entries(compilers).map(async ([id, exe]) => [id, { binary: exe, sha256: await fileSha(exe) }]))), settings, samples: [] };
  await writeJson(path.join(out, 'run.json'), run);
  for (let rep = 0; rep < Math.max(reps, tsgoReps); rep++) {
    for (const id of rep % 2 ? ['tsrs', 'tsgo'] : ['tsgo', 'tsrs']) {
      if (rep >= (id === 'tsgo' ? tsgoReps : reps)) continue;
      const result = await watchSession(compilers[id], cwd, config, leaf, settings, path.join(out, `${id}-${rep + 1}.log`));
      run.samples.push({ project: projectName, compiler: id, repetition: rep + 1, ...result });
      await writeJson(path.join(out, 'run.json'), run);
    }
  }
  run.status = 'complete'; run.completedAt = now(); await writeJson(path.join(out, 'run.json'), run);
  const lines = ['# Watch benchmark', '', `Measured ${run.startedAt} on ${run.machine.label}.`, '', '| Compiler | Successful sessions | Startup | First edit | Repeat edit | Type error | Peak RSS |', '| --- | ---: | ---: | ---: | ---: | ---: | ---: |'];
  for (const id of ['tsgo', 'tsrs']) { const samples = run.samples.filter(s => s.compiler === id && s.ok); lines.push(`| ${id} | ${samples.length} | ${median(samples.map(s => s.initialSeconds))?.toFixed(3) ?? 'n/a'} s | ${median(samples.map(s => s.firstEditSeconds))?.toFixed(3) ?? 'n/a'} s | ${median(samples.map(s => s.repeatEditSeconds))?.toFixed(3) ?? 'n/a'} s | ${median(samples.map(s => s.typeErrorSeconds))?.toFixed(3) ?? 'n/a'} s | ${((median(samples.map(s => s.peakRssBytes)) ?? 0) / 2 ** 30).toFixed(3)} GiB |`); }
  await writeFile(path.join(out, 'report.md'), lines.join('\n') + '\n');
  requireValue(run.samples.every(s => s.ok), `Some watch sessions failed; see ${out}`);
  process.stdout.write(`${out}\n`);
}

async function historicalRuns(directory) {
  const found = [];
  for (const item of await readdir(directory, { withFileTypes: true })) {
    const candidate = path.join(directory, item.name);
    try {
      if (item.isDirectory()) {
        const run = await loadBundle(candidate);
        if (run.kind === 'compile') found.push({ file: candidate, run });
      } else if (item.name.endsWith('.json')) {
        const run = await readJson(candidate);
        if (run.tsrs && run.projects) found.push({ file: candidate, run });
      }
    } catch {}
  }
  return found.toSorted((a, b) => (a.run.startedAt ?? a.run.date ?? '').localeCompare(b.run.startedAt ?? b.run.date ?? ''));
}

function historicalValue(run, project, metric) {
  if (run.schemaVersion === 2) {
    const samples = run.samples?.filter(s => s.project === project && s.compiler === 'tsrs') ?? [];
    const field = { instructions: 'instructions', peak: 'peakRssBytes', wall: 'wallSeconds', wide_wall: 'wallSeconds', wide_peak: 'peakRssBytes' }[metric];
    return samples.length ? mean(samples.map(s => s[field])) : null;
  }
  const [mode, field] = { instructions: ['single', 'instructions'], peak: ['single', 'max_rss_bytes'], wall: ['default', 'wall_s'], wide_wall: ['wide', 'wall_s'], wide_peak: ['wide', 'peak_rss_bytes'] }[metric];
  return run.projects?.[project]?.[mode]?.tsrs?.[field] ?? null;
}

function commit(run) { return run.tsrs?.commit ?? run.repositoryRevision ?? run.renderer?.revision ?? ''; }

function setup(run, project) {
  const measured = run.schemaVersion === 2 ? run.machine : run.projects?.[project]?.single?.machine ?? run.machine;
  return JSON.stringify({ cpu: measured?.cpu, libc: measured?.libc, label: measured?.label, cpus: measured?.cpus,
    build: run.schemaVersion === 2 ? run.settings : run.tsrs?.build, rustc: run.rustc ?? run.tsrs?.rustc,
    projectCommit: run.schemaVersion === 2 ? run.projects.find(p => p.name === project)?.commit : undefined });
}

async function gitOutput(args) {
  try { return (await exec(['git', ...args], { cwd: REPO, accept: [0, 1, 128] })).stdout.trim(); } catch { return ''; }
}

async function firstParentOrder(runs, branch, includeAll) {
  const line = await gitOutput(['rev-list', '--first-parent', branch]);
  if (!line) return runs;
  const positions = new Map(line.split('\n').toReversed().map((sha, i) => [sha, i]));
  const placed = [], unplaced = [];
  for (const item of runs) {
    const sha = commit(item.run), full = sha && await gitOutput(['rev-parse', '--verify', `${sha}^{commit}`]);
    if (positions.has(full)) placed.push({ item, position: positions.get(full) });
    else unplaced.push(item);
  }
  placed.sort((a, b) => a.position - b.position || (a.item.run.startedAt ?? a.item.run.date).localeCompare(b.item.run.startedAt ?? b.item.run.date));
  return [...placed.map(p => p.item), ...(includeAll ? unplaced : [])];
}

export async function history(options) {
  const directory = path.resolve(options['results-dir'] ?? path.join(BENCH, 'results'));
  const runs = await firstParentOrder(await historicalRuns(directory), options.branch ?? 'origin/main', Boolean(options.all));
  requireValue(runs.length, `No results on ${options.branch ?? 'origin/main'}; use --all to include other commits`);
  const metrics = values(options.metrics, ['instructions', 'peak', 'wall']);
  requireValue(metrics.every(m => ['instructions', 'peak', 'wall', 'wide_wall', 'wide_peak'].includes(m)), 'Unknown history metric');
  const projects = values(options.projects, [...new Set(runs.flatMap(({ run }) => run.schemaVersion === 2 ? run.projects.map(p => p.name) : Object.keys(run.projects)))]);
  const since = options.since ? runs.findIndex(r => commit(r.run).startsWith(options.since)) : 0;
  requireValue(since >= 0, `No result starts at commit ${options.since}`);
  const filtered = runs.slice(since);
  for (const metric of metrics) {
    const header = ['commit', 'date', ...projects], rows = [header];
    const previous = new Map();
    for (const { run } of filtered) {
      const cells = [commit(run).slice(0, 12), (run.startedAt ?? run.date ?? '').slice(0, 10)];
      for (const project of projects) {
        const value = historicalValue(run, project, metric), key = setup(run, project), old = previous.get(project);
        let cell = value == null ? '' : value.toFixed(metric.includes('wall') ? 3 : 0);
        if (value != null && old && old.key === key && old.value) {
          const change = (value / old.value - 1) * 100;
          const flag = metric === 'instructions' && change > 1 || metric === 'peak' && change > 1 && value - old.value > 2 * 2 ** 20;
          cell += ` (${change >= 0 ? '+' : ''}${change.toFixed(1)}%${flag ? '!' : ''})`;
        } else if (value != null && old) cell += ' (new baseline)';
        cells.push(cell);
        if (value != null) previous.set(project, { key, value });
      }
      rows.push(cells);
    }
    if (options.tsv) process.stdout.write(rows.map(cells => cells.join('\t')).join('\n') + '\n\n');
    else process.stdout.write(`### ${metric} (change against the previous compatible result)\n\n${rows.map((cells, i) => `| ${cells.join(' | ')} |${i === 0 ? `\n| ${cells.map((_, j) => j < 2 ? '---' : '---:').join(' | ')} |` : ''}`).join('\n')}\n\n`);
  }
}

export async function regressions(options, positional) {
  const directory = path.resolve(options['results-dir'] ?? path.join(BENCH, 'results'));
  const runs = await historicalRuns(directory);
  requireValue(runs.length >= 2, 'Need at least two comparable benchmark results');
  const current = positional[0] ? runs.find(r => path.resolve(r.file) === path.resolve(positional[0])) : runs.at(-1);
  requireValue(current, 'Result not found');
  const candidates = runs.slice(0, runs.indexOf(current)).toReversed().filter(r => {
    if (r.run.machine?.cpu !== current.run.machine?.cpu || r.run.machine?.libc !== current.run.machine?.libc) return false;
    if (r.run.machine?.label !== current.run.machine?.label) return false;
    if (r.run.schemaVersion !== current.run.schemaVersion) return false;
    if (r.run.schemaVersion !== 2 && r.run.tsrs?.build !== current.run.tsrs?.build) return false;
    if (r.run.schemaVersion === 2 && current.run.schemaVersion === 2) {
      const settings = ['checkers', 'workers', 'memoryGiB'];
      if (settings.some(key => r.run.settings?.[key] !== current.run.settings?.[key])) return false;
      if (r.run.projects.some(p => current.run.projects.find(c => c.name === p.name)?.commit !== p.commit)) return false;
    }
    return true;
  });
  const currentCommit = commit(current.run);
  const ancestors = [];
  for (const item of candidates) {
    const older = commit(item.run);
    if (!older || !currentCommit || older === currentCommit) continue;
    if ((await exec(['git', 'merge-base', '--is-ancestor', older, currentCommit], { cwd: REPO, accept: [0, 1, 128] })).code !== 0) continue;
    const distance = Number(await gitOutput(['rev-list', '--count', `${older}..${currentCommit}`]));
    if (distance > 0) ancestors.push({ item, distance });
  }
  const previous = ancestors.length ? ancestors.toSorted((a, b) => a.distance - b.distance)[0].item : candidates[0];
  requireValue(previous, 'No earlier result on the same CPU and C library');
  const projects = current.run.schemaVersion === 2 ? current.run.projects.map(p => p.name) : Object.keys(current.run.projects);
  const threshold = Number(options.threshold ?? 1), lines = ['| Project | Metric | Before | After | Change | Verdict |', '| --- | --- | ---: | ---: | ---: | --- |'];
  requireValue(Number.isFinite(threshold) && threshold >= 0, '--threshold must be a nonnegative percentage');
  const beforeRustc = previous.run.rustc ?? previous.run.tsrs?.rustc;
  const afterRustc = current.run.rustc ?? current.run.tsrs?.rustc;
  const toolchainChanged = beforeRustc !== afterRustc;
  if (toolchainChanged) process.stdout.write(`Rust compiler changed (${beforeRustc ?? 'unrecorded'} → ${afterRustc ?? 'unrecorded'}); reporting changes without flags.\n\n`);
  let flagged = false;
  for (const project of projects) for (const metric of ['instructions', 'peak']) {
    const before = historicalValue(previous.run, project, metric), after = historicalValue(current.run, project, metric);
    if (!before || !after) continue;
    const delta = (after / before - 1) * 100, floor = metric === 'peak' ? 2 * 2 ** 20 : 0;
    const limit = metric === 'instructions' ? threshold : 1;
    const verdict = toolchainChanged ? 'toolchain change' : delta > limit && after - before > floor ? 'regression' : delta < -limit && before - after > floor ? 'improvement' : 'unchanged';
    flagged ||= verdict === 'regression';
    lines.push(`| ${project} | ${metric} | ${before.toFixed(0)} | ${after.toFixed(0)} | ${delta.toFixed(2)}% | ${verdict} |`);
  }
  const report = lines.join('\n') + '\n'; process.stdout.write(report);
  if (options.comment && flagged) {
    requireValue(process.env.GITHUB_TOKEN && options.repo && options.issue, '--comment requires GITHUB_TOKEN, --repo and --issue');
    const response = await fetch(`https://api.github.com/repos/${options.repo}/issues/${options.issue}/comments`, { method: 'POST', headers: { Authorization: `Bearer ${process.env.GITHUB_TOKEN}`, Accept: 'application/vnd.github+json' }, body: JSON.stringify({ body: `Benchmark regression:\n\n${report}` }) });
    requireValue(response.ok, `GitHub comment failed: HTTP ${response.status}`);
  }
}
