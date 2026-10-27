import { A } from '__ENTRY__';
import type { B } from '__ENTRY__';
const a: A = new A();
const b: B = {};
// @ts-expect-error local class shadows transitive same-name function
A();
