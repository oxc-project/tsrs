import { A, B } from '__ENTRY__';
const fromA: typeof B = A();
const fromB: typeof A = B();
const decoratedA: void = A(function () {});
const decoratedB: void = B(function () {});
const descriptorA: PropertyDescriptor = A(B, 'name', { configurable: true });
const descriptorB: PropertyDescriptor = B(A, 'name', { configurable: true });
// @ts-expect-error retain the three-argument overload's property-name type
A(B, 1, {});
// @ts-expect-error retain the one-argument overload's Function constraint
B(1);
// @ts-expect-error unused overload groups must not become public
import { unused } from '__ENTRY__';
