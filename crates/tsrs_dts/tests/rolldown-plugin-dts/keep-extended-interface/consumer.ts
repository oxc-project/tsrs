import type { Foo } from '__ENTRY__';
const foo: Foo = {};
// @ts-expect-error an empty inherited interface still excludes null
const wrong: Foo = null;
// @ts-expect-error the base interface is a retained private dependency
import type { Bar } from '__ENTRY__';
