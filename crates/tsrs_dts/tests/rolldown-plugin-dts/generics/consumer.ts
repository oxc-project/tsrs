import { Cl, fn } from '__ENTRY__';
import type { I1, Ty, TyFn, TyCtor, I2 } from '__ENTRY__';
const first: I1<number> = { a: 1, b: {} };
const type: Ty<string> = { c: 'x', d: {} };
const cls = new Cl<boolean>();
cls.e = true;
const defaultType: I1 = { a: {}, b: {} };
const inherited: I2 = {};
fn<number>(1, {});
declare const genericFn: TyFn;
genericFn<number>(1, {});
declare const ctor: TyCtor;
new ctor<number>(1, {});
// @ts-expect-error preserve generic property substitution
first.a = 'x';
// @ts-expect-error preserve function type argument
fn<number>('x', {});
// @ts-expect-error preserve generic constructor argument
new ctor<number>('x', {});
