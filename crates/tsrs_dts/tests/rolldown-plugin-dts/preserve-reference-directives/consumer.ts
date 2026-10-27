import { a, b } from '__ENTRY__';
const tags: Array<keyof JSX.IntrinsicElements> = [...a, ...b];
// @ts-expect-error intrinsic element keys are not numbers
const wrong: number[] = a;
