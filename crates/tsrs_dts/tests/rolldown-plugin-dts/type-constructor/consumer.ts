import type { Foo } from "__ENTRY__";
declare const Constructor: Foo;
const result: {} = new Constructor({}, {});
// @ts-expect-error both constructor parameters remain required
new Constructor({});
// @ts-expect-error constructor signature does not imply a call signature
Constructor({}, {});
