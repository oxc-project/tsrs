import type { Foo } from '__ENTRY__';
const minimum: Foo = [1];
const optional: Foo = [1, 'two'];
const rest: Foo = [1, undefined, true, 42, 'rest'];
// @ts-expect-error first element is required
const missing: Foo = [];
// @ts-expect-error the optional second element is string
const wrong: Foo = [1, 2];
