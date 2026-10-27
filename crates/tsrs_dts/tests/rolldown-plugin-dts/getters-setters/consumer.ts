import { D } from '__ENTRY__';
const d = new D();
d.b = {};
const a: {} = d.a;
const c: {} = d.c;
// @ts-expect-error getter-only property stays readonly
d.a = {};
// @ts-expect-error explicit readonly property stays readonly
d.c = {};
// @ts-expect-error setter parameter type is retained
d.b = null;
