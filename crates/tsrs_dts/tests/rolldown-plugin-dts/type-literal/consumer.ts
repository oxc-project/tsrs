import type { Foo } from "__ENTRY__";
const object: Foo = { a: {} };
// @ts-expect-error required property is retained
const missing: Foo = {};
// @ts-expect-error member interface excludes null
const nullable: Foo = { a: null };
