import { Test } from "__ENTRY__";
const test = new Test();
test.functionOne("x", 1);
test.functionTwo("x", 1);
// @ts-expect-error instance method parameter order is preserved
test.functionTwo(1, "x");
// @ts-expect-error Parameters<typeof this.functionOne> retains arity
test.functionTwo("x");
