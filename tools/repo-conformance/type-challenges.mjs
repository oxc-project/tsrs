#!/usr/bin/env node
// Fill selected upstream templates in a disposable type-challenges checkout and compile its own test cases.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root = process.argv[2];
if (!root) throw new Error('Usage: type-challenges.mjs <checkout>');
const solutions = fileURLToPath(new URL('./fixtures/type-challenges/solutions/', import.meta.url));
if (!fs.existsSync(path.join(root, 'utils', 'index.d.ts'))) {
  throw new Error('Upstream @type-challenges/utils declarations are missing');
}

const names = fs.readdirSync(solutions).filter(name => name.endsWith('.ts')).sort();
if (!names.length) throw new Error('No known solutions');
const levels = new Set();
const files = [];
let assertions = 0;
for (const name of names) {
  const match = /^\d{5}-(easy|medium|hard)-[a-z0-9-]+\.ts$/.exec(name);
  if (!match) throw new Error(`Invalid solution name: ${name}`);
  levels.add(match[1]);
  const directory = path.join('questions', name.slice(0, -3));
  const template = path.join(root, directory, 'template.ts');
  const cases = path.join(root, directory, 'test-cases.ts');
  if (!fs.existsSync(template) || !fs.existsSync(cases)) {
    throw new Error(`Upstream challenge is missing: ${directory}`);
  }
  const source = fs.readFileSync(cases, 'utf8');
  if (!source.includes("from '@type-challenges/utils'")) {
    throw new Error(`Upstream assertions are missing: ${cases}`);
  }
  const count = [...source.matchAll(/\bExpect\s*</g)].length;
  if (!count) throw new Error(`No Expect assertions: ${cases}`);
  assertions += count;
  fs.copyFileSync(path.join(solutions, name), template);
  files.push(path.join(directory, 'template.ts'), path.join(directory, 'test-cases.ts'));
}
if (levels.size !== 3) throw new Error('Known solutions must cover easy, medium and hard challenges');

fs.writeFileSync(path.join(root, 'tsconfig.json'), JSON.stringify({
  compilerOptions: {
    target: 'esnext',
    module: 'preserve',
    moduleResolution: 'bundler',
    strict: true,
    noEmit: true,
    skipLibCheck: true,
    paths: {'@type-challenges/utils': ['./utils/index.d.ts']},
  },
  files,
  include: [],
}, null, 2) + '\n');
console.log(`type-challenges: ${names.length} upstream challenge cases, ${assertions} Expect assertions`);
