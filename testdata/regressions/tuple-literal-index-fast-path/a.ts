// Literal indexed accesses into tuples that reach the checker without an access node (through instantiation of
// `Nested` and `Get`), so a tuple fast path answers them. The errors pin the element types and the `length` literal,
// the out-of-range index (`undefined`, no error without an access node), and the optional and variadic tuples that
// stay on the property-lookup path.
type Nested<T extends readonly unknown[]> = {
  first: T[0];
  second: T[1];
  len: T["length"];
  third: T[2];
  str1: T["1"];
  padded: T["01"];
};

type Get<T extends readonly unknown[], K extends keyof T> = { v: T[keyof T & K] };

declare const fixed: Nested<[string, number]>;
const a: number = fixed.first;
const b: string = fixed.second;
const c: 3 = fixed.len;
const d: never = fixed.third;
const e: string = fixed.str1;
const f: never = fixed.padded;

declare const g0: Get<[string, number], "0">;
declare const g1: Get<[string, number], 1>;
declare const gl: Get<[string, number], "length">;
const g: number = g0.v;
const h: string = g1.v;
const i: 3 = gl.v;

declare const opt: Nested<[string, number?]>;
const j: number = opt.second;
const k: 3 = opt.len;

declare const rest: Nested<[string, ...number[]]>;
const l: string = rest.second;
const m: 3 = rest.len;

declare const ro: Nested<readonly [boolean, bigint]>;
const n: string = ro.second;
const o: 1 = ro.len;
