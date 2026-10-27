import { C, L, V } from '__ENTRY__';
const literal: 123 = C;
const letValue: number = L;
const varValue: number = V;
// @ts-expect-error C must not widen to a different numeric literal
const wrong: 124 = C;
// @ts-expect-error let declarations retain their numeric annotation
const wrongLet: string = L;
// @ts-expect-error var declarations retain their numeric annotation
const wrongVar: string = V;
