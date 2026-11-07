#!/usr/bin/env node
// Renders bench/lint-typecheck/chart.svg and RESULTS.md from results.json:
//   node bench/lint-typecheck/render.mjs [results.json] [output dir]
// Medians are taken within each project and setup; projects are never combined.
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const input = process.argv[2] ?? path.join(HERE, 'results.json');
const outDir = process.argv[3] ?? HERE;
const data = JSON.parse(readFileSync(input, 'utf8'));

// Prose and labels live here; results.json holds measurements, versions and the machine description.
const CHART_RULE_SET = 'outline';
const SETUPS = {
  'tsrs one pass': {
    legend: 'Oxlint + tsrs, one run',
    short: 'tsrs, one run',
    command: '`oxlint --type-aware --type-check` with `OXLINT_TSGOLINT_PATH=tsrs-lint`',
  },
  'tsrs separate': {
    legend: 'Oxlint + tsrs, separate check',
    short: 'tsrs, separate check',
    command: '`oxlint --type-aware` with tsrs-lint, and `tsrs --noEmit`',
  },
  'tsgolint one pass': {
    legend: 'Oxlint + tsgolint, one run',
    short: 'tsgolint, one run',
    command: '`oxlint --type-aware --type-check` with oxlint-tsgolint, which type-checks only the linted files',
  },
  'tsgolint separate': {
    legend: 'Oxlint + tsgolint, tsgo',
    short: 'tsgolint + tsgo',
    command: '`oxlint --type-aware` with oxlint-tsgolint, and `tsc --noEmit` from TypeScript 7.0.2 (tsgo)',
  },
  'eslint separate': {
    legend: 'ESLint + typescript-eslint, tsc',
    short: 'ESLint + tsc',
    command: '`eslint` with typescript-eslint, and `tsc --noEmit` from TypeScript 6.0.3',
  },
  'tsrs lint only': {
    legend: 'Oxlint + tsrs, lint only',
    short: 'tsrs lint only',
    command: '`oxlint --type-aware` with tsrs-lint',
  },
  'tsrs check only': { legend: 'tsrs, check only', short: 'tsrs check only', command: '`tsrs --noEmit`' },
};
const LABELS = { vscode: 'VS Code', outline: 'Outline', vue: 'Vue core' };
const RULE_SET_TEXT = {
  outline: {
    heading: "Outline's type-aware rules (7)",
    chartLabel: "Outline's 7 type-aware rules",
    notes: [
      'The type-aware rules Outline enables in its own `.oxlintrc.json`. The chart uses this set. ESLint ran 3 times per project.',
    ],
  },
  recommended: {
    heading: 'typescript-eslint recommended-type-checked (23)',
    chartLabel: 'recommended-type-checked',
    notes: [
      "The 23 rules of typescript-eslint's `recommended-type-checked` preset that need type information. ESLint ran once per project.",
    ],
  },
  all: {
    heading: "tsgolint's benchmark rules (59)",
    chartLabel: "tsgolint's benchmark rules",
    notes: [
      "The rule list of tsgolint's own benchmark (`benchmarks/eslint.config.mjs`). These projects don't follow most of these rules, so lint reports many more findings and takes most of the time. ESLint ran once per project.",
    ],
  },
};
const INTRO = [
  "Type-aware lint and a type check usually run as separate commands, and each one parses, binds and checks the same TypeScript program. With tsrs as Oxlint's type-aware backend (0.23.0 and later), `oxlint --type-aware --type-check` lints and reports the type errors of each tsconfig program in one run, so each program is built and checked once. This compares that run with lint and `tsrs --noEmit` started together, with tsgolint plus tsgo, and with ESLint (typescript-eslint) plus tsc. In every rule set the one run's CPU time stayed near the larger of lint alone and the type check alone (0-30% above it), while the separate check pays for both.",
];
const METHOD = [
  '## Method',
  '',
  "- **Projects.** Pinned release tags, dependencies installed without install scripts. That skips VS Code's postinstall, which downloads `electron.d.ts`, so setup fetches it with `node build/npm/electronTypes.ts`. Vue's tsconfig sets `composite`, which rejects `--incremental false`, so `composite` was turned off, along with `isolatedDeclarations`, which needs it. Outline's own `.oxlintrc.json` files were removed so only the rule set under test runs.",
  "- **Files.** Every setup lints the same files: the project's source files in its tsconfig program, without declaration files. For Outline that is limited to the folders its `lint` script passes (`app`, `server`, `shared`, `plugins`). The type check covers the whole tsconfig program, except in the tsgolint one-run setup.",
  "- **Commands.** Type checks are `-p <tsconfig> --noEmit --incremental false --pretty false`. The tsrs setups set `TSRS_FAST=1`. Oxlint runs with `--format unix` and a config that turns off its default rules and turns on the rule set's type-aware rules. ESLint uses the same rules through typescript-eslint with `parserOptions.project`, and a 32 GiB Node heap.",
  "- **Measurement.** Each setup's processes, including the type-aware helper Oxlint starts, run in a fresh cgroup. CPU time is the cgroup's `cpu.stat` usage and peak memory its `memory.peak`, so both cover every process of the setup. Wall time ends when the last process exits. One untimed run of each setup (except ESLint) warms the file cache, then 5 timed rounds run every setup, with the order rotated each round and reversed every other round. Values are medians.",
  "- **Findings.** The tsrs one run reports the lint findings of its separate-check setup plus the type errors of the type check, in every project and rule set but one: with the recommended-type-checked rules on Vue it reports one more `no-unsafe-argument` finding in `packages/compiler-sfc/src/style/pluginScoped.ts`, the file where TypeScript 7.1 reports circular instantiations (TS5115), likely because the full check reaches that code in a different order. tsgolint's one run reports up to 40 more lint findings than its lint-only run on VS Code. ESLint's lint findings are within 1% of Oxlint's, apart from Outline with its own rules (1 vs 0). tsrs reports the same type errors as `tsc` from TypeScript 7.1 beta (11 in VS Code, 1 in Outline, 5 in Vue). TypeScript 7.0.2 reports 0, 0 and 1, and TypeScript 6.0.3 reports 7, 0 and 0.",
  '- **Scripts.** `setup.sh` prepares a machine, `measure.mjs` runs a rule set and `render.mjs` writes this page and the chart from `results.json`.',
];

// Setups drawn in the chart, in legend order. The others appear in the tables only.
const CHART_SETUPS = ['tsrs one pass', 'tsrs separate', 'tsgolint separate', 'eslint separate'];
const COLORS = {
  background: '#f4f3ec',
  border: '#dcdad0',
  text: '#16171d',
  muted: '#6f6876',
  rule: '#e2e0d6',
  axis: '#bdbab0',
  'tsrs one pass': '#f38020',
  'tsrs separate': '#faae40',
  'tsgolint separate': '#3178c6',
  'eslint separate': '#4b32c3',
};
const FONT = "Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif";
const WIDTH = 1200;
const BAR_HEIGHT = 12,
  BAR_GAP = 5,
  GROUP_GAP = 34;

const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
};
const cell = (ruleSet, project, setup) => {
  const samples = data.samples[ruleSet]?.[project]?.[setup];
  if (!samples?.length) return null;
  return {
    n: samples.length,
    wall: median(samples.map((s) => s.wallMs)) / 1000,
    cpu: median(samples.map((s) => s.cpuMs)) / 1000,
    memory: median(samples.map((s) => s.peakMiB)) / 1024,
  };
};

const f1 = (value) => value.toFixed(1);
const escText = (value) => String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
const escAttr = (value) => escText(value).replaceAll('"', '&quot;');
function element(parent, tag, attrs = {}, content) {
  const node = { tag, attrs, content, children: [] };
  parent?.children.push(node);
  return node;
}
function serialize(node, depth = 0) {
  const pad = '  '.repeat(depth);
  const attrs = Object.entries(node.attrs)
    .filter(([, v]) => v !== undefined)
    .map(([k, v]) => ` ${k}="${escAttr(v)}"`)
    .join('');
  if (node.content !== undefined) return `${pad}<${node.tag}${attrs}>${escText(node.content)}</${node.tag}>`;
  if (!node.children.length) return `${pad}<${node.tag}${attrs} />`;
  return `${pad}<${node.tag}${attrs}>\n${node.children.map((child) => serialize(child, depth + 1)).join('\n')}\n${pad}</${node.tag}>`;
}
function text(parent, x, y, value, size, weight = 400, fill, anchor) {
  return element(
    parent,
    'text',
    { x, y, 'font-size': size, 'font-weight': weight, fill, 'text-anchor': anchor },
    value,
  );
}
function textWidth(value, size) {
  return size * [...value].reduce((sum, char) => sum + ('ijlrtf-. '.includes(char) ? 0.3 : 0.56), 0);
}

const seconds = (value) => (value >= 100 ? `${Math.round(value)} s` : `${value.toFixed(value < 1 ? 2 : 1)} s`);
const gib = (value) => (value >= 1000 / 1024 ? `${value.toFixed(1)} GiB` : `${Math.round(value * 1024)} MiB`);
const times = (value, reference, more, less) => {
  const ratio = value / reference;
  if (Math.abs(ratio - 1) < 0.005) return 'same';
  return ratio >= 1 ? `${ratio.toFixed(ratio >= 10 ? 0 : 2)}x ${more}` : `${(1 / ratio).toFixed(2)}x ${less}`;
};

function chart(ruleSet) {
  const projects = data.projects.filter((p) => CHART_SETUPS.every((setup) => cell(ruleSet, p.name, setup)));
  const panels = [
    { key: 'cpu', title: 'CPU time, all processes', x: 236, right: 680, format: seconds },
    { key: 'memory', title: 'Peak memory, all processes', x: 736, right: WIDTH - 48, format: gib },
  ];
  const labels = (project, setup, panel) => {
    const value = cell(ruleSet, project.name, setup)[panel.key];
    const reference = cell(ruleSet, project.name, CHART_SETUPS[0])[panel.key];
    return [panel.format(value), setup === CHART_SETUPS[0] ? null : times(value, reference, 'more', 'less')];
  };
  const labelWidth = (project, setup, panel) => {
    const [amount, change] = labels(project, setup, panel);
    return textWidth(amount, 14) + (change ? 12 + textWidth(change, 14) * 1.08 : 0) + 12;
  };

  // Each project is scaled on its own: the setups compare within a project, and the projects differ by more than 10x in size.
  // ESLint is far off that scale, so its bar is cut short with a break mark; its label keeps the true value.
  const BREAK = 22;
  const OFF_SCALE = 'eslint separate';
  const layout = new Map();
  for (const project of projects) {
    for (const panel of panels) {
      const room = panel.right - panel.x;
      const value = (setup) => cell(ruleSet, project.name, setup)[panel.key];
      const scale = Math.min(
        ...CHART_SETUPS.filter((setup) => setup !== OFF_SCALE).map(
          (setup) => (room - labelWidth(project, setup, panel)) / value(setup),
        ),
      );
      for (const setup of CHART_SETUPS) {
        const full = value(setup) * scale;
        const limit = room - labelWidth(project, setup, panel);
        layout.set(
          `${project.name}|${panel.key}|${setup}`,
          full > limit ? { length: limit, broken: true } : { length: Math.max(2, full), broken: false },
        );
      }
    }
  }

  const legendItems = CHART_SETUPS.map((setup) => SETUPS[setup].legend);
  const legendRows = [];
  let row = [],
    x = 48;
  for (const [i, label] of legendItems.entries()) {
    const width = 22 + textWidth(label, 16) + 34;
    if (x + width > WIDTH - 48 && row.length) {
      legendRows.push(row);
      row = [];
      x = 48;
    }
    row.push({ setup: CHART_SETUPS[i], label, x });
    x += width;
  }
  legendRows.push(row);

  const legendTop = 126;
  const barTop = legendTop + legendRows.length * 28 + 84;
  const groupHeight = CHART_SETUPS.length * BAR_HEIGHT + (CHART_SETUPS.length - 1) * BAR_GAP;
  const pitch = groupHeight + GROUP_GAP;
  const bottom = barTop + (projects.length - 1) * pitch + groupHeight;
  const height = bottom + 64;

  const set = RULE_SET_TEXT[ruleSet];
  const title = 'Type-aware lint and type check: one run or two';
  const subtitle = `${set.chartLabel} and a full type check; ${data.machine.short}; median of ${cell(ruleSet, projects[0].name, CHART_SETUPS[0]).n} runs (ESLint ${cell(ruleSet, projects[0].name, 'eslint separate').n})`;
  const description = `${subtitle}. ${projects.map((project) => `${LABELS[project.name]}: ${panels.map((panel) => `${panel.title}: ${CHART_SETUPS.map((setup) => `${SETUPS[setup].legend} ${labels(project, setup, panel).filter(Boolean).join(', ')}`).join('; ')}`).join('. ')}.`).join(' ')} Shorter bars are better.`;
  const metadata = `Generated by bench/lint-typecheck/render.mjs; rules: ${ruleSet}; ${Object.entries(data.versions)
    .map(([k, v]) => `${k}@${v}`)
    .join(
      '; ',
    )}; measured: ${data.measured}. Details: https://github.com/oxc-project/tsrs/blob/main/bench/lint-typecheck/RESULTS.md`;

  const svg = element(null, 'svg', {
    xmlns: 'http://www.w3.org/2000/svg',
    width: WIDTH,
    height,
    viewBox: `0 0 ${WIDTH} ${height}`,
    role: 'img',
    'aria-labelledby': 'title description',
  });
  element(svg, 'title', { id: 'title' }, title);
  element(svg, 'desc', { id: 'description' }, description);
  element(svg, 'metadata', {}, metadata);
  element(svg, 'rect', {
    x: 0.5,
    y: 0.5,
    width: WIDTH - 1,
    height: height - 1,
    rx: 4,
    fill: COLORS.background,
    stroke: COLORS.border,
  });
  const drawing = element(svg, 'g', { 'font-family': FONT, fill: COLORS.text });
  text(drawing, 48, 66, title, 26, 600);
  text(drawing, 48, 98, subtitle, 17, 400, COLORS.muted);

  const legend = element(drawing, 'g', { 'data-role': 'legend' });
  for (const [r, items] of legendRows.entries()) {
    for (const item of items) {
      const y = legendTop + r * 28;
      element(legend, 'rect', { x: f1(item.x), y, width: 14, height: 14, rx: 3, fill: COLORS[item.setup] });
      text(legend, f1(item.x + 22), y + 12, item.label, 16, 500);
    }
  }

  const table = element(drawing, 'g', { 'data-role': 'bars' });
  for (const panel of panels) {
    text(table, panel.x, barTop - 34, panel.title, 17, 600);
    element(table, 'line', {
      x1: panel.x,
      y1: barTop - 6,
      x2: panel.x,
      y2: bottom + 6,
      stroke: COLORS.axis,
      'stroke-width': 1,
    });
  }
  for (const [index, project] of projects.entries()) {
    const top = barTop + index * pitch;
    element(table, 'line', {
      x1: 48,
      y1: top - 15,
      x2: WIDTH - 48,
      y2: top - 15,
      stroke: COLORS.rule,
      'stroke-width': 1,
    });
    const group = element(table, 'g', { 'data-project': project.name });
    text(group, 48, f1(top + groupHeight / 2 - 3), LABELS[project.name], 17, 500);
    text(
      group,
      48,
      f1(top + groupHeight / 2 + 17),
      `${project.lintFiles.toLocaleString('en-US')} files`,
      14,
      400,
      COLORS.muted,
    );
    for (const panel of panels) {
      for (const [i, setup] of CHART_SETUPS.entries()) {
        const y = top + i * (BAR_HEIGHT + BAR_GAP);
        const { length, broken } = layout.get(`${project.name}|${panel.key}|${setup}`);
        const [amount, change] = labels(project, setup, panel);
        element(group, 'rect', {
          'data-setup': setup,
          'data-metric': panel.key,
          x: panel.x,
          y,
          width: f1(length),
          height: BAR_HEIGHT,
          rx: 2,
          fill: COLORS[setup],
        });
        if (broken) {
          const at = panel.x + length - BREAK;
          element(group, 'path', {
            d: `M${f1(at)} ${y + BAR_HEIGHT + 2}L${f1(at + 6)} ${y - 2}M${f1(at + 7)} ${y + BAR_HEIGHT + 2}L${f1(at + 13)} ${y - 2}`,
            stroke: COLORS.background,
            'stroke-width': 3,
          });
        }
        text(group, f1(panel.x + length + 8), y + 11, amount, 14, 400, COLORS.muted);
        if (change) text(group, f1(panel.x + length + 20 + textWidth(amount, 14)), y + 11, change, 14, 600);
      }
    }
  }
  text(
    drawing,
    48,
    height - 26,
    `Separate checks start at the same time as the lint. Bars are scaled within each project. ESLint bars with a break mark are cut short; labels show the real values.`,
    14,
    400,
    COLORS.muted,
  );
  return serialize(svg) + '\n';
}

function report() {
  const { versions, machine } = data;
  const lines = [
    '# Type-aware lint and type check: one run or two',
    '',
    '<!-- Generated by `node bench/lint-typecheck/render.mjs`; edit render.mjs or results.json, not this file. -->',
    '',
    ...INTRO,
    '',
    '![CPU time and peak memory of lint plus type check by project](chart.svg)',
    '',
    '## Setups',
    '',
    '| setup | commands |',
    '| --- | --- |',
    ...Object.values(SETUPS).map((s) => `| ${s.legend} | ${s.command} |`),
    '',
    `Versions: ${Object.entries(versions)
      .map(([k, v]) => `${k} ${v}`)
      .join(', ')}. Machine: ${machine.long}.`,
    '',
  ];
  for (const [ruleSet, set] of Object.entries(RULE_SET_TEXT)) {
    if (!data.samples[ruleSet]) continue;
    lines.push(`## ${set.heading}`, '', ...set.notes, '');
    for (const [title, key, format] of [
      ['CPU time', 'cpu', seconds],
      ['Peak memory, all processes', 'memory', gib],
      ['Wall time', 'wall', seconds],
    ]) {
      const setups = Object.keys(SETUPS).filter((setup) => data.projects.some((p) => cell(ruleSet, p.name, setup)));
      lines.push(
        `**${title}**`,
        '',
        `| project | ${setups.map((s) => SETUPS[s].short).join(' | ')} |`,
        `| --- | ${setups.map(() => '---:').join(' | ')} |`,
      );
      for (const project of data.projects) {
        const reference = cell(ruleSet, project.name, 'tsrs one pass');
        if (!reference) continue;
        lines.push(
          `| ${LABELS[project.name]} | ${setups
            .map((setup) => {
              const value = cell(ruleSet, project.name, setup);
              if (!value) return '-';
              const [more, less] = key === 'wall' ? ['slower', 'faster'] : ['more', 'less'];
              return setup === 'tsrs one pass'
                ? format(value[key])
                : `${format(value[key])} (${times(value[key], reference[key], more, less)})`;
            })
            .join(' | ')} |`,
        );
      }
      lines.push('');
    }
  }
  lines.push(...METHOD, '');
  return lines.join('\n');
}

const headline = CHART_RULE_SET;
writeFileSync(path.join(outDir, 'chart.svg'), chart(headline));
writeFileSync(path.join(outDir, 'RESULTS.md'), report());
console.log(`wrote ${path.join(outDir, 'chart.svg')} and RESULTS.md`);
