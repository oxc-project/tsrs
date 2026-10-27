import type { TypeUse, ValueUse, BothUse, Derived } from '__ENTRY__';
const type: TypeUse = { value: 'ok' };
const value: ValueUse = { value: 1 };
const both: BothUse = { type: { value: 'ok' }, value: { value: 1 } };
// @ts-expect-error both spaces retain different property types
const bad: BothUse = { type: { value: 1 }, value: { value: 'wrong' } };
// @ts-expect-error class base brand survives a value-space heritage reference
const derived: Derived = { value: 1 };
