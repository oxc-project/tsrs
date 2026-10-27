import { A, B, C, D, E, F, G, H, I, J, K, L, M, O } from "__ENTRY__";
import type { N } from "__ENTRY__";
const literalString: "string" = A;
const literalNumber: 8 = B;
const voidValue: void = C;
const unknownValue: unknown = D;
const anyValue: { anything: true } = E;
const booleanValue: boolean = F;
const numberValue: number = G;
const stringValue: string = H;
const objectValue: object = I;
const nullValue: null = J;
const undefinedValue: undefined = K;
const symbolValue: symbol = L;
const neverValue: never = M;
const bigintValue: bigint = O;
interface Derived extends N { extra: string }
declare const derived: Derived;
const fluent: Derived = derived.foo();
// @ts-expect-error string literal remains exact
const otherLiteral: "other" = A;
// @ts-expect-error unknown is not assignable without narrowing
const unsafe: string = D;
// @ts-expect-error bigint is distinct from number
const notNumber: number = O;
