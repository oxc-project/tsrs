import { Foo } from '__ENTRY__';
const instance = new Foo({}, {});
// @ts-expect-error private parameter property remains private
instance.a;
// @ts-expect-error protected parameter property remains protected
instance.b;
// @ts-expect-error private identity rejects structural construction
const fake: Foo = {};
