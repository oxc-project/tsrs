import type { Foo } from '__ENTRY__';
declare const foo: Foo;
const instance = new foo.ns.Bar();
const constructor: new () => object = foo.ns.Bar;
// @ts-expect-error namespace exports remain scoped to ns
foo.ns.Missing;
// @ts-expect-error Bar has a zero-argument constructor
new foo.ns.Bar(1);
