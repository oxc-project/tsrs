import { sLit, nLit, aLit, sLitDef, nLitDef, sDef, nDef, aDef, tuple, unique } from '__ENTRY__';
const s: '' = sLit;
const n: 0 = nLit;
const sd: '' = sLitDef;
const nd: 0 = nLitDef;
const broadS: string = sDef;
const broadN: number = nDef;
const values: number[] = aDef;
const pair: [number, string] = tuple;
const symbol: typeof unique = unique;
// @ts-expect-error never array cannot receive numbers
aLit.push(1);
// @ts-expect-error tuple element types stay ordered
const bad: [string, number] = tuple;
// @ts-expect-error unique symbol remains nominal
const badSymbol: typeof unique = Symbol();
