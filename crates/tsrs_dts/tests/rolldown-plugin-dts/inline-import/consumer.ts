import type { Foo } from '__ENTRY__';
const foo: Foo = { bar: {}, baz: 123 };
const literal: 123 = foo.baz;
// @ts-expect-error typeof the imported const retains its literal type
const wrong: Foo = { bar: {}, baz: 124 };
// @ts-expect-error the local dependency is not a public entry export
import type { Bar } from '__ENTRY__';
