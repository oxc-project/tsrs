import Foo from '__ENTRY__';
const foo = new Foo({});
const b: {} = foo.b;
const result: {} = foo.method({});
// @ts-expect-error constructor dependency is required
new Foo();
// @ts-expect-error method parameter remains non-null
foo.method(null);
