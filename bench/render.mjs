import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { BENCH, fileSha, requireValue, writeJson } from './lib.mjs';
import { releaseAssets } from './release.mjs';
import { readPublication, rendererIdentity } from './results.mjs';

const COLORS = { tsrs: '#0d6a73', bun: '#c2255c', typescript_1: '#3178c6', typescript_2: '#76a7d7', typescript_3: '#18405f' };
const PALETTE = { bg: '#f4f3ec', fg: '#16171d', muted: '#6f6876', grid: '#dcdad0', panel: '#ffffff' };
const GIB = 2 ** 30;
const esc = value => String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
const num = value => Number(value.toFixed(2));

function label(parts, value, x, y, size, fill, { bold = false, anchor = 'start' } = {}) {
  parts.push(`<text x="${num(x)}" y="${num(y)}" font-size="${num(size)}" font-weight="${bold ? 700 : 400}" text-anchor="${anchor}" fill="${fill}" aria-hidden="true">${esc(value)}</text>`);
}

function fittedLabel(parts, value, x, y, preferredSize, maxWidth, fill, options = {}) {
  const size = Math.max(9, Math.min(preferredSize, maxWidth / (String(value).length * 0.6)));
  label(parts, value, x, y, size, fill, options);
}

function rect(parts, x, y, width, height, fill, extra = '') { parts.push(`<rect x="${num(x)}" y="${num(y)}" width="${num(width)}" height="${num(height)}" fill="${fill}" ${extra}/>`); }
function line(parts, x1, y1, x2, y2, color, width = 1) { parts.push(`<line x1="${num(x1)}" y1="${num(y1)}" x2="${num(x2)}" y2="${num(y2)}" stroke="${color}" stroke-width="${width}"/>`); }

function svg(width, height, title, description, draw) {
  const parts = [];
  rect(parts, 0, 0, width, height, PALETTE.bg);
  draw(parts, PALETTE);
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" font-family="system-ui, sans-serif" role="img" aria-labelledby="title description">\n<title id="title">${esc(title)}</title>\n<desc id="description">${esc(description)}</desc>\n${parts.join('\n')}\n</svg>\n`;
}

function compilerLabel(data, id) { const info = data.compilers[id]; return id === 'tsrs' ? (data.settings.tsrsFast ? 'tsrs (fast)' : 'tsrs') : id === 'bun' ? `bun check ${info.version ?? ''}` : `TypeScript ${info.version}`; }
function color(id) { return COLORS[id] ?? '#91b8e3'; }
function row(data, project, compiler) { return data.rows.find(r => r.project === project && r.compiler === compiler); }
function titleBlock(parts, palette, width, title, subtitle) {
  label(parts, title, 40, 57, 24, palette.fg, { bold: true });
  fittedLabel(parts, subtitle, 40, 86, 13, width - 80, palette.muted);
  line(parts, 40, 103, width - 40, 103, palette.grid);
}

function barChart(data) {
  const projects = data.projects.map(p => p.name), compilers = data.compilerOrder;
  const width = 1400, rowHeight = Math.max(70, compilers.length * 26 + 20), height = 165 + projects.length * rowHeight + 95;
  const description = `Wall time and peak resident memory for ${projects.length} projects. Arithmetic means of ${data.settings.runs} runs, with ${data.settings.warmup} warmups. ${compilers.map(c => compilerLabel(data, c)).join(', ')}.`;
  return svg(width, height, 'TypeScript type checking benchmark', description, (parts, palette) => {
    titleBlock(parts, palette, width, 'TypeScript type checking', `${data.machine.label}. ${data.settings.checkers} checkers, ${data.settings.workers} worker CPUs.`);
    let lx = 45;
    for (const id of compilers) { rect(parts, lx, 122, 14, 14, color(id)); label(parts, compilerLabel(data, id), lx + 22, 135, 13, palette.fg); lx += 190; }
    const columns = [{ key: 'wallSeconds', title: 'Wall time', left: 220, width: 420, unit: 's' }, { key: 'peakRssBytes', title: 'Peak RSS', left: 800, width: 420, unit: 'GiB' }];
    label(parts, 'Project', 42, 174, 14, palette.fg, { bold: true });
    for (const c of columns) label(parts, c.title, c.left, 174, 14, palette.fg, { bold: true });
    const maxima = Object.fromEntries(columns.map(c => [c.key, Math.max(...data.rows.map(r => c.key === 'peakRssBytes' ? r[c.key] / GIB : r[c.key])) * 1.2]));
    for (const [index, name] of projects.entries()) {
      const top = 185 + index * rowHeight;
      if (index % 2 === 0) rect(parts, 30, top, width - 60, rowHeight, palette.panel);
      label(parts, name, 42, top + rowHeight / 2 + 5, 14, palette.fg, { bold: true });
      for (const [ci, id] of compilers.entries()) {
        const item = row(data, name, id), y = top + 10 + ci * 26;
        for (const column of columns) {
          const value = column.key === 'peakRssBytes' ? item.peakRssBytes / GIB : item.wallSeconds;
          const bar = Math.max(2, value / maxima[column.key] * column.width);
          rect(parts, column.left, y, bar, 16, color(id));
          label(parts, `${value.toFixed(2)} ${column.unit}`, column.left + bar + 8, y + 13, 12, palette.fg);
        }
      }
    }
    label(parts, `Means per compiler and project; no averaging across projects. Measured ${data.measuredAt.slice(0, 10)}.`, 40, height - 35, 12, palette.muted);
  });
}

function report(data) {
  const lines = ['# Benchmark results', '', `Measured ${data.measuredAt} on ${data.machine.label}.`, '', `Arithmetic means of ${data.settings.runs} runs after ${data.settings.warmup} warmups per compiler and project. Projects are never averaged together.`, '', `${data.settings.checkers} checkers and ${data.settings.workers} worker CPUs per compiler.`, ''];
  if (data.settings.tsrsFast) lines.push('tsrs runs in fast mode (`TSRS_FAST=1`); TypeScript keeps its own checker behavior.', '');
  if (data.measuredCommit) lines.push(`tsrs commit: \`${data.measuredCommit}\`.`, '');
  if (data.runUrl) lines.push(`[Benchmark run and raw-data artifacts](${data.runUrl}).`, '');
  lines.push('[Methodology](README.md).', '', '![All measured compiler versions](benchmark.svg)', '', '| Project | Compiler | Wall time | Peak memory | Instructions | Files | Diagnostics |', '| --- | --- | ---: | ---: | ---: | ---: | ---: |');
  for (const project of data.projects) for (const id of data.compilerOrder) {
    const item = row(data, project.name, id);
    lines.push(`| ${project.name} | ${compilerLabel(data, id)} | ${item.wallSeconds.toFixed(3)} s | ${(item.peakRssBytes / GIB).toFixed(3)} GiB | ${item.instructions == null ? 'n/a' : Math.round(item.instructions).toLocaleString('en-US')} | ${item.files ?? 'n/a'} | ${item.diagnostics ?? 'n/a'} |`);
  }
  lines.push('', 'File counts and diagnostics were collected in separate, untimed runs.', '');
  return lines.join('\n');
}

export async function render(input, output, { commit, runUrl } = {}) {
  const renderer = await rendererIdentity([path.join(BENCH, 'render.mjs'), path.join(BENCH, 'results.mjs'), path.join(BENCH, 'lib.mjs'), path.join(BENCH, 'release.mjs'), path.join(BENCH, '..', 'pnpm-lock.yaml')]);
  const data = await readPublication(input, renderer);
  if (commit) data.measuredCommit = commit;
  if (runUrl) data.runUrl = runUrl;
  if (data.renderer) requireValue(data.renderer.sha256 === renderer.sha256, `Renderer source differs; check out ${data.renderer.revision}`);
  await mkdir(output, { recursive: true });
  const assets = data.kind === 'release-publication' ? releaseAssets(data) : {
    'benchmark.svg': barChart(data),
    'RESULTS.md': report(data),
  };
  await writeJson(path.join(output, 'publication.json'), data);
  const hashes = {};
  for (const [name, content] of Object.entries(assets)) { await writeFile(path.join(output, name), content); hashes[name] = await fileSha(path.join(output, name)); }
  const means = data.rows.map(r => ({ ...r, peakRssGiB: r.peakRssBytes / GIB }));
  await writeJson(path.join(output, 'means.json'), means);
  await writeFile(path.join(output, 'means.csv'), ['project,checkers,compiler,wall_seconds,instructions,peak_rss_bytes,files,diagnostics', ...means.map(r => [r.project, r.checkers ?? data.settings.checkers, r.compiler, r.wallSeconds, r.instructions, r.peakRssBytes, r.files, r.diagnostics].join(','))].join('\n') + '\n');
  await writeJson(path.join(output, 'render-manifest.json'), { publicationSha256: await fileSha(path.join(output, 'publication.json')), renderer, assets: hashes });
  return { output, assets: hashes };
}
