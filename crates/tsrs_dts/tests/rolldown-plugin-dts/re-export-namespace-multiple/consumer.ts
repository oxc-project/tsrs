import { ns, deep, onlyOne } from "__ENTRY__";
import type { WithA } from "__ENTRY__";
const wrapped: WithA = { a: {} };
const member: ns.A = wrapped.a;
const alias: deep.ns.A = member;
const singleton: onlyOne.A = alias;
ns.B();
new deep.ns.C();
const enumValue: 1 = ns.D.B;
const text: string = ns.E;
const namedType: ns.F = "ok";
// @ts-expect-error namespace function has no parameters
ns.B(1);
// @ts-expect-error onlyOne exposes only A
onlyOne.B();
// @ts-expect-error F remains string
const bad: ns.F = 1;
