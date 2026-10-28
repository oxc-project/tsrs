import { A } from '__ENTRY__';
const system = new A.System({ amount: 1 });
const amount: number = system._obj.amount;
const props: A.Props = { amount: 1 };
// @ts-expect-error retain the Record<string, number> constraint after hoisting
new A.System({ amount: 'wrong' });
// @ts-expect-error Props constrains every property to number
const wrong: A.Props = { amount: 'wrong' };
