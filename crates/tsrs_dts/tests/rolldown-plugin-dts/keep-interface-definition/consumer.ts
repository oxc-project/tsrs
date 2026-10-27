import type { Foo } from '__ENTRY__';
const foo: Foo = { a: 'a', b: [1, 2] };
const value: number = foo.b[0];
// @ts-expect-error retain the array element type
const wrong: Foo = { a: 'a', b: ['wrong'] };
// @ts-expect-error both properties remain required
const missing: Foo = { a: 'a' };
