import type { Foo } from "__ENTRY__";
declare const fn: Foo;
const result: {} = fn({}, {});
// @ts-expect-error both function parameters remain required
fn({});
// @ts-expect-error call signature does not imply a constructor signature
new fn({}, {});
