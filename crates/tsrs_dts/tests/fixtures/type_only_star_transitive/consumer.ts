import * as api from '__ENTRY__';
import type { foo } from '__ENTRY__';
const n: typeof foo = 1;
// @ts-expect-error transitively type-only star stays type-only
api.foo;
