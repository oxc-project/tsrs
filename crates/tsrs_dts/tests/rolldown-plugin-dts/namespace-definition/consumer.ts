import { fn } from '__ENTRY__';
const result: string = fn('input');
const property: string = fn.staticProp;
fn.staticProp = 'changed';
// @ts-expect-error retain the callable signature of the merged declaration
fn(1);
// @ts-expect-error retain the static namespace property's type
fn.staticProp = 1;
