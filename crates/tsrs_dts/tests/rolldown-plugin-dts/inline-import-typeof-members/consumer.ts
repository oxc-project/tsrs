import type { TypeScript, Test } from '__ENTRY__';
declare const compiler: TypeScript;
const program = compiler.createProgram([], {});
const count: number = program.getSourceFiles().length;
const test: Test = { rollup: { input: 'index.js' } };
// @ts-expect-error rollup input cannot be numeric
const wrong: Test = { rollup: { input: 1 } };
