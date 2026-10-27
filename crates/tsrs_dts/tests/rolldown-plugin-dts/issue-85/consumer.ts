import { foo } from '__ENTRY__';
const result = foo();
const done: void = result.bar('anything', 1);
// @ts-expect-error the nested function retains both required arguments
result.bar(1);
// @ts-expect-error the outer function has no parameters
foo(1);
