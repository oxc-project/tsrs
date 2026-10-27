import { mkdir, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { BENCH, REPO, TIMED_FLAGS, catalog, cleanEnvironment, exec, fileSha, log, machine, median, now, prepare, projectPath, requireValue, sha, stamp, values, writeJson } from './lib.mjs';
import { hyperfine, installCompilers, measureOne } from './runner.mjs';

const GIB = 2 ** 30;
const COMPILERS = ['typescript', 'tsrs', 'tsrs_fast'];
const CRASH = /^(?:panic: |fatal error: |thread '.*' panicked at )/m;
const OPTION_ERRORS_BEFORE_CHECK = new Set(['TS6379']);
const DEFAULT_MODE = 'default';
export const RELEASE_PROJECTS = { vscode: 'vscode', 'mui-docs': 'mui-docs', 't3code-server': 't3code-server', 'supabase-studio': 'supabase', playwright: 'playwright', typeorm: 'typeorm', 'date-fns': 'date-fns', nuxt: 'nuxt', 'xstate-main': 'xstate', 'formbricks-web': 'formbricks', 'cal-diy': 'cal-diy' };
const SCATTER_PROJECTS = ['vscode', 'mui-docs', 't3code-server', 'supabase-studio', 'playwright', 'typeorm', 'date-fns'];

function integer(value, fallback, minimum = 1) {
  const n = value === undefined ? fallback : Number(value);
  requireValue(Number.isSafeInteger(n) && n >= minimum, `Expected an integer >= ${minimum}, got ${value}`);
  return n;
}

function positive(value, fallback) {
  const n = value === undefined ? fallback : Number(value);
  requireValue(Number.isFinite(n) && n > 0, `Expected a positive number, got ${value}`);
  return n;
}

function checkerMode(value) {
  return value === DEFAULT_MODE ? value : integer(value);
}

function modeLabel(checkers) {
  return checkers === DEFAULT_MODE ? 'default settings' : `--checkers ${checkers}`;
}

function command(executables, compiler, config, checkers) {
  const count = checkers === DEFAULT_MODE ? [] : ['--checkers', String(checkers)];
  return [compiler === 'typescript' ? executables.typescript : executables.tsrs, '-p', config, ...TIMED_FLAGS, ...count, ...(compiler === 'tsrs_fast' ? ['--fast'] : [])];
}

function errorKeys(output) {
  const keys = [];
  for (const line of output.split('\n')) {
    const match = line.match(/^((?:\S.*\(\d+,\d+\): )?error TS\d+):/);
    if (match) keys.push(match[1]);
  }
  return keys.sort();
}

function cpuTotals() {
  return os.cpus().reduce((sum, cpu) => {
    const total = Object.values(cpu.times).reduce((a, b) => a + b, 0);
    return { busy: sum.busy + total - cpu.times.idle, total: sum.total + total };
  }, { busy: 0, total: 0 });
}

async function busyCores(ms = 750) {
  const before = cpuTotals();
  await new Promise(resolve => setTimeout(resolve, ms));
  const after = cpuTotals();
  return (after.busy - before.busy) / (after.total - before.total) * os.cpus().length;
}

async function waitQuiet(context, cores, seconds) {
  const started = Date.now();
  let busy = await busyCores();
  while (busy > cores && Date.now() - started < seconds * 1000) {
    log(`${context}: waiting for background CPU (${busy.toFixed(1)} cores busy)`);
    await new Promise(resolve => setTimeout(resolve, 2000));
    busy = await busyCores();
  }
  return { backgroundCores: Number(busy.toFixed(2)), quiet: busy <= cores, waitedSeconds: Math.round((Date.now() - started) / 1000) };
}

function publicCompiler(info) {
  return { package: info.package, version: info.version, label: info.label, requested: info.requested, tarball: info.dist?.tarball ?? null, integrity: info.dist?.integrity ?? null, sha256: info.sha256, version_output: info.version_output };
}

export async function release(options) {
  const cfg = await catalog();
  const names = values(options.projects, Object.keys(RELEASE_PROJECTS));
  requireValue(names.length && new Set(names).size === names.length, 'Select distinct projects');
  const scatter = values(options['scatter-projects'], SCATTER_PROJECTS.filter(name => names.includes(name)));
  requireValue(scatter.every(name => names.includes(name)) && scatter.length <= 7, 'Scatter projects must be at most seven measured projects');
  const projects = names.map(name => {
    const project = cfg.projects.find(p => p.name === name);
    requireValue(project, `Unknown project ${name}`);
    return structuredClone(project);
  });
  const scatterCheckers = values(options.checkers, [DEFAULT_MODE]).map(checkerMode);
  const settings = {
    workers: options.workers === undefined ? null : integer(options.workers), runs: integer(options.runs, 10), typescriptRuns: integer(options['typescript-runs'], 5),
    scatterCheckers, detailCheckers: checkerMode(options['detail-checkers'] ?? scatterCheckers.at(-1)),
    quietCores: positive(options['quiet-cores'], 1.5), quietWaitSeconds: integer(options['quiet-wait'], 180, 0),
  };
  requireValue(settings.scatterCheckers.includes(settings.detailCheckers), '--checkers must include --detail-checkers');
  const work = path.resolve(options['work-dir'] ?? path.join(BENCH, '.work'));
  const out = path.join(path.resolve(options['out-dir'] ?? path.join(BENCH, 'results')), stamp());
  await prepare(cfg, projects, work);
  await mkdir(out, { recursive: true });
  const hf = await hyperfine(work, options.hyperfine);
  const installed = await installCompilers(work, out, [options['typescript-version'] ?? 'beta'], options['tsrs-version'] ?? 'latest');
  const executables = { typescript: installed.executables.typescript_1, tsrs: installed.executables.tsrs };
  const { env, removed } = cleanEnvironment(settings.workers ?? undefined);
  delete env.TSRS_FAST;
  const run = {
    schemaVersion: 2, kind: 'release', status: 'running', startedAt: now(), machine: machine(), suite: cfg.suite, settings,
    compilers: { typescript: publicCompiler(installed.compilers.typescript_1), tsrs: publicCompiler(installed.compilers.tsrs) },
    hyperfine: { version: (await exec([hf, '--version'])).stdout.trim(), sha256: await fileSha(hf) },
    environment: { GOMAXPROCS: env.GOMAXPROCS ?? null, RAYON_NUM_THREADS: env.RAYON_NUM_THREADS ?? null, TSRS_FAST: null }, clearedEnvironmentVariables: removed,
    projects: [], preflight: {}, samples: [],
  };
  try { run.repositoryRevision = (await exec(['git', 'rev-parse', 'HEAD'], { cwd: REPO })).stdout.trim(); } catch { run.repositoryRevision = null; }
  await writeJson(path.join(out, 'run.json'), run);
  const scratch = path.join(out, 'sample.json');
  for (const project of projects) {
    const { cwd, config } = projectPath(project, work);
    const modes = scatter.includes(project.name) ? settings.scatterCheckers : [settings.detailCheckers];
    run.projects.push({ name: project.name, project: project.project, commit: project.commit, overlay_sha256: project.overlay_sha256 ?? null, scatter: scatter.includes(project.name), checkers: modes });
    run.preflight[project.name] = {};
    for (const checkers of modes) {
      run.preflight[project.name][checkers] = {};
      for (const compiler of COMPILERS) {
        const argv = command(executables, compiler, config, checkers);
        const result = await exec(argv, { cwd, env, timeout: 900, accept: [0, 1, 2] });
        requireValue(!CRASH.test(result.stdout + result.stderr), `${project.name}/${compiler} ${modeLabel(checkers)}: compiler crash`);
        const keys = errorKeys(result.stdout + result.stderr);
        run.preflight[project.name][checkers][compiler] = { command: argv, exitCode: result.code, errors: keys.length, errorKeys: keys };
        log(`${project.name} ${modeLabel(checkers)} ${compiler}: preflight exit ${result.code}, ${keys.length} errors`);
      }
    }
    for (let rep = 0; rep < Math.max(settings.runs, settings.typescriptRuns); rep++) {
      const order = COMPILERS.slice(rep % COMPILERS.length).concat(COMPILERS.slice(0, rep % COMPILERS.length));
      for (const checkers of modes) for (const compiler of order) {
        if (rep >= (compiler === 'typescript' ? settings.typescriptRuns : settings.runs)) continue;
        const context = `${project.name} ${modeLabel(checkers)} ${compiler} run ${rep + 1}`;
        const check = run.preflight[project.name][checkers][compiler];
        const quiet = await waitQuiet(context, settings.quietCores, settings.quietWaitSeconds);
        const sample = await measureOne(hf, check.command, { cwd, env, output: scratch });
        requireValue(sample.exitCode === check.exitCode, `${context}: exit status changed`);
        run.samples.push({ project: project.name, checkers, compiler, repetition: rep + 1, ...quiet, ...sample });
        log(`${context}: ${sample.wallSeconds.toFixed(3)} s, ${(sample.peakRssBytes / GIB).toFixed(3)} GiB, background ${quiet.backgroundCores} cores`);
      }
    }
    await writeJson(path.join(out, 'run.json'), run);
  }
  await rm(scratch, { force: true });
  run.status = 'complete'; run.completedAt = now();
  await writeJson(path.join(out, 'run.json'), run);
  log(`Saved ${run.samples.length} samples: ${out}`);
  process.stdout.write(`${out}\n`);
}

export function validateRelease(run, source) {
  const { settings } = run;
  requireValue(run.projects?.length && settings?.runs > 0 && settings.typescriptRuns > 0, `${source}: incomplete release settings`);
  requireValue(run.compilers?.typescript?.version && run.compilers?.tsrs?.version, `${source}: missing compiler versions`);
  for (const project of run.projects) for (const checkers of project.checkers) for (const compiler of COMPILERS) {
    const check = run.preflight[project.name]?.[checkers]?.[compiler];
    requireValue(check && check.errors === check.errorKeys.length, `${source}: missing preflight for ${project.name}/${checkers}/${compiler}`);
    const samples = run.samples.filter(s => s.project === project.name && s.checkers === checkers && s.compiler === compiler);
    requireValue(samples.length === (compiler === 'typescript' ? settings.typescriptRuns : settings.runs), `${source}: missing samples for ${project.name}/${checkers}/${compiler}`);
    for (const sample of samples) requireValue(sample.exitCode === check.exitCode && [sample.wallSeconds, sample.peakRssBytes, sample.instructions].every(v => Number.isFinite(v) && v > 0), `${source}: invalid sample for ${project.name}/${checkers}/${compiler}`);
  }
}

export function releasePublication(run, sourceBundleSha256, renderer) {
  const rows = [];
  for (const project of run.projects) for (const checkers of project.checkers) for (const compiler of COMPILERS) {
    const samples = run.samples.filter(s => s.project === project.name && s.checkers === checkers && s.compiler === compiler);
    const check = run.preflight[project.name][checkers][compiler];
    const codes = {};
    for (const key of check.errorKeys) { const code = key.match(/TS\d+$/)[0]; codes[code] = (codes[code] ?? 0) + 1; }
    rows.push({ project: project.name, checkers, compiler, wallSeconds: median(samples.map(s => s.wallSeconds)), peakRssBytes: median(samples.map(s => s.peakRssBytes)), instructions: median(samples.map(s => s.instructions)), runs: samples.length, exitCode: check.exitCode, diagnostics: check.errors, diagnosticCodes: Object.fromEntries(Object.entries(codes).sort(([a], [b]) => a.localeCompare(b))), diagnosticsSha256: sha(JSON.stringify(check.errorKeys)).slice(0, 16) });
  }
  return {
    schemaVersion: 1, kind: 'release-publication', measuredAt: run.startedAt, completedAt: run.completedAt, machine: run.machine, settings: run.settings,
    environment: run.environment, hyperfine: run.hyperfine.version, compilers: run.compilers,
    projects: run.projects.map(p => ({ name: p.name, project: p.project, commit: p.commit, overlaySha256: p.overlay_sha256 ?? null, scatter: p.scatter, checkers: p.checkers })),
    samples: { total: run.samples.length, contended: run.samples.filter(s => !s.quiet).length },
    rows, sourceBundleSha256, renderer,
  };
}

export function validateReleasePublication(publication, source) {
  requireValue(publication.projects?.length && publication.rows?.length && publication.compilers?.typescript && publication.compilers?.tsrs, `${source}: missing chart data`);
  for (const project of publication.projects) for (const checkers of project.checkers) for (const compiler of COMPILERS) {
    const rows = publication.rows.filter(r => r.project === project.name && r.checkers === checkers && r.compiler === compiler);
    requireValue(rows.length === 1 && ['wallSeconds', 'peakRssBytes'].every(k => Number.isFinite(rows[0][k]) && rows[0][k] > 0), `${source}: invalid row ${project.name}/${checkers}/${compiler}`);
  }
  requireValue(publication.projects.some(p => p.scatter), `${source}: no scatter projects`);
}

const FONT = "Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif";
const THEMES = {
  light: { background: '#f4f3ec', border: '#dcdad0', text: '#16171d', muted: '#6f6876', grid: '#e2e0d6', axis: '#16171d', dot: '#16171d', projects: ['#6c3bff', '#e8590c', '#2f9e44', '#d4a017', '#867e8e', '#a0522d', '#7b8a00'] },
  dark: { background: '#16171d', border: '#3b3440', text: '#ffffff', muted: '#a49cab', grid: '#2a2630', axis: '#867e8e', dot: '#0c0912', projects: ['#b39aff', '#ff922b', '#51cf66', '#fcc419', '#b0a8b9', '#e0a07a', '#c0cf3a'] },
};
const BAR_COLORS = { background: '#f4f3ec', border: '#dcdad0', text: '#16171d', muted: '#6f6876', rule: '#e2e0d6', axis: '#bdbab0', tsrs_fast: '#f38020', tsrs: '#faae40', typescript: '#3178c6' };
const WIDTH = 1200, HEIGHT = 884;
const PLOT_LEFT = 130, PLOT_RIGHT = 1140, PLOT_TOP = 205, PLOT_BOTTOM = 720;
const LEGEND_Y = 139, LEGEND_ROW = 34, LEGEND_GAP = 36;
const BAR_TOP = 214, BAR_HEIGHT = 12, BAR_GAP = 5, BAR_PITCH = 76;
const SPEED_TICKS = [0.1, 0.2, 0.3, 0.5, 0.75, 1, 1.5, 2, 3, 4, 5, 6, 8, 10, 15, 20, 30, 40, 50];

const f1 = value => value.toFixed(1);
const g = value => String(Number(value.toPrecision(6)));
const escText = value => String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
const escAttr = value => escText(value).replaceAll('"', '&quot;');

function element(parent, tag, attrs = {}, content) {
  const node = { tag, attrs, content, children: [] };
  parent?.children.push(node);
  return node;
}

function serialize(node, depth = 0) {
  const pad = '  '.repeat(depth);
  const attrs = Object.entries(node.attrs).map(([key, value]) => ` ${key}="${escAttr(value)}"`).join('');
  if (node.children.length) return `${pad}<${node.tag}${attrs}>\n${node.children.map(child => serialize(child, depth + 1)).join('\n')}\n${pad}</${node.tag}>`;
  if (node.content !== undefined) return `${pad}<${node.tag}${attrs}>${escText(node.content)}</${node.tag}>`;
  return `${pad}<${node.tag}${attrs} />`;
}

function text(parent, x, y, value, size, weight = 400, fill, anchor) {
  const attrs = { x, y, 'font-size': size, 'font-weight': weight };
  if (fill) attrs.fill = fill;
  if (anchor) attrs['text-anchor'] = anchor;
  return element(parent, 'text', attrs, value);
}

function textWidth(value, size) {
  return size * [...value].reduce((sum, char) => sum + ('ijlrtf-. '.includes(char) ? 0.3 : 0.56), 0);
}

function ink(fill) {
  const channels = [1, 3, 5].map(i => Number.parseInt(fill.slice(i, i + 2), 16) / 255).map(c => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2] > 0.18 ? '#16171d' : '#ffffff';
}

function niceStep(span) {
  const magnitude = 10 ** Math.floor(Math.log10(span / 8));
  return [1, 2, 5, 10].map(m => m * magnitude).find(step => span / step <= 8);
}

function typescriptName(version) {
  const [release, prerelease = ''] = version.split(/-(.*)/s);
  const minor = release.split('.').slice(0, 2).join('.');
  if (prerelease.startsWith('dev.')) return `TypeScript ${minor} nightly`;
  if (/^(beta|rc)$/.test(prerelease)) return `TypeScript ${minor} ${prerelease === 'rc' ? 'RC' : prerelease}`;
  return `TypeScript ${version}`;
}

function context(data) {
  const cells = {};
  for (const row of data.rows) ((cells[row.project] ??= {})[row.checkers] ??= {})[row.compiler] = row;
  const typescript = typescriptName(data.compilers.typescript.version);
  const { machine, settings } = data;
  const date = data.measuredAt.slice(0, 10);
  const releases = `TypeScript: typescript@${data.compilers.typescript.version}. tsrs: tsrs@${data.compilers.tsrs.version}, npm release binary`;
  const article = /^[aeiou8]/i.test(machine.cpu) ? 'an' : 'a';
  return {
    data, cells, typescript, date, time: `${date} ${data.measuredAt.slice(11, 16)} UTC`,
    label: name => RELEASE_PROJECTS[name] ?? name,
    names: { typescript, tsrs: 'tsrs (default)', tsrs_fast: 'tsrs --fast' },
    machinePhrase: `${article} ${machine.cpu}${settings.workers ? ` limited to ${settings.workers} threads` : ` (${machine.cpus} cores)`}`,
    machineLine: `${machine.cpu}, ${machine.cpus} cores, ${machine.memory_gb} GB${settings.workers ? `; GOMAXPROCS and RAYON_NUM_THREADS ${settings.workers}` : ''}`,
    reps: `median of ${settings.runs} interleaved runs (${settings.typescriptRuns} for TypeScript)`,
    scatterFooter: `${releases}, default mode. Measured ${date}.`,
  };
}

function root(width, height, title, description, metadata, colors) {
  const svg = element(null, 'svg', { xmlns: 'http://www.w3.org/2000/svg', width, height, viewBox: `0 0 ${width} ${height}`, role: 'img', 'aria-labelledby': 'title description' });
  element(svg, 'title', { id: 'title' }, title);
  element(svg, 'desc', { id: 'description' }, description);
  element(svg, 'metadata', {}, metadata);
  element(svg, 'rect', { x: 0.5, y: 0.5, width: width - 1, height: height - 1, rx: 4, fill: colors.background, stroke: colors.border });
  return svg;
}

function header(svg, colors, title, kind, subtitle) {
  const drawing = element(svg, 'g', { 'font-family': FONT, fill: colors.text });
  text(drawing, 48, 66, title, 26, 600);
  text(drawing, WIDTH - 48, 66, kind, 21, 400, colors.muted, 'end');
  text(drawing, 48, 98, subtitle, 17, 400, colors.muted);
  return drawing;
}

function footer(drawing, colors, height, ctx, first) {
  text(drawing, 48, height - 50, first, 14, 400, colors.muted);
  text(drawing, 48, height - 28, `${ctx.machineLine}. Wall time and peak RSS, ${ctx.reps}.`, 14, 400, colors.muted);
}

function legendEntry(parent, colors, x, y, fill, label) {
  element(parent, 'circle', { cx: f1(x), cy: y, r: 9, fill, stroke: colors.dot, 'stroke-width': 1.5 });
  text(parent, f1(x + 20), y + 7, label, 19, 500);
}

function scatterChart(ctx, theme) {
  const colors = THEMES[theme], { data } = ctx, checkerCounts = data.settings.scatterCheckers;
  const sweep = checkerCounts.length > 1 || checkerCounts[0] !== DEFAULT_MODE;
  const names = data.projects.filter(p => p.scatter).map(p => p.name);
  const series = Object.fromEntries(names.map(name => [name, checkerCounts.map(checkers => {
    const cell = ctx.cells[name][checkers];
    return { checkers, speed: cell.typescript.wallSeconds / cell.tsrs.wallSeconds, memory: cell.typescript.peakRssBytes / cell.tsrs.peakRssBytes, cell };
  })]));
  const all = Object.values(series).flat();
  const speeds = all.map(p => p.speed), memories = all.map(p => p.memory);
  const speedMin = Math.min(...speeds), memoryMin = Math.min(...memories), memoryMax = Math.max(...memories);
  const xStart = speedMin >= 1 ? 1 : SPEED_TICKS.findLast(tick => tick <= speedMin / 1.08) ?? speedMin / 1.08;
  const xEnd = SPEED_TICKS.find(tick => tick > 1 && tick >= Math.max(...speeds) * 1.08) ?? SPEED_TICKS.at(-1);
  const yStep = niceStep(Math.max(memoryMax - Math.min(memoryMin, 1), 0.5) * 8 / 10);
  const yEnd = 1 + Math.ceil(Math.max(memoryMax - 1, 0.5) / yStep + 0.5) * yStep;
  const yStart = memoryMin >= 1 ? 1 : Math.max(0, 1 - Math.ceil((1 - memoryMin) / yStep + 0.5) * yStep);
  const xOf = value => PLOT_LEFT + (PLOT_RIGHT - PLOT_LEFT) * Math.log(value / xStart) / Math.log(xEnd / xStart);
  const rows = [[]];
  for (const name of names) {
    const width = 20 + textWidth(ctx.label(name), 19);
    if (rows.at(-1).length && rows.at(-1).reduce((sum, [, w]) => sum + w + LEGEND_GAP, 0) + width > WIDTH - 96) rows.push([]);
    rows.at(-1).push([name, width]);
  }
  const plotTop = PLOT_TOP + LEGEND_ROW * (rows.length - 1);
  const yOf = value => PLOT_BOTTOM - (PLOT_BOTTOM - plotTop) * (value - yStart) / (yEnd - yStart);
  const projectColors = Object.fromEntries(names.map((name, i) => [name, colors.projects[i]]));
  const counts = `${checkerCounts.slice(0, -1).join(', ')}${checkerCounts.length > 1 ? ' and ' : ''}${checkerCounts.at(-1)}`;
  const format = row => `${row.wallSeconds.toFixed(2)} s, ${(row.peakRssBytes / GIB).toFixed(2)} GiB`;
  const details = names.map(name => `${ctx.label(name)}: ${series[name].map(p => `${sweep ? `--checkers ${p.checkers}: ` : ''}${p.speed.toFixed(1)}x faster, ${p.memory.toFixed(1)}x less memory (TypeScript ${format(p.cell.typescript)}; tsrs ${format(p.cell.tsrs)})`).join('; ')}.`);
  const title = `tsrs vs ${ctx.typescript}`;
  const axes = xStart === 1 && yStart === 1 ? 'Both axes start at 1x, parity with TypeScript' : 'Dashed lines mark 1x, parity with TypeScript; points below 1x are slower or use more memory than TypeScript';
  const description = `How many times faster default tsrs type-checks than ${ctx.typescript}, against how many times less peak memory it uses, both compilers ${sweep ? `at --checkers ${counts}` : 'with default settings'} on ${ctx.machinePhrase}. ${axes}; the speed axis is logarithmic. Up and to the right is better. ${details.join(' ')} ${ctx.reps[0].toUpperCase()}${ctx.reps.slice(1)}. ${ctx.scatterFooter} Machine: ${ctx.machineLine}.`;
  const metadata = `Generated by bench/release.mjs; modes: ${checkerCounts.map(c => (c === DEFAULT_MODE ? c : `checkers${c}`)).join(', ')}; typescript@${data.compilers.typescript.version}; tsrs@${data.compilers.tsrs.version}; measured: ${ctx.time}. Details: https://github.com/oxc-project/tsrs/blob/main/bench/RESULTS.md`;
  const svg = root(WIDTH, HEIGHT, title, description, metadata, colors);
  const drawing = header(svg, colors, title, 'Speed vs peak memory', `Both compilers ${sweep ? 'at the same checker count' : 'with default settings'} on ${ctx.machinePhrase}`);
  if (sweep) {
    const key = element(drawing, 'g', { 'data-role': 'key' });
    const keyText = `parallel checker threads (--checkers ${checkerCounts.join(', ')})`;
    const keyX = WIDTH - 48 - textWidth(keyText, 17);
    text(key, f1(keyX), 98, keyText, 17, 400, colors.muted);
    element(key, 'circle', { cx: f1(keyX - 22), cy: 92, r: 13, fill: colors.background, stroke: colors.text, 'stroke-width': 1.5 });
    text(key, f1(keyX - 22), 97, 'n', 14, 700, colors.text, 'middle');
  }
  const legend = element(drawing, 'g', { 'data-role': 'legend' });
  for (const [row, items] of rows.entries()) {
    let x = WIDTH - 48;
    for (const [name, width] of items.toReversed()) {
      x -= width;
      legendEntry(legend, colors, x, LEGEND_Y + row * LEGEND_ROW, projectColors[name], ctx.label(name));
      x -= LEGEND_GAP;
    }
  }
  const grid = element(drawing, 'g', { 'data-role': 'grid' });
  for (let k = 0; k <= Math.round((yEnd - yStart) / yStep); k++) {
    const value = yStart + k * yStep, y = yOf(value);
    if (k) element(grid, 'line', { x1: PLOT_LEFT, y1: f1(y), x2: PLOT_RIGHT, y2: f1(y), stroke: colors.grid, 'stroke-width': 1.5 });
    text(grid, PLOT_LEFT - 18, f1(y + 7), `${g(value)}x`, 19, 400, colors.muted, 'end');
  }
  for (const value of SPEED_TICKS.filter(tick => tick >= xStart && tick <= xEnd)) {
    const x = xOf(value);
    element(grid, 'line', { x1: f1(x), y1: PLOT_BOTTOM, x2: f1(x), y2: PLOT_BOTTOM + 8, stroke: colors.axis, 'stroke-width': 1.5 });
    text(grid, f1(x), PLOT_BOTTOM + 36, `${g(value)}x`, 19, 400, colors.muted, 'middle');
  }
  if (xStart < 1) element(grid, 'line', { 'data-role': 'parity', x1: f1(xOf(1)), y1: plotTop, x2: f1(xOf(1)), y2: PLOT_BOTTOM, stroke: colors.axis, 'stroke-width': 1.5, 'stroke-dasharray': '6 6' });
  if (yStart < 1) element(grid, 'line', { 'data-role': 'parity', x1: PLOT_LEFT, y1: f1(yOf(1)), x2: PLOT_RIGHT, y2: f1(yOf(1)), stroke: colors.axis, 'stroke-width': 1.5, 'stroke-dasharray': '6 6' });
  element(grid, 'path', { d: `M${PLOT_LEFT} ${plotTop}V${PLOT_BOTTOM}H${PLOT_RIGHT}`, fill: 'none', stroke: colors.axis, 'stroke-width': 1.5 });
  text(grid, (PLOT_LEFT + PLOT_RIGHT) / 2, PLOT_BOTTOM + 84, `Times faster than ${ctx.typescript}, log scale`, 21, 400, undefined, 'middle');
  text(grid, 0, 0, `Times less peak memory than ${ctx.typescript}`, 21, 400, undefined, 'middle').attrs.transform = `translate(${PLOT_LEFT - 78} ${(plotTop + PLOT_BOTTOM) / 2}) rotate(-90)`;
  const plot = element(drawing, 'g', { 'data-role': 'points' });
  if (sweep) for (const name of names) element(plot, 'polyline', { 'data-project': name, fill: 'none', stroke: projectColors[name], 'stroke-width': 3.5, 'stroke-linejoin': 'round', 'stroke-linecap': 'round', points: series[name].map(p => `${f1(xOf(p.speed))},${f1(yOf(p.memory))}`).join(' ') });
  for (const name of names) {
    const group = element(plot, 'g', { 'data-project': name });
    for (const point of series[name]) {
      const x = xOf(point.speed), y = yOf(point.memory);
      element(group, 'circle', { 'data-checkers': point.checkers, 'data-speedup': Number(point.speed.toPrecision(6)), 'data-memory': Number(point.memory.toPrecision(6)), cx: f1(x), cy: f1(y), r: sweep ? 13 : 11, fill: projectColors[name], stroke: colors.dot, 'stroke-width': 1.5 });
      if (sweep) text(group, f1(x), f1(y + 5), String(point.checkers), 14, 700, ink(projectColors[name]), 'middle');
    }
  }
  footer(drawing, colors, HEIGHT, ctx, ctx.scatterFooter);
  return serialize(svg) + '\n';
}

function times(reference, value, better, worse) {
  const r = reference / value;
  return r >= 1 ? `${r.toFixed(1)}× ${better}` : `${(1 / r).toFixed(1)}× ${worse}`;
}

function barChart(ctx) {
  const colors = BAR_COLORS, { data, cells, names } = ctx, checkers = data.settings.detailCheckers;
  const compilers = ['tsrs_fast', 'tsrs', 'typescript'];
  const order = data.projects.map(p => p.name).filter(name => cells[name][checkers]).sort((a, b) => cells[b][checkers].typescript.wallSeconds - cells[a][checkers].typescript.wallSeconds);
  const panels = [
    { metric: 'wallSeconds', title: 'Check time', x: 236, right: 680, unit: 1, suffix: 's', better: 'faster', worse: 'slower' },
    { metric: 'peakRssBytes', title: 'Peak memory', x: 736, right: WIDTH - 48, unit: GIB, suffix: 'GiB', better: 'less', worse: 'more' },
  ];
  const value = (name, compiler, panel) => cells[name][checkers][compiler][panel.metric] / panel.unit;
  const labels = (name, compiler, panel) => [`${value(name, compiler, panel).toFixed(2)} ${panel.suffix}`, compiler === 'typescript' ? null : times(value(name, 'typescript', panel), value(name, compiler, panel), panel.better, panel.worse)];
  const labelWidth = (name, compiler, panel) => {
    const [amount, change] = labels(name, compiler, panel);
    return textWidth(amount, 14) + (change ? 12 + textWidth(change, 14) * 1.08 : 0);
  };
  for (const panel of panels) panel.scale = Math.min(...order.flatMap(name => compilers.map(compiler => (panel.right - panel.x - 12 - labelWidth(name, compiler, panel)) / value(name, compiler, panel))));
  const groupHeight = compilers.length * BAR_HEIGHT + (compilers.length - 1) * BAR_GAP;
  const bottom = BAR_TOP + (order.length - 1) * BAR_PITCH + groupHeight;
  const height = bottom + 48;
  const title = `tsrs vs ${ctx.typescript}`;
  const setting = checkers === DEFAULT_MODE ? 'Each compiler with its default settings' : `All three at --checkers ${checkers}`;
  const description = `${setting} on ${ctx.machinePhrase}; ${ctx.reps}. ${order.map(name => `${ctx.label(name)}: ${panels.map(panel => `${panel.title.toLowerCase()} ${compilers.map(compiler => `${names[compiler]} ${labels(name, compiler, panel).filter(Boolean).join(', ')}`).join('; ')}`).join('. ')}.`).join(' ')} Shorter bars are better.`;
  const metadata = `Generated by bench/release.mjs; mode: ${checkers === DEFAULT_MODE ? checkers : `checkers${checkers}`}; typescript@${data.compilers.typescript.version}; tsrs@${data.compilers.tsrs.version}; measured: ${ctx.time}. Details: https://github.com/oxc-project/tsrs/blob/main/bench/RESULTS.md`;
  const svg = root(WIDTH, height, title, description, metadata, colors);
  const drawing = header(svg, colors, title, 'Time and memory by project', `${setting} on ${ctx.machinePhrase}, sorted by TypeScript's check time`);
  const legend = element(drawing, 'g', { 'data-role': 'legend' });
  let x = WIDTH - 48;
  for (const compiler of compilers.toReversed()) {
    x -= 22 + textWidth(names[compiler], 17);
    element(legend, 'rect', { x: f1(x), y: 129, width: 14, height: 14, rx: 3, fill: colors[compiler] });
    text(legend, f1(x + 22), 142, names[compiler], 17, 500);
    x -= 32;
  }
  const table = element(drawing, 'g', { 'data-role': 'bars' });
  for (const panel of panels) {
    text(table, panel.x, BAR_TOP - 34, panel.title, 17, 600);
    element(table, 'line', { x1: panel.x, y1: BAR_TOP - 6, x2: panel.x, y2: bottom + 6, stroke: colors.axis, 'stroke-width': 1 });
  }
  for (const [index, name] of order.entries()) {
    const top = BAR_TOP + index * BAR_PITCH;
    element(table, 'line', { x1: 48, y1: top - 15, x2: WIDTH - 48, y2: top - 15, stroke: colors.rule, 'stroke-width': 1 });
    const row = element(table, 'g', { 'data-project': name });
    text(row, 48, f1(top + groupHeight / 2 + 6), ctx.label(name), 17, 500);
    for (const panel of panels) for (const [i, compiler] of compilers.entries()) {
      const y = top + i * (BAR_HEIGHT + BAR_GAP), length = Math.max(2, value(name, compiler, panel) * panel.scale);
      const [amount, change] = labels(name, compiler, panel);
      element(row, 'rect', { 'data-compiler': compiler, 'data-metric': panel.metric, x: panel.x, y, width: f1(length), height: BAR_HEIGHT, rx: 2, fill: colors[compiler] });
      text(row, f1(panel.x + length + 8), y + 11, amount, 14, 400, colors.muted);
      if (change) text(row, f1(panel.x + length + 20 + textWidth(amount, 14)), y + 11, change, 14, 600);
    }
  }
  return serialize(svg) + '\n';
}

function seconds(value) { return `${value.toFixed(2)} s`; }
function memory(bytes) { return bytes >= 1000 * 2 ** 20 ? `${(bytes / GIB).toFixed(2)} GiB` : `${Math.round(bytes / 2 ** 20)} MiB`; }
function ratio(reference, value, better, worse) { const r = reference / value; return r >= 1 ? `${r.toFixed(2)}x ${better}` : `${(1 / r).toFixed(2)}x ${worse}`; }

function picture(name, alt) {
  return ['<picture>', `  <source media="(prefers-color-scheme: dark)" srcset="${name}-dark.svg">`, `  <img alt="${alt}" src="${name}.svg">`, '</picture>', ''];
}

function report(ctx) {
  const { data, cells, names } = ctx, { settings, machine, compilers } = data, checkers = settings.detailCheckers;
  const reference = compilers.tsrs.version_output?.match(/microsoft\/TypeScript@([0-9a-f]+)/)?.[1];
  const order = data.projects.map(p => p.name).sort((a, b) => cells[b][checkers].typescript.wallSeconds - cells[a][checkers].typescript.wallSeconds);
  const scatter = data.projects.filter(p => p.scatter).map(p => p.name);
  const sweep = settings.scatterCheckers.length > 1;
  const modes = [...new Set([...settings.scatterCheckers, checkers])];
  const counted = modes.some(mode => mode !== DEFAULT_MODE);
  const phrase = mode => (mode === DEFAULT_MODE ? 'with default settings' : `at \`--checkers ${mode}\``);
  const threads = settings.workers ? `, with \`GOMAXPROCS\` and \`RAYON_NUM_THREADS\` set to ${settings.workers}.` : '.';
  const defaults = modes.includes(DEFAULT_MODE) ? ` Default-settings runs pass no \`--checkers\`${settings.workers ? '' : ' and set no thread limit'}, so each compiler picks its own checker count${settings.workers ? '' : ` and uses all ${machine.cpus} cores`}.` : '';
  const lines = [
    '# Benchmark results', '',
    '<!-- Generated by `node bench/bench.mjs render`; edit bench/release.mjs, not this file. -->', '',
    `npm release binaries, measured ${ctx.date}: TypeScript ${compilers.typescript.version} (\`${compilers.typescript.package}\`) and tsrs ${compilers.tsrs.version} (\`${compilers.tsrs.package}\`)${reference ? `, which ports microsoft/TypeScript@${reference}` : ''}. tsrs runs in its default mode and with the fast preset (\`--fast\`).`, '',
    `Each command is \`-p <project> --noEmit --incremental false --pretty false${counted ? ' [--checkers N]' : ''}\`, plus \`--fast\` for tsrs --fast, on ${/^[aeiou8]/i.test(machine.cpu) ? 'an' : 'a'} ${machine.cpu} (${machine.cpus} cores, ${machine.memory_gb} GB, ${machine.platform})${threads}${defaults} Values are medians of ${settings.runs} interleaved runs (${settings.typescriptRuns} for TypeScript), measured with ${data.hyperfine.replace(/^hyperfine/, 'Hyperfine')}. An untimed run of every command first warms the file cache and records its diagnostics. Each timed run waits until other processes use at most ${settings.quietCores} CPU cores, for up to ${settings.quietWaitSeconds} s; ${data.samples.contended} of ${data.samples.total} runs started after that wait expired.`, '',
    `The bar chart and first tables compare all three ${phrase(checkers)}. The speed-vs-memory chart compares default tsrs with TypeScript${sweep ? ` at \`--checkers ${settings.scatterCheckers.join(', ')}\`` : `, both ${phrase(settings.scatterCheckers[0])}`}. Pinned projects are in [projects.json](projects.json); see the [methodology](README.md).`, '',
    '![Check time and peak memory by project](benchmark.svg)', '',
    ...picture('large-projects', 'Default tsrs type-check speedup against peak-memory ratio'),
    checkers === DEFAULT_MODE ? '## Default settings' : `## \`--checkers ${checkers}\``, '',
  ];
  for (const [title, metric, format, better, worse] of [['Wall time', 'wallSeconds', seconds, 'faster', 'slower'], ['Peak memory', 'peakRssBytes', memory, 'less', 'more']]) {
    lines.push(`**${title}**`, '', `| project | ${names.typescript} | tsrs | tsrs --fast | tsrs vs TypeScript | tsrs --fast vs TypeScript |`, '| --- | ---: | ---: | ---: | ---: | ---: |');
    for (const name of order) {
      const cell = cells[name][checkers];
      lines.push(`| ${name} | ${format(cell.typescript[metric])} | ${format(cell.tsrs[metric])} | ${format(cell.tsrs_fast[metric])} | ${ratio(cell.typescript[metric], cell.tsrs[metric], better, worse)} | ${ratio(cell.typescript[metric], cell.tsrs_fast[metric], better, worse)} |`);
    }
    lines.push('');
  }
  if (sweep) lines.push('## Checker scaling', '', `The speed-vs-memory chart's projects at every measured checker count.`, '');
  for (const [title, metric, format, better, worse] of sweep ? [['Wall time', 'wallSeconds', seconds, 'faster', 'slower'], ['Peak memory', 'peakRssBytes', memory, 'less', 'more']] : []) {
    lines.push(`**${title}**`, '', `| project | checkers | ${names.typescript} | tsrs | tsrs --fast | tsrs vs TypeScript | tsrs --fast vs TypeScript |`, '| --- | ---: | ---: | ---: | ---: | ---: | ---: |');
    for (const name of scatter) for (const n of settings.scatterCheckers) {
      const cell = cells[name][n];
      lines.push(`| ${name} | ${n} | ${format(cell.typescript[metric])} | ${format(cell.tsrs[metric])} | ${format(cell.tsrs_fast[metric])} | ${ratio(cell.typescript[metric], cell.tsrs[metric], better, worse)} | ${ratio(cell.typescript[metric], cell.tsrs_fast[metric], better, worse)} |`);
    }
    lines.push('');
  }
  const differences = data.projects.flatMap(p => p.checkers.flatMap(n => ['tsrs', 'tsrs_fast'].filter(c => cells[p.name][n][c].diagnosticsSha256 !== cells[p.name][n].typescript.diagnosticsSha256).map(c => `${p.name} (${n === DEFAULT_MODE ? 'default settings' : `\`--checkers ${n}\``}, ${names[c]}: ${cells[p.name][n][c].diagnostics}, TypeScript: ${cells[p.name][n].typescript.diagnostics})`)));
  const counts = order.map(name => `${name} ${cells[name][checkers].typescript.diagnostics}`).join(', ');
  lines.push('## Diagnostics', '');
  lines.push(differences.length ? `TypeScript, tsrs and tsrs --fast differ on ${differences.join('; ')}. The other runs report identical diagnostics.` : `TypeScript, tsrs and tsrs --fast report identical diagnostics (file, position and code) for every project${sweep ? ' and checker count' : ''}. Errors${checkers === DEFAULT_MODE ? '' : ` at \`--checkers ${checkers}\``}: ${counts}.`, '');
  const early = data.projects.filter(p => { const codes = Object.keys(cells[p.name][checkers].typescript.diagnosticCodes ?? {}); return codes.length && codes.every(code => OPTION_ERRORS_BEFORE_CHECK.has(code)); });
  for (const project of early) lines.push(`${project.name} reports only ${Object.keys(cells[project.name][checkers].typescript.diagnosticCodes).join(', ')}: a composite project cannot disable incremental compilation. This option error stops all three compilers before semantic checking, so the ${project.name} times cover loading, parsing and binding.`, '');
  return lines.join('\n');
}

export function releaseAssets(data) {
  const ctx = context(data);
  return {
    'benchmark.svg': barChart(ctx),
    'large-projects.svg': scatterChart(ctx, 'light'),
    'large-projects-dark.svg': scatterChart(ctx, 'dark'),
    'RESULTS.md': report(ctx),
  };
}
