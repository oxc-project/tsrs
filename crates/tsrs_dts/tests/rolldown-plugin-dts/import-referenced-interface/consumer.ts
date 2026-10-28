import type { Foo } from '__ENTRY__';
const foo: Foo = { bar: {} };
// @ts-expect-error retain dependency-bearing required property
const wrong: Foo = {};
// @ts-expect-error retain referenced non-null interface
foo.bar = null;
