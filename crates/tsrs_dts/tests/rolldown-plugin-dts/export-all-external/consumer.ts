import { rollup, type RollupOptions } from '__ENTRY__';
const options: RollupOptions = { input: 'index.js' };
rollup(options).then(build => build.close());
// @ts-expect-error invalid input type
const wrong: RollupOptions = { input: 42 };
