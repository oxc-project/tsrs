import assert from 'node:assert/strict';
import { checkSync } from '../src-js/index.mjs';
const options = { cwd: process.argv[2] };
for (let i = 0; i < 10; i++) checkSync(options);
const before = process.memoryUsage().rss;
for (let i = 0; i < 60; i++) assert.equal(checkSync(options).diagnostics[0].code, 2322);
const growth = process.memoryUsage().rss - before;
assert.ok(growth < 48 * 1024 * 1024, `RSS grew ${Math.round(growth / 1024 / 1024)} MiB after warmup`);
