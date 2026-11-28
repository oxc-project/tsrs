import type { Two, Weak } from './model';
declare const source: { a: number; live: boolean };
const two: Two<number> = source;
const optional: string | undefined = two.aliasToken;
declare const other: { zz: number };
const weak: Weak<number> = other;
