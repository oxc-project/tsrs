import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import test from 'node:test';

const adapter = fileURLToPath(new URL('./type-assertions.mjs', import.meta.url));
const parser = process.env.TSRS_CONFORMANCE_TYPESCRIPT;
const tsgo = process.env.TSRS_CONFORMANCE_TSGO;
const tsrs = process.env.TSRS_CONFORMANCE_TSRS;
const enabled = Boolean(parser && tsgo && tsrs);

function checkout(source, suite) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'tsrs-type-assertions-'));
  fs.writeFileSync(path.join(root, 'package.json'), '{"type":"module"}\n');
  fs.writeFileSync(path.join(root, 'tsconfig.json'), JSON.stringify({
    compilerOptions: {strict: true, noEmit: true, types: [], suppressImplicitAnyIndexErrors: true},
    include: suite === 'millsp/ts-toolbelt' ? ['sources/**/*.ts', 'tests/**/*.ts'] : ['test-d/**/*.ts'],
  }));
  if (suite === 'millsp/ts-toolbelt') {
    fs.mkdirSync(path.join(root, 'tests'));
    fs.mkdirSync(path.join(root, 'sources'));
    fs.writeFileSync(path.join(root, 'sources', 'Test.ts'), `
type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends
  (<T>() => T extends B ? 1 : 2) ? 1 : 0;
export declare function check<A, B, Outcome extends 0 | 1>(): Equal<Equal<A, B>, Outcome>;
export declare function checks(values: 1[]): void;
`);
    fs.writeFileSync(path.join(root, 'tests', 'Any.ts'), source);
  } else {
    fs.mkdirSync(path.join(root, 'test-d'));
    fs.writeFileSync(path.join(root, 'test-d', 'assert.ts'), source);
    // The type-fest test only needs a simple config; obsolete options are tested by ts-toolbelt below.
    const config = JSON.parse(fs.readFileSync(path.join(root, 'tsconfig.json'), 'utf8'));
    delete config.compilerOptions.suppressImplicitAnyIndexErrors;
    fs.writeFileSync(path.join(root, 'tsconfig.json'), JSON.stringify(config));
  }
  return root;
}

function run(root, command, args) {
  return spawnSync(command, args, {cwd: root, encoding: 'utf8', env: {
    ...process.env, TSRS_CONFORMANCE_TYPESCRIPT: parser,
  }});
}

function checked(source, suite = 'sindresorhus/type-fest') {
  const root = checkout(source, suite);
  const transformed = run(root, process.execPath, [adapter, root, suite]);
  assert.equal(transformed.status, 0, transformed.stderr);
  return {root, transformed: transformed.stdout,
    go: run(root, tsgo, ['-p', 'tsconfig.json', '--pretty', 'false']),
    rust: run(root, tsrs, ['-p', 'tsconfig.json', '--pretty', 'false'])};
}

test('tsd assertions pass on both compilers', {skip: !enabled}, () => {
  const result = checked(`
import {expectType, expectNotType, expectAssignable, expectNotAssignable, expectError, expectNever} from 'tsd';
declare const value: string;
declare const bottom: never;
expectType<string>(value);
expectNotType<number>(value);
expectAssignable<string | number>(value);
expectNotAssignable<number>(value);
expectError(value.toFixed());
expectNever(bottom);
`);
  try {
    assert.match(result.transformed, /6 tsd assertions compiled/);
    assert.equal(result.go.status, 0, result.go.stdout);
    assert.equal(result.rust.status, 0, result.rust.stdout);
  } finally {
    fs.rmSync(result.root, {recursive: true, force: true});
  }
});

for (const [name, expression, code] of [
  ['exact type', 'expectType<number>(value);', 'TS1360'],
  ['different type', 'expectNotType<string>(value);', 'TS2344'],
  ['assignability', 'expectAssignable<number>(value);', 'TS2322'],
  ['negative assignability', 'expectNotAssignable<string>(value);', 'TS2344'],
  ['expected error', 'expectError(value.toString());', 'TS2578'],
]) {
  test(`tsd ${name} rejects a wrong assertion`, {skip: !enabled}, () => {
    const result = checked(`
import {expectType, expectNotType, expectAssignable, expectNotAssignable, expectError} from 'tsd';
declare const value: string;
${expression}
`);
    try {
      for (const compiler of [result.go, result.rust]) {
        assert.equal(compiler.status, 2, compiler.stdout);
        assert.match(compiler.stdout, new RegExp(code));
      }
    } finally {
      fs.rmSync(result.root, {recursive: true, force: true});
    }
  });
}

test('ts-toolbelt checks are included and an obsolete option is removed', {skip: !enabled}, () => {
  const good = checked("import {check, checks} from '../sources/Test';\nchecks([check<1, 1, 1>()]);\n",
    'millsp/ts-toolbelt');
  const bad = checked("import {check, checks} from '../sources/Test';\nchecks([check<1, 2, 1>()]);\n",
    'millsp/ts-toolbelt');
  try {
    assert.match(good.transformed, /Test.check\/Test.checks assertions/);
    assert.doesNotMatch(fs.readFileSync(path.join(good.root, 'tsconfig.json'), 'utf8'),
      /suppressImplicitAnyIndexErrors/);
    assert.equal(good.go.status, 0, good.go.stdout);
    assert.equal(good.rust.status, 0, good.rust.stdout);
    assert.equal(bad.go.status, 2, bad.go.stdout);
    assert.equal(bad.rust.status, 2, bad.rust.stdout);
  } finally {
    fs.rmSync(good.root, {recursive: true, force: true});
    fs.rmSync(bad.root, {recursive: true, force: true});
  }
});

test('unsupported tsd assertions stop setup', {skip: !enabled}, () => {
  const root = checkout("import {expectDeprecated} from 'tsd';\nexpectDeprecated(Date.now);\n",
    'sindresorhus/type-fest');
  try {
    const transformed = run(root, process.execPath, [adapter, root, 'sindresorhus/type-fest']);
    assert.notEqual(transformed.status, 0);
    assert.match(transformed.stderr, /unsupported tsd assertion expectDeprecated/);
  } finally {
    fs.rmSync(root, {recursive: true, force: true});
  }
});
