import { fn } from '__ENTRY__';
import type { I } from '__ENTRY__';
const i: I = fn;
const s: string = i('x');
const prop: string = fn.staticProp;
// @ts-expect-error preserve call parameter
fn(1);
// @ts-expect-error preserve static property type
const wrong: number = i.staticProp;
