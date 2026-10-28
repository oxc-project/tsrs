import type { Foo } from '__ENTRY__';
const foo: Foo = {};
// @ts-expect-error barrel does not expose unrelated type
import type { Bar } from '__ENTRY__';
// @ts-expect-error barrel does not propagate dependency UMD namespace
ns;
