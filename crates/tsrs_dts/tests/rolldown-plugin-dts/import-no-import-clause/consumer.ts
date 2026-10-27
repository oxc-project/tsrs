import type { Foo } from '__ENTRY__';
const foo: Foo = {};
// @ts-expect-error side-effect dependency does not expose unused type
import type { Bar } from '__ENTRY__';
