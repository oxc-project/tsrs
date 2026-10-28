import Foo from '__ENTRY__';
const foo: Foo = new Foo();
// @ts-expect-error class constructor is not callable
Foo();
