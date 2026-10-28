import type { Bar } from '__ENTRY__';
const value: Bar = {};
// @ts-expect-error private alias dependency is not an entry export
import type { Foo } from '__ENTRY__';
