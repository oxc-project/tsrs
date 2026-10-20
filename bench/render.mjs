import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import opentype from 'opentype.js';
import { BENCH, fileSha, requireValue, writeJson } from './lib.mjs';
import { readPublication, rendererIdentity } from './results.mjs';

const FONT_FILES = [path.join(BENCH, 'fonts', 'DejaVuSans.ttf'), path.join(BENCH, 'fonts', 'DejaVuSans-Bold.ttf')];
const FONTS = FONT_FILES.map(file => opentype.parse(readFileSync(file)));
const COLORS = { tsrs: '#0d6a73', bun: '#c2255c', typescript_1: '#3178c6', typescript_2: '#76a7d7', typescript_3: '#18405f' };
const THEMES = {
  light: { bg: '#f4f3ec', fg: '#16171d', muted: '#6f6876', grid: '#dcdad0', panel: '#ffffff' },
  dark: { bg: '#16171d', fg: '#ffffff', muted: '#b0a8b9', grid: '#3b3440', panel: '#22232a' },
};
const GIB = 2 ** 30;
const esc = value => String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
const num = value => Number(value.toFixed(2));

function glyphs(value, x, baseline, size, bold = false) {
  const font = FONTS[bold ? 1 : 0];
  let width = 0, data = '', previous;
  for (const character of String(value)) {
    const glyph = font.charToGlyph(character);
    if (previous) width += font.getKerningValue(previous, glyph) * size / font.unitsPerEm;
    data += glyph.getPath(x + width, baseline, size).toPathData(2);
    width += glyph.advanceWidth * size / font.unitsPerEm;
    previous = glyph;
  }
  return { width, data };
}

function label(parts, value, x, y, size, fill, { bold = false, anchor = 'start' } = {}) {
  const shape = glyphs(value, 0, 0, size, bold);
  const dx = anchor === 'end' ? -shape.width : anchor === 'middle' ? -shape.width / 2 : 0;
  parts.push(`<path d="${shape.data}" transform="translate(${num(x + dx)} ${num(y)})" fill="${fill}" aria-hidden="true"/>`);
}

function fittedLabel(parts, value, x, y, preferredSize, maxWidth, fill, options = {}) {
  let size = preferredSize;
  while (size > 9 && glyphs(value, 0, 0, size, options.bold).width > maxWidth) size -= 0.5;
  label(parts, value, x, y, size, fill, options);
}

function rect(parts, x, y, width, height, fill, extra = '') { parts.push(`<rect x="${num(x)}" y="${num(y)}" width="${num(width)}" height="${num(height)}" fill="${fill}" ${extra}/>`); }
function line(parts, x1, y1, x2, y2, color, width = 1) { parts.push(`<line x1="${num(x1)}" y1="${num(y1)}" x2="${num(x2)}" y2="${num(y2)}" stroke="${color}" stroke-width="${width}"/>`); }
function circle(parts, x, y, radius, color) { parts.push(`<circle cx="${num(x)}" cy="${num(y)}" r="${radius}" fill="${color}"/>`); }

function svg(width, height, title, description, draw, theme = 'light') {
  const palette = THEMES[theme], parts = [];
  rect(parts, 0, 0, width, height, palette.bg);
  draw(parts, palette);
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-labelledby="title description">\n<title id="title">${esc(title)}</title>\n<desc id="description">${esc(description)}</desc>\n${parts.join('\n')}\n</svg>\n`;
}

function compilerLabel(data, id) { const info = data.compilers[id]; return id === 'tsrs' ? 'tsrs' : id === 'bun' ? `bun check ${info.version ?? ''}` : `TypeScript ${info.version}`; }
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

function projectChart(data, metric, theme) {
  const key = metric === 'time' ? 'wallSeconds' : 'peakRssBytes', unit = metric === 'time' ? 's' : 'GiB';
  const title = metric === 'time' ? 'Check time by project' : 'Peak memory by project';
  const reference = data.compilerOrder.find(id => id !== 'tsrs');
  const projects = data.projects.map(p => p.name).toSorted((a, b) => row(data, b, reference)[key] - row(data, a, reference)[key]);
  const width = 1200, rowHeight = Math.max(48, data.compilerOrder.length * 12 + 10), height = 185 + projects.length * rowHeight + 65;
  const max = Math.max(...data.rows.map(r => key === 'peakRssBytes' ? r[key] / GIB : r[key])) * 1.16;
  const description = projects.map(name => `${name}: ${data.compilerOrder.map(id => `${compilerLabel(data, id)} ${(key === 'peakRssBytes' ? row(data, name, id)[key] / GIB : row(data, name, id)[key]).toFixed(2)} ${unit}`).join(', ')}`).join('. ') + '. Lower is better.';
  return svg(width, height, title, description, (parts, palette) => {
    titleBlock(parts, palette, width, title, `${data.settings.checkers} checkers per compiler; mean of ${data.settings.runs} runs.`);
    let lx = 45;
    for (const id of data.compilerOrder) { rect(parts, lx, 119, 12, 12, color(id)); label(parts, compilerLabel(data, id), lx + 20, 131, 12, palette.fg); lx += 205; }
    const left = 205, plotWidth = 890;
    for (let tick = 0; tick <= 4; tick++) {
      const x = left + tick * plotWidth / 4;
      line(parts, x, 166, x, height - 65, palette.grid);
      label(parts, `${(max * tick / 4).toFixed(1)} ${unit}`, x, height - 44, 11, palette.muted, { anchor: 'middle' });
    }
    for (const [i, name] of projects.entries()) {
      const y = 170 + i * rowHeight;
      label(parts, name, 188, y + 21, 12, palette.fg, { anchor: 'end' });
      for (const [j, id] of data.compilerOrder.entries()) {
        const raw = row(data, name, id)[key], value = key === 'peakRssBytes' ? raw / GIB : raw;
        const bar = value / max * plotWidth;
        rect(parts, left, y + j * 12, bar, 9, color(id));
      }
    }
  }, theme);
}

function scatterChart(data, theme) {
  const reference = data.compilerOrder.find(id => id !== 'tsrs');
  requireValue(reference, 'A TypeScript compiler is required for the scatter chart');
  const points = data.projects.map(p => {
    const a = row(data, p.name, 'tsrs'), b = row(data, p.name, reference);
    return { name: p.name, speed: b.wallSeconds / a.wallSeconds, memory: b.peakRssBytes / a.peakRssBytes };
  });
  const width = 1200, height = 830, left = 140, top = 150, plotWidth = 900, plotHeight = 560;
  const xmax = Math.max(1, ...points.map(p => p.speed)) * 1.15, ymax = Math.max(1, ...points.map(p => p.memory)) * 1.15;
  const description = points.map(p => `${p.name}: tsrs ${p.speed.toFixed(2)} times faster and ${p.memory.toFixed(2)} times less peak memory than ${compilerLabel(data, reference)}`).join('. ') + '.';
  return svg(width, height, `tsrs vs ${compilerLabel(data, reference)}`, description, (parts, palette) => {
    titleBlock(parts, palette, width, `tsrs vs ${compilerLabel(data, reference)}`, `Mean wall time and peak memory on ${data.machine.label}. Higher and farther right is better.`);
    for (let i = 0; i <= 5; i++) {
      const x = left + i * plotWidth / 5, y = top + i * plotHeight / 5;
      line(parts, x, top, x, top + plotHeight, palette.grid);
      line(parts, left, y, left + plotWidth, y, palette.grid);
      label(parts, `${(i * xmax / 5).toFixed(1)}×`, x, top + plotHeight + 24, 11, palette.muted, { anchor: 'middle' });
      label(parts, `${((5 - i) * ymax / 5).toFixed(1)}×`, left - 15, y + 4, 11, palette.muted, { anchor: 'end' });
    }
    for (const point of points) {
      const x = left + point.speed / xmax * plotWidth, y = top + plotHeight - point.memory / ymax * plotHeight;
      circle(parts, x, y, 7, color('tsrs'));
      label(parts, point.name, x + 12, y - 8, 11, palette.fg);
    }
    label(parts, 'Wall-time speedup (TypeScript / tsrs)', left + plotWidth / 2, height - 44, 14, palette.fg, { anchor: 'middle' });
    label(parts, 'Peak-memory ratio (TypeScript / tsrs)', left, 132, 12, palette.muted);
  }, theme);
}

function report(data) {
  const lines = ['# Benchmark results', '', `Measured ${data.measuredAt} on ${data.machine.label}.`, '', `Arithmetic means of ${data.settings.runs} runs after ${data.settings.warmup} warmups per compiler and project. Projects are never averaged together.`, '', `${data.settings.checkers} checkers and ${data.settings.workers} worker CPUs per compiler.`, ''];
  if (data.measuredCommit) lines.push(`tsrs commit: \`${data.measuredCommit}\`.`, '');
  if (data.runUrl) lines.push(`[Benchmark run and raw-data artifacts](${data.runUrl}).`, '');
  lines.push('[Methodology](README.md).', '', '![All measured compiler versions](benchmark.svg)', '', '<picture>', '  <source media="(prefers-color-scheme: dark)" srcset="large-projects-dark.svg">', '  <img alt="Type-check speedup against peak-memory ratio" src="large-projects.svg">', '</picture>', '', '<picture>', '  <source media="(prefers-color-scheme: dark)" srcset="per-project-time-dark.svg">', '  <img alt="Check time by project" src="per-project-time.svg">', '</picture>', '', '<picture>', '  <source media="(prefers-color-scheme: dark)" srcset="per-project-memory-dark.svg">', '  <img alt="Peak memory by project" src="per-project-memory.svg">', '</picture>', '', '| Project | Compiler | Wall time | Peak memory | Instructions | Files | Diagnostics |', '| --- | --- | ---: | ---: | ---: | ---: | ---: |');
  for (const project of data.projects) for (const id of data.compilerOrder) {
    const item = row(data, project.name, id);
    lines.push(`| ${project.name} | ${compilerLabel(data, id)} | ${item.wallSeconds.toFixed(3)} s | ${(item.peakRssBytes / GIB).toFixed(3)} GiB | ${item.instructions == null ? 'n/a' : Math.round(item.instructions).toLocaleString('en-US')} | ${item.files ?? 'n/a'} | ${item.diagnostics ?? 'n/a'} |`);
  }
  lines.push('', 'File counts and diagnostics were collected in separate, untimed runs.', '');
  return lines.join('\n');
}

export async function render(input, output, { commit, runUrl } = {}) {
  const renderer = await rendererIdentity([path.join(BENCH, 'render.mjs'), path.join(BENCH, 'results.mjs'), path.join(BENCH, 'lib.mjs'), path.join(BENCH, '..', 'pnpm-lock.yaml'), ...FONT_FILES]);
  const data = await readPublication(input, renderer);
  if (commit) data.measuredCommit = commit;
  if (runUrl) data.runUrl = runUrl;
  if (data.renderer) requireValue(data.renderer.sha256 === renderer.sha256, `Renderer source differs; check out ${data.renderer.revision}`);
  await mkdir(output, { recursive: true });
  const assets = {
    'benchmark.svg': barChart(data),
    'large-projects.svg': scatterChart(data, 'light'),
    'large-projects-dark.svg': scatterChart(data, 'dark'),
    'per-project-time.svg': projectChart(data, 'time', 'light'),
    'per-project-time-dark.svg': projectChart(data, 'time', 'dark'),
    'per-project-memory.svg': projectChart(data, 'memory', 'light'),
    'per-project-memory-dark.svg': projectChart(data, 'memory', 'dark'),
    'RESULTS.md': report(data),
  };
  await writeJson(path.join(output, 'publication.json'), data);
  const hashes = {};
  for (const [name, content] of Object.entries(assets)) { await writeFile(path.join(output, name), content); hashes[name] = await fileSha(path.join(output, name)); }
  const means = data.rows.map(r => ({ ...r, peakRssGiB: r.peakRssBytes / GIB }));
  await writeJson(path.join(output, 'means.json'), means);
  await writeFile(path.join(output, 'means.csv'), ['project,compiler,wall_seconds,instructions,peak_rss_bytes,files,diagnostics', ...means.map(r => [r.project, r.compiler, r.wallSeconds, r.instructions, r.peakRssBytes, r.files, r.diagnostics].join(','))].join('\n') + '\n');
  await writeJson(path.join(output, 'render-manifest.json'), { publicationSha256: await fileSha(path.join(output, 'publication.json')), renderer, assets: hashes });
  return { output, assets: hashes };
}
