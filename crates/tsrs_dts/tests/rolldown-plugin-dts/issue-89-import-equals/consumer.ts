import { Foo } from '__ENTRY__';
const foo: Foo = {};
// @ts-expect-error import-equals aliases an interface, not a value
const value = Foo;
// @ts-expect-error even the empty interface excludes null under strict checking
const wrong: Foo = null;
