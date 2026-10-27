import type { Derived } from "__ENTRY__";
const good: Derived = { base: "b", derived: "d" };
// @ts-expect-error heritage members must survive tree shaking
const missingBase: Derived = { derived: "d" };
// @ts-expect-error own members must also survive
const missingOwn: Derived = { base: "b" };
