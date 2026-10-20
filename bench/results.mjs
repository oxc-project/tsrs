import { cp, mkdir, readdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { exec, fileSha, mean, readJson, requireValue, sha, writeJson } from './lib.mjs';
import { hyperfineResults } from './runner.mjs';

export async function loadBundle(source) {
  const run = await readJson(path.join(source, 'run.json'));
  requireValue(run.schemaVersion === 2 && run.status === 'complete', `${source}: incomplete or unsupported bundle`);
  requireValue(['compile', 'compare', 'watch', 'count'].includes(run.kind), `${source}: unsupported benchmark kind`);
  if (run.kind === 'compile') {
    const names = Object.keys(run.compilers), projects = run.projects.map(p => p.name);
    requireValue(projects.length && new Set(projects).size === projects.length, `${source}: duplicate projects`);
    requireValue(run.cells.length === projects.length && new Set(run.cells.map(c => c.project)).size === projects.length, `${source}: missing command cells`);
    const reconstructed = [];
    for (const cell of run.cells) {
      requireValue(projects.includes(cell.project) && Object.keys(cell.commands).length === names.length, `${source}: incomplete commands`);
      const raw = await readJson(path.join(source, cell.project, 'hyperfine.json'));
      const results = hyperfineResults(raw, names, run.settings.runs, cell.project);
      const checks = run.preflight[cell.project];
      requireValue(checks && names.every(n => checks[n]?.files > 0), `${source}: incomplete preflight`);
      for (const result of results) {
        const command = cell.commands[result.name];
        requireValue(result.command === command.map(x => `'${String(x).replaceAll("'", "'\\''")}'`).join(' '), `${source}: measured command differs`);
        for (const [i, sample] of result.measurements.entries()) {
          requireValue(sample.exit_code === checks[result.name].exitCode, `${source}: exit status changed`);
          reconstructed.push({ project: cell.project, compiler: result.name, repetition: i + 1, wallSeconds: sample.time_wall_clock.value, instructions: sample.instructions.value, peakRssBytes: sample.memory_peak_resident.value, exitCode: sample.exit_code });
        }
      }
    }
    requireValue(JSON.stringify(reconstructed) === JSON.stringify(run.samples), `${source}: samples differ from Hyperfine exports`);
  }
  return run;
}

function identity(compilers) {
  return Object.fromEntries(Object.entries(compilers).map(([id, info]) => [id, Object.fromEntries(Object.entries(info).filter(([key]) => !['binary', 'requested'].includes(key)))]));
}

export async function mergeBundles(sources, destination) {
  requireValue(sources.length > 0, 'No bundles to merge');
  const runs = await Promise.all(sources.map(loadBundle));
  const first = runs[0];
  requireValue(first.kind === 'compile' && runs.every(r => r.kind === 'compile'), 'Merge accepts compile bundles only');
  const projects = [], cells = [], samples = [], preflight = {}, seen = new Set();
  for (const run of runs) {
    for (const key of ['settings', 'suite', 'hyperfine', 'machine', 'repositoryRevision', 'rustc']) requireValue(JSON.stringify(run[key]) === JSON.stringify(first[key]), `Cannot merge different ${key}`);
    requireValue(JSON.stringify(identity(run.compilers)) === JSON.stringify(identity(first.compilers)), 'Cannot merge different compiler builds');
    for (const project of run.projects) { requireValue(!seen.has(project.name), `Duplicate project ${project.name}`); seen.add(project.name); projects.push(project); }
    cells.push(...run.cells); samples.push(...run.samples); Object.assign(preflight, run.preflight);
  }
  await mkdir(destination, { recursive: false });
  for (const [index, source] of sources.entries()) {
    const origin = path.join(destination, 'partials', String(index));
    await mkdir(origin, { recursive: true });
    for (const item of await readdir(source, { withFileTypes: true })) {
      if (item.isDirectory()) {
        if (item.name === 'partials') continue;
        await cp(path.join(source, item.name), path.join(destination, item.name), { recursive: true });
      } else {
        await cp(path.join(source, item.name), path.join(origin, item.name));
        if (index === 0 && (item.name.endsWith('package-lock.json') || item.name === 'npm-packages.json')) await cp(path.join(source, item.name), path.join(destination, item.name));
      }
    }
  }
  const merged = { ...first, projects, cells, samples, preflight, startedAt: runs.map(r => r.startedAt).sort()[0], completedAt: runs.map(r => r.completedAt).sort().at(-1), sourceBundles: sources.map((_, i) => `partials/${i}/run.json`) };
  await writeJson(path.join(destination, 'run.json'), merged);
  await loadBundle(destination);
  return destination;
}

export async function rendererIdentity(files) {
  const digests = {};
  for (const file of files) digests[path.basename(file)] = await fileSha(file);
  const revision = (await exec(['git', 'rev-parse', 'HEAD'], { accept: [0, 128] })).stdout.trim() || 'unknown';
  return { revision, files: digests, sha256: sha(JSON.stringify(digests)) };
}

export function publicationFromRun(run, sourceSha256, renderer) {
  requireValue(run.kind === 'compile', 'Only compile bundles can make the publication');
  const compilerOrder = ['tsrs', ...Object.keys(run.compilers).filter(c => c !== 'tsrs')];
  const rows = [];
  for (const project of run.projects) for (const compiler of compilerOrder) {
    const samples = run.samples.filter(s => s.project === project.name && s.compiler === compiler);
    requireValue(samples.length === run.settings.runs, `${project.name}/${compiler}: missing samples`);
    rows.push({ project: project.name, compiler, wallSeconds: mean(samples.map(s => s.wallSeconds)), instructions: mean(samples.map(s => s.instructions)), peakRssBytes: mean(samples.map(s => s.peakRssBytes)), files: run.preflight[project.name][compiler].files, diagnostics: run.preflight[project.name][compiler].errors });
  }
  return { schemaVersion: 1, kind: 'publication', measuredAt: run.startedAt, machine: run.machine, settings: run.settings, projects: run.projects.map(p => ({ name: p.name, commit: p.commit, overlaySha256: p.overlay_sha256 ?? null })), compilerOrder, compilers: run.compilers, rows, sourceBundleSha256: sourceSha256, renderer };
}

export async function readPublication(source, renderer) {
  const { existsSync } = await import('node:fs');
  if (existsSync(path.join(source, 'run.json'))) {
    const run = await loadBundle(source);
    return publicationFromRun(run, await fileSha(path.join(source, 'run.json')), renderer);
  }
  if (existsSync(path.join(source, 'metadata.json'))) {
    const meta = await readJson(path.join(source, 'metadata.json'));
    requireValue(meta.schema_version === 1 && meta.completed_utc, 'Incomplete legacy bundle');
    const raw = await readJson(path.join(source, 'raw.json'));
    const compilerOrder = ['tsrs', ...Object.keys(meta.compilers).filter(c => c !== 'tsrs')];
    const rows = [];
    for (const project of meta.projects) {
      const preflight = await readJson(path.join(source, project.name, 'preflight.json'));
      for (const compiler of compilerOrder) {
        const group = raw.filter(s => s.project === project.name && s.compiler === compiler);
        requireValue(group.length === meta.runs, `${project.name}/${compiler}: missing legacy samples`);
        const check = preflight.compilers[compiler];
        rows.push({ project: project.name, compiler, wallSeconds: mean(group.map(s => s.wall_seconds)), instructions: mean(group.map(s => s.instructions)), peakRssBytes: mean(group.map(s => s.peak_rss_bytes)), files: check.files, diagnostics: check.errors });
      }
    }
    return { schemaVersion: 1, kind: 'publication', measuredAt: meta.started_utc, machine: meta.machine, settings: { checkers: meta.settings.checkers, workers: meta.settings.workers, memoryGiB: meta.settings.memory_gib, runs: meta.runs, warmup: meta.warmup }, projects: meta.projects.map(p => ({ name: p.name, commit: p.commit, overlaySha256: p.overlay_sha256 ?? null })), compilerOrder, compilers: meta.compilers, rows, sourceBundleSha256: await fileSha(path.join(source, 'raw.json')), renderer };
  }
  const publication = await readJson(source);
  if (publication.tsrs && publication.projects && !publication.schemaVersion) {
    const names = Object.keys(publication.projects);
    const modes = ['checkers8', 'checkers4', 'checkers2', 'checkers1', 'wide', 'default'];
    const mode = modes.find(m => names.every(n => publication.projects[n][m]?.tsrs && publication.projects[n][m]?.tsgo));
    requireValue(mode, `${source}: no shared legacy chart mode`);
    const hasBun = names.every(n => publication.projects[n][mode].bun);
    const compilerOrder = hasBun ? ['tsrs', 'typescript_1', 'bun'] : ['tsrs', 'typescript_1'];
    const compilers = { tsrs: publication.tsrs, typescript_1: publication.tsgo, ...(hasBun ? { bun: publication.bun ?? { version: 'check' } } : {}) };
    const rows = names.flatMap(name => compilerOrder.map(id => {
      const cell = publication.projects[name][mode][id === 'typescript_1' ? 'tsgo' : id];
      return { project: name, compiler: id, wallSeconds: cell.wall_s, peakRssBytes: cell.peak_rss_bytes, instructions: null, files: cell.files ?? null, diagnostics: cell.errors ?? null };
    }));
    return { schemaVersion: 1, kind: 'publication', measuredAt: publication.date, machine: publication.machine, settings: { checkers: Number(mode.match(/\d+/)?.[0] ?? 4), workers: publication.machine.cpus, memoryGiB: null, runs: publication.reps ?? 1, warmup: 0 }, projects: names.map(name => ({ name })), compilerOrder, compilers, rows, sourceBundleSha256: await fileSha(source), renderer };
  }
  requireValue(publication.schemaVersion === 1 && publication.kind === 'publication', `${source}: unsupported publication`);
  requireValue(publication.projects?.length && publication.rows?.length && publication.compilerOrder?.length, `${source}: missing chart data`);
  for (const project of publication.projects) for (const compiler of publication.compilerOrder) {
    const rows = publication.rows.filter(r => r.project === project.name && r.compiler === compiler);
    requireValue(rows.length === 1 && ['wallSeconds', 'peakRssBytes'].every(k => Number.isFinite(rows[0][k]) && rows[0][k] > 0), `${source}: invalid row ${project.name}/${compiler}`);
  }
  return publication;
}
