import type { Foo } from '__ENTRY__';
const foo: Foo = {};
// @ts-expect-error unreferenced module type is not public
import type { Bar } from '__ENTRY__';
