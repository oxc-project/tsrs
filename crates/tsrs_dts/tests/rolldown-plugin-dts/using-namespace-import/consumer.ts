import type { Foo } from "__ENTRY__";
const wrapped: Foo = { bar: {} };
const member: Foo["bar"] = wrapped.bar;
// @ts-expect-error namespace reference retains the required bar member
const missing: Foo = {};
// @ts-expect-error referenced Bar remains non-nullable
const wrong: Foo = { bar: null };
// @ts-expect-error namespace dependency is not a direct entry export
import type { Bar } from "__ENTRY__";
