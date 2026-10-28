import type { Foo } from '__ENTRY__';
const foo: Foo = { bar: {} };
// @ts-expect-error alias resolves to retained dependency
const wrong: Foo = {};
// @ts-expect-error local import alias is not re-exported
import type { Baz } from '__ENTRY__';
