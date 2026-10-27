import type { Item, Nested, Module } from '__ENTRY__';
const item: Item = { source: 'external' }; const nested: Nested = item;
declare const module: Module; const client = new module.Client();
// @ts-expect-error explicit export overrides star identity
const bad: Item = { source: 'other' };
