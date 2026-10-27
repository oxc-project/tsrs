import type { Foo } from '__ENTRY__';
const foo: Foo = { bar: { t: 42 } };
const value: number = foo.bar.t;
// @ts-expect-error retain the number generic argument
const wrong: Foo = { bar: { t: 'text' } };
