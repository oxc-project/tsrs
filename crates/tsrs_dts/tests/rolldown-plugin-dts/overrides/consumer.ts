import Foo from '__ENTRY__';
const foo = new Foo({});
const result: {} = foo.method({});
// @ts-expect-error the constructor overloads each require one argument
new Foo();
// @ts-expect-error the method overloads each require one argument
foo.method();
// @ts-expect-error the empty interface arguments exclude null
foo.method(null);
