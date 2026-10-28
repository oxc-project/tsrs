import type { Foo } from '__ENTRY__';
declare const C: Foo;
new C();
// @ts-expect-error construct signature does not imply callable
C();
// @ts-expect-error preserve constructor arity
new C(1);
