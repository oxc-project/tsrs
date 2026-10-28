import type { Foo } from '__ENTRY__';
const foo: Foo = { bar: {} };
// @ts-expect-error the referenced member remains required
const missing: Foo = {};
// @ts-expect-error the empty referenced interface excludes null
const wrong: Foo = { bar: null };
// @ts-expect-error the dependency is private to the entry
import type { Bar } from '__ENTRY__';
