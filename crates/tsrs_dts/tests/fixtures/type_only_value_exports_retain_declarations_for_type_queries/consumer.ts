import { fn, value } from '__ENTRY__';
const f: typeof fn = (text: string) => text.length;
const v: typeof value = { tag: 'value' };
// @ts-expect-error keep the function signature
const bad: typeof fn = (text: number) => text;
// @ts-expect-error type-only export cannot be called
fn('x');
// @ts-expect-error type-only export cannot be read as a value
value.tag;
