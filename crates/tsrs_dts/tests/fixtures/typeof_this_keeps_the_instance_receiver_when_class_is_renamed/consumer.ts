import { Test } from '__ENTRY__';
const test = new Test();
test.functionTwo('x', 1);
// @ts-expect-error preserve receiver member parameter types
test.functionTwo(1, 'x');
