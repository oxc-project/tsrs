import { A, B } from '__ENTRY__';
const a: 'a' = A.Item();
const b: 'b' = B.Item();
const g: 'a' = Outer.Inner.GlobalItem();
// @ts-expect-error keep namespace members distinct
const wrong: 'a' = B.Item();
