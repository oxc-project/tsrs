#!/usr/bin/env node
// Summarizes a tsrs-lint request traced with TSRS_LINT_TRACE=<file> (notes/perf-lint-check-one-run.md): each phase
// span, then for the per-file check and lint spans the busy time per checker thread and how much of the pass's cores
// it kept busy.
//   node bench/lint-typecheck/trace.mjs <trace file> [cores]
import { readFileSync } from 'node:fs';

const [file, coresArg] = process.argv.slice(2);
if (!file) {
  console.error('usage: node bench/lint-typecheck/trace.mjs <trace file> [cores]');
  process.exit(2);
}
const cores = Number(coresArg ?? 8);
const spans = readFileSync(file, 'utf8')
  .trim()
  .split('\n')
  .map((line) => {
    const [from, to, thread, name, detail] = line.split('\t');
    return { from: Number(from), to: Number(to), thread, name, detail: detail ?? '' };
  });
const fmt = (s) => s.toFixed(3);
const perFile = new Set(['check', 'lint']);
for (const span of spans.filter((s) => !perFile.has(s.name)).sort((a, b) => a.from - b.from))
  console.log(`${span.name.padEnd(16)} ${fmt(span.from)} -> ${fmt(span.to)}  ${fmt(span.to - span.from)} s  ${span.thread}`);
for (const kind of perFile) {
  const files = spans.filter((s) => s.name === kind);
  if (!files.length) continue;
  const from = Math.min(...files.map((s) => s.from));
  const to = Math.max(...files.map((s) => s.to));
  const threads = new Map();
  for (const s of files) {
    const t = threads.get(s.thread) ?? { busy: 0, last: 0 };
    t.busy += s.to - s.from;
    t.last = Math.max(t.last, s.to);
    threads.set(s.thread, t);
  }
  const busy = [...threads.values()].reduce((sum, t) => sum + t.busy, 0);
  const max = Math.max(...[...threads.values()].map((t) => t.busy));
  const firstIdle = Math.min(...[...threads.values()].map((t) => t.last));
  const used = (100 * busy) / ((to - from) * Math.min(cores, threads.size));
  console.log(
    `${kind}: ${files.length} files on ${threads.size} threads, ${fmt(from)} -> ${fmt(to)}, busy ${fmt(busy)} s ` +
      `(max ${fmt(max)}, mean ${fmt(busy / threads.size)}), ${used.toFixed(1)}% of the cores, first thread done at ${fmt(firstIdle)}`,
  );
}
