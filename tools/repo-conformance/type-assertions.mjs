#!/usr/bin/env node
// Turn tsd's checker-API assertions into compiler-enforced assertions in the disposable checkout.
// The generated source is checked by both pinned tsgo and tsrs, so a changed answer appears in their diagnostics.
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';

const [root, suite] = process.argv.slice(2);
if (!root || !['sindresorhus/type-fest', 'millsp/ts-toolbelt'].includes(suite)) {
  throw new Error('Usage: type-assertions.mjs <checkout> <type suite>');
}
const requireFromCheckout = createRequire(path.join(root, 'package.json'));
// Use the parser pinned in the sandbox image. A project's own `typescript` can be the native TS7
// package, which has no JavaScript parser API, or an older version with different syntax support.
const ts = requireFromCheckout(process.env.TSRS_CONFORMANCE_TYPESCRIPT ?? '/usr/local/lib/node_modules/typescript');

function filesUnder(directory) {
  if (!fs.existsSync(directory)) return [];
  return fs.readdirSync(directory, {withFileTypes: true}).flatMap(entry => {
    const file = path.join(directory, entry.name);
    return entry.isDirectory() ? filesUnder(file) : entry.isFile() && /\.tsx?$/.test(file) ? [file] : [];
  });
}

function requireIncluded(tests) {
  const configFile = path.join(root, 'tsconfig.json');
  const config = ts.readConfigFile(configFile, ts.sys.readFile);
  if (config.error) throw new Error(`Could not read ${configFile}`);
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, root);
  const included = new Set(parsed.fileNames.map(file => path.resolve(file)));
  const missing = tests.filter(file => !included.has(path.resolve(file)));
  if (missing.length) throw new Error(`${missing.length} type test files are not in the root tsconfig: ${missing[0]}`);
}

const helpers = `
export {};
type __TsrsTsdEqual<A, B> = (<T>() => T extends A ? 1 : 2) extends
  (<T>() => T extends B ? 1 : 2) ? true : false;
type __TsrsTsdAssert<T extends true> = T;
type __TsrsTsdAssignable<A, B> = [A] extends [B] ? true : false;
`;
const supported = new Set([
  'expectType', 'expectNotType', 'expectAssignable', 'expectNotAssignable', 'expectError', 'expectNever',
]);

function oneLine(expression) {
  const scanner = ts.createScanner(ts.ScriptTarget.Latest, true, ts.LanguageVariant.Standard, expression);
  const parts = [];
  for (let token = scanner.scan(); token !== ts.SyntaxKind.EndOfFileToken; token = scanner.scan()) {
    parts.push(scanner.getTokenText());
  }
  return parts.join(' ');
}

function transform(file) {
  const input = fs.readFileSync(file, 'utf8');
  if (!/from\s*['"]tsd['"]/.test(input)) return 0;
  const source = ts.createSourceFile(file, input, ts.ScriptTarget.Latest, true,
    file.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  if (source.parseDiagnostics.length) throw new Error(`${file}: TypeScript could not parse tsd assertions`);
  const imports = new Map();
  const edits = [];
  for (const statement of source.statements) {
    if (!ts.isImportDeclaration(statement) || statement.moduleSpecifier.text !== 'tsd') continue;
    const bindings = statement.importClause?.namedBindings;
    if (!bindings || !ts.isNamedImports(bindings) || statement.importClause.isTypeOnly) {
      throw new Error(`${file}: unsupported tsd import`);
    }
    for (const element of bindings.elements) {
      const name = element.propertyName?.text ?? element.name.text;
      if (!supported.has(name)) throw new Error(`${file}: unsupported tsd assertion ${name}`);
      imports.set(element.name.text, name);
    }
    edits.push({start: statement.getStart(source), end: statement.end, text: ''});
  }
  if (!imports.size) throw new Error(`${file}: no supported tsd assertions imported`);
  let count = 0;
  function visit(node) {
    if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && imports.has(node.expression.text)) {
      const assertion = imports.get(node.expression.text);
      if (!ts.isExpressionStatement(node.parent) || node.arguments.length !== 1) {
        throw new Error(`${file}: ${assertion} must be a statement with one argument`);
      }
      const expected = node.typeArguments?.[0]?.getText(source);
      if ((assertion === 'expectError' || assertion === 'expectNever') ?
        node.typeArguments?.length : node.typeArguments?.length !== 1) {
        throw new Error(`${file}: unexpected type arguments for ${assertion}`);
      }
      const expression = node.arguments[0].getText(source);
      const id = `__tsrsTsd${count++}`;
      let replacement;
      switch (assertion) {
        case 'expectType':
          replacement = `const ${id} = (${expression}) satisfies ${expected};\n` +
            `type ${id}Check = __TsrsTsdAssert<__TsrsTsdEqual<typeof ${id}, ${expected}>>;`;
          break;
        case 'expectNotType':
          replacement = `const ${id} = ${expression};\n` +
            `type ${id}Check = __TsrsTsdAssert<__TsrsTsdEqual<typeof ${id}, ${expected}> extends false ? true : false>;`;
          break;
        case 'expectAssignable':
          replacement = `const ${id}: ${expected} = ${expression};`;
          break;
        case 'expectNotAssignable':
          replacement = `const ${id} = ${expression};\n` +
            `type ${id}Check = __TsrsTsdAssert<__TsrsTsdAssignable<typeof ${id}, ${expected}> extends false ? true : false>;`;
          break;
        case 'expectNever':
          replacement = `const ${id}: never = ${expression};`;
          break;
        case 'expectError':
          replacement = `// @ts-expect-error\n${oneLine(expression)};`;
          break;
      }
      edits.push({start: node.parent.getStart(source), end: node.parent.end, text: replacement});
      return;
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
  let boundary = input.length;
  for (const edit of edits.sort((a, b) => b.start - a.start)) {
    // Any overlapping edits indicate a construct that the adapter does not know how to evaluate.
    if (edit.start < 0 || edit.end > boundary) throw new Error(`${file}: invalid assertion span`);
    boundary = edit.start;
  }
  let output = input;
  for (const edit of edits) output = output.slice(0, edit.start) + edit.text + output.slice(edit.end);
  fs.writeFileSync(file, output + '\n' + helpers);
  return count;
}

if (suite === 'millsp/ts-toolbelt') {
  const configFile = path.join(root, 'tsconfig.json');
  const config = ts.parseConfigFileTextToJson(configFile, fs.readFileSync(configFile, 'utf8'));
  if (config.error) throw new Error(`Could not parse ${configFile}`);
  // This pre-TypeScript-7 option prevents the otherwise still useful tests from being checked.
  delete config.config.compilerOptions?.suppressImplicitAnyIndexErrors;
  fs.writeFileSync(configFile, JSON.stringify(config.config, null, 2) + '\n');
  const tests = filesUnder(path.join(root, 'tests'));
  if (!tests.some(file => /\bchecks\s*\(\s*\[/.test(fs.readFileSync(file, 'utf8')))) {
    throw new Error('ts-toolbelt has no Test.check/Test.checks assertions');
  }
  requireIncluded(tests);
  console.log(`ts-toolbelt: ${tests.length} type test files with Test.check/Test.checks assertions`);
} else {
  const tests = filesUnder(path.join(root, 'test-d'));
  if (!tests.length) throw new Error('type-fest has no test-d type suite');
  requireIncluded(tests);
  const assertions = tests.reduce((count, file) => count + transform(file), 0);
  if (!assertions) throw new Error('type-fest has no tsd assertions to evaluate');
  console.log(`type-fest: ${tests.length} type test files, ${assertions} tsd assertions compiled`);
}
