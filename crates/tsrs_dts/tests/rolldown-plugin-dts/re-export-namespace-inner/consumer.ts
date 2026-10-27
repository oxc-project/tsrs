import { outer } from '__ENTRY__';
const ty: outer.inner.Ty = 1;
const value: number = outer.inner.num;
// @ts-expect-error retain the nested numeric type alias
const wrong: outer.inner.Ty = 'wrong';
// @ts-expect-error retain the nested value annotation
const wrongValue: string = outer.inner.num;
// @ts-expect-error inner members must remain under the inner namespace
outer.num;
