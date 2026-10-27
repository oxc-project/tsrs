import { fn, foo, Cls } from '__ENTRY__';
const value: typeof foo = 1;
const f: typeof fn = (text, options) => {};
declare const instance: Cls;
const text: string = instance.foo;
// @ts-expect-error retain the variable type
const bad: typeof foo = 'x';
// @ts-expect-error retain the function parameter type
f(1, {});
// @ts-expect-error retain type-only export restriction
fn('x', {});
// @ts-expect-error retain the variable's type-only export restriction
foo;
// @ts-expect-error retain the class's type-only export restriction
new Cls();
