import type { foo } from '__ENTRY__';
const n: typeof foo = 1;
// @ts-expect-error preserve the exported const type
const bad: typeof foo = 'x';
import * as api from '__ENTRY__';
// @ts-expect-error a type-only star does not expose a value
api.foo;
