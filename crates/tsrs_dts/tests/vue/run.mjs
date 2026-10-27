import assert from "node:assert/strict";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { ROOT, pinnedCommit, run } from "../../../../tools/lib/cli.mjs";

const directory = path.dirname(fileURLToPath(import.meta.url));
const root = ROOT;
const [hostArgument, compilerArgument, referenceArgument] = process.argv.slice(2);
assert(hostArgument && compilerArgument && referenceArgument,
  "usage: node crates/tsrs_dts/tests/vue/run.mjs <vue-test-host> <tsrs> <pinned-tsgo>");
const host = path.resolve(hostArgument);
const compiler = path.resolve(compilerArgument);
const reference = path.resolve(referenceArgument);

function execute(command, args, timeout = 120_000) {
  const result = run([command, ...args], { cwd: root, check: false, timeout: timeout / 1000 });
  assert.equal(result.signal, null, `${command} terminated: ${result.signal}`);
  return { status: result.status, text: result.stdout + result.stderr, stdout: result.stdout };
}

const checkout = execute("git", ["-C", "ts-ref", "rev-parse", "HEAD"]);
assert.equal(checkout.status, 0, checkout.text);
assert.equal(checkout.stdout.trim(), pinnedCommit(), "ts-ref must match the workspace TypeScript pin");
const clean = execute("git", ["-C", "ts-ref", "diff", "--exit-code", "HEAD", "--", "tsc"]);
assert.equal(clean.status, 0, clean.text);

const cases = JSON.parse(readFileSync(path.join(directory, "fixtures/cases.json"), "utf8"));
assert(cases.length > 0, "Vue fixture selection is empty");
assert.equal(new Set(cases.map(test => test.name)).size, cases.length, "duplicate Vue case");
const scratch = path.join(directory, ".work");
mkdirSync(scratch, { recursive: true });
const work = mkdtempSync(path.join(scratch, "run-"));

function codes(text) {
  return [...new Set([...text.matchAll(/error TS(\d+):/g)].map(match => Number(match[1])))].sort((a, b) => a - b);
}

function check(command, config, expected, mapper) {
  const result = execute(command, [
    "-p", config, "--noEmit", "--pretty", "false", "--singleThreaded",
    ...(mapper ? ["--runExternalCode"] : []),
  ]);
  assert.equal(result.status, expected.length ? 2 : 0, `${command}: ${config}\n${result.text}`);
  assert.deepEqual(codes(result.text), expected, `${command}: ${config}\n${result.text}`);
}

function bundle(config, entry) {
  const result = execute(host, [config, entry], 300_000);
  assert.equal(result.status, 0, result.text);
  return JSON.parse(result.stdout);
}

function checkBundle(project, result, name, consumer) {
  const output = path.join(project, name);
  mkdirSync(output);
  assert(Object.keys(result.files).length > 0, "Vue build returned no artifacts");
  for (const [file, text] of Object.entries(result.files)) {
    assert.equal(path.basename(file), file, "bundle artifact must be a basename");
    writeFileSync(path.join(output, file), text);
  }
  cpSync(consumer, path.join(output, "consumer.ts"));
  const config = path.join(output, "tsconfig.json");
  writeFileSync(config, JSON.stringify({
    compilerOptions: {
      strict: true, target: "ES2022", module: "ESNext", moduleResolution: "Bundler",
      noEmit: true, types: [], skipLibCheck: false, skipDefaultLibCheck: false,
    },
    files: [...Object.keys(result.files).filter(file => /\.d\.[cm]?ts$/.test(file)), "consumer.ts"],
  }));
  for (const command of [reference, compiler]) check(command, config, [], false);
}

try {
  for (const test of cases) {
    const project = path.join(work, test.name);
    cpSync(path.join(directory, "fixtures/base"), project, { recursive: true });
    if (test.name !== "valid") {
      cpSync(path.join(directory, "fixtures", test.name), project, { recursive: true });
    }
    const config = path.join(project, "tsconfig.json");
    const expected = [...test.codes].sort((a, b) => a - b);
    for (const command of [reference, compiler]) check(command, config, expected, true);
    const result = bundle(config, path.join(project, "src/index.ts"));
    assert.deepEqual([...new Set(result.codes)].sort((a, b) => a - b), expected,
      `${test.name}: ${result.diagnostics.join("\n")}`);
    if (expected.length) {
      assert(result.emitError, `${test.name}: invalid source emitted declarations`);
    } else {
      checkBundle(project, result, "bundle", path.join(directory, "fixtures/base/consumer.ts"));
      checkBundle(project, bundle(config, path.join(project, "src/Child.vue")),
        "child-bundle", path.join(directory, "fixtures/component-consumer.ts"));
    }
    console.log(`pass Vue ${test.name}`);
  }
} finally {
  rmSync(work, { recursive: true, force: true });
}
