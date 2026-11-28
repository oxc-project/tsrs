// Generic signatures whose parameters have type-variable-free annotations: an instantiated signature keeps the
// original parameter symbols. The errors print the instantiated signatures (names, optionality, rest, readonly,
// tuples, literal unions) through the node builder, and call checks read the parameter types.
declare function f1<T>(x: string, y: T): T;
declare function f2<T>(x?: number | "a" | 1n, ...rest: readonly (string | null)[]): T;
declare function f3<T>(pair: [string, number?], named: [first: boolean, ...more: never[]]): T;
declare function f4<T>(p: (string), q: readonly string[] | undefined, r: unknown, s: object, v: void): T;
declare function f5<T>(x: `a${string}`, y: T[], z: Array<string>): T;
declare function f6<T>(x: string, cb: (y: string, z: T) => void): T;
declare function f7<T extends string>(x: T extends string ? (s: string) => T : never): T;

const g1: number = f1;
const g2: number = f2<string>;
const g3: number = f3<boolean>;
const g4: number = f4<symbol>;
const g5: number = f5<bigint>;
const g6: number = f6<null>;
const g7: number = f7<"k">;
void [g1, g2, g3, g4, g5, g6, g7];

f1<number>(1, 2);
f2<string>("x");
f3<boolean>(["s"], [true, 1]);
f6<null>("s", (y, z) => { const n: number = y; void n; });
f7<"k">((s) => "k");

class C<T> {
    m(x: string, y?: number, ...z: boolean[]): T { return undefined as never; }
    constructor(public a: string, b: readonly [1, 2]) {}
}
const c: number = new C<string>("a", [1, 2]).m;
const d: number = new C<Date>("a", [1, 2]);
void [c, d];

interface I<T> {
    (x: string | number): T;
    new (x: undefined, y: null): T;
    method(this: void, x: never[]): T;
}
declare const i: I<Date>;
const e1: number = i;
const e2: number = i.method;
void [e1, e2];
