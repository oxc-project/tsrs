import { C1, C2 } from "__ENTRY__";
const first: x1.I1 = new C1().X;
const second: x2.I2 = new C2().X;
new C1().X = {};
new C2().X = {};
// @ts-expect-error class still exposes X, not an arbitrary field
new C1().missing;
// @ts-expect-error referenced empty interface excludes null in strict mode
new C2().X = null;
