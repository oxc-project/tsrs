import { ns } from '__ENTRY__';
declare const value: ns.A;
const a: {} = value.a;
const b: ns.Shadowed1 = value.b;
const c: ns.Shadowed2 = value.c;
const d: undefined = value.d;
const e: void = value.e();
const child: ns.childNS.ref = {};
const constructed: ns.Shadowed1 = new ns.Shadowed1();
// @ts-expect-error Shadowed3 is the inner undefined alias, not the outer empty interface
const wrong: ns.Shadowed3 = {};
// @ts-expect-error retain the namespace function's signature
ns.Shadowed4(1);
// @ts-expect-error referenced outer interfaces are dependencies, not entry exports
import type { Referenced1 } from '__ENTRY__';
