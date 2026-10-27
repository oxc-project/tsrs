import { Foo } from "__ENTRY__";
const instance: Foo = new Foo();
// @ts-expect-error empty constructor has no parameters
new Foo(1);
// @ts-expect-error the supplied Foo declares no members
instance.internal;
