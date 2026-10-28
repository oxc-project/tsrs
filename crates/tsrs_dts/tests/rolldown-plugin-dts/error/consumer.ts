import { C, D, E, F, G, H } from '__ENTRY__';
import type { A, B } from '__ENTRY__';
const a: A = {};
const b: B = 1;
F();
const g: G = new G();
// @ts-expect-error ambient function arity is preserved
F(1);
