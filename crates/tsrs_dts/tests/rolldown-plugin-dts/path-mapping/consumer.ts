import { Foo } from '__ENTRY__';
Foo();
// @ts-expect-error preserve the mapped function signature
Foo(1);
