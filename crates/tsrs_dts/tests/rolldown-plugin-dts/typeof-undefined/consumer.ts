import type { T } from "__ENTRY__";
const missing: T = undefined;
const number: T = 123;
const nan: T = NaN;
// @ts-expect-error typeof NaN is number, not string
const string: T = "NaN";
// @ts-expect-error undefined union does not include null
const nullable: T = null;
