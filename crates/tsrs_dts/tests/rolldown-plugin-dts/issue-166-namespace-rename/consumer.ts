import { A, B } from '__ENTRY__';
A.Item();
B.Item();
// @ts-expect-error preserve the member signature
A.Item(1);
// @ts-expect-error preserve the member signature
B.Item(1);
