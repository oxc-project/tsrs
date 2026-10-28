import { A, B, C, D } from '__ENTRY__';
const a: 0 = A.A;
const b: 'B' = B.B;
const c: 0 = C.C;
const d: 'D' = D.D;
// @ts-expect-error preserve literal enum value
const wrong: 'C' = D.D;
// @ts-expect-error missing enum member
A.missing;
