import { foo } from '__ENTRY__';
const literal: 'bar' = foo;
// @ts-expect-error export literal must not widen
const wrong: 'baz' = foo;
