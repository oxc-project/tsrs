import type { Klass } from '__ENTRY__';
declare const value: Klass;
const b: {} = value.B;
const c: {} = value[0];
const optional: {}[] | undefined = value.a;
type SymbolKeys = Extract<keyof Klass, symbol>;
type Assert<T extends true> = T;
type HasSymbol = Assert<[SymbolKeys] extends [never] ? false : true>;
// @ts-expect-error unknown property is not an index signature
value.missing;
// @ts-expect-error optional array is not a required number
const wrong: number = value.a;
