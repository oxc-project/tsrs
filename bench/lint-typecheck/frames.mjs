#!/usr/bin/env node
// Compares two tsrs-lint outputs (protocol frames: 4-byte little-endian length, 1-byte type, JSON): findings per rule,
// the number of type errors, and whether the frame sequences and multisets are the same.
//   node bench/lint-typecheck/frames.mjs <a.bin> <b.bin>     SHOW=1 prints a few frames only one side has
import { readFileSync } from 'node:fs';

function frames(path) {
  const buf = readFileSync(path);
  const out = [];
  for (let at = 0; at < buf.length; ) {
    const len = buf.readUInt32LE(at);
    out.push(`${buf[at + 4]} ${buf.subarray(at + 5, at + 5 + len).toString('utf8')}`);
    at += 5 + len;
  }
  return out;
}

const paths = process.argv.slice(2);
if (paths.length !== 2) {
  console.error('usage: node bench/lint-typecheck/frames.mjs <a.bin> <b.bin>');
  process.exit(2);
}
const [a, b] = paths.map(frames);
const key = (frame) => {
  const json = JSON.parse(frame.slice(frame.indexOf(' ') + 1));
  return json.rule ?? json.message?.id ?? 'other';
};
const count = (list) => list.reduce((m, f) => m.set(key(f), (m.get(key(f)) ?? 0) + 1), new Map());
const [countA, countB] = [count(a), count(b)];
const typeErrors = [0, 0];
for (const k of [...new Set([...countA.keys(), ...countB.keys()])].sort()) {
  if (/^TS\d+$/.test(k)) {
    typeErrors[0] += countA.get(k) ?? 0;
    typeErrors[1] += countB.get(k) ?? 0;
  } else console.log(`${k}: ${countA.get(k) ?? 0} ${countB.get(k) ?? 0}`);
}
console.log(`type errors: ${typeErrors[0]} ${typeErrors[1]}`);
console.log(`frames: ${a.length} ${b.length}, same sequence: ${a.length === b.length && a.every((f, i) => f === b[i])}`);
const multiset = (list) => list.reduce((m, f) => m.set(f, (m.get(f) ?? 0) + 1), new Map());
const [setA, setB] = [multiset(a), multiset(b)];
const onlyA = [...setA].filter(([f, n]) => (setB.get(f) ?? 0) < n);
const onlyB = [...setB].filter(([f, n]) => (setA.get(f) ?? 0) < n);
console.log(`same multiset: ${!onlyA.length && !onlyB.length} (only first ${onlyA.length}, only second ${onlyB.length})`);
if (process.env.SHOW) for (const [f] of [...onlyA.slice(0, 5), ...onlyB.slice(0, 5)]) console.log(f.slice(0, 400));
