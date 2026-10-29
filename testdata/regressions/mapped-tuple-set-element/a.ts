// Homomorphic mapped types over tuples whose template is `P extends K ? V : T[P]` (the type-level "set element"
// idiom). The errors print the resulting tuples: a single literal key replaces one element, and the shapes a
// one-element fast path must leave to the per-element path (a union key, a numeric key, `"length"`, an optional
// element, an extends type that mentions P, a key the tuple does not have) still produce the per-element result.
// A tuple whose element is the error type (`Array` without its type argument) collapses to the error type on every
// path, so `l` reports only TS2314.
type SetAt<T, K extends keyof T, V> = { [P in keyof T]: P extends K ? V : T[P] };
type SetWide<T, K extends string, V> = { [P in keyof T]: P extends `${K}` ? V : T[P] };
type SetSelf<T, V> = { [P in keyof T]: P extends Exclude<P, "0"> ? V : T[P] };
type Pair = [string, number];

declare const s1: SetAt<Pair, "1", boolean>;
const a: [string, boolean] = s1;
const b: [boolean, number] = s1;

declare const s0: SetAt<readonly [string, number], "0", boolean>;
const c: never = s0;

declare const sLen: SetAt<Pair, "length", boolean>;
const d: never = sLen;

declare const sUnion: SetAt<Pair, "0" | "1", boolean>;
const e: never = sUnion;

declare const sNum: SetAt<Pair, 1, boolean>;
const f: never = sNum;

declare const sOpt: SetAt<[string, number?], "0", boolean>;
const g: never = sOpt;

declare const sWide: SetWide<Pair, "1", boolean>;
const h: never = sWide;

declare const sSelf: SetSelf<Pair, boolean>;
const i: never = sSelf;

declare const sOut: SetWide<Pair, "7", boolean>;
const j: never = sOut;

declare const sNested: SetAt<[[string], { a: number }], "0", boolean>;
const k: never = sNested;

declare const sErr: SetAt<[string, Array], "0", boolean>;
const l: never = sErr;
