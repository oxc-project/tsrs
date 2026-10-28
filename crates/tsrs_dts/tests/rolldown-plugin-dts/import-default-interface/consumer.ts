import type { Foo } from '__ENTRY__';
const foo: Foo = {};
// @ts-expect-error imported default interface is not re-exported
import type { Bar } from '__ENTRY__';
